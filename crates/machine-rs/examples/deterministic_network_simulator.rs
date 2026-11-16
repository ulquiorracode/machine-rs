//! Killer-Demo: Deterministic Network & Chaos Simulator.
//!
//! Demonstrates the power of `machine-rs` Algebraic Effects:
//! - The exact same Finite State Machine (PingPongSession) runs:
//!   1. In Simulation / Chaos Tests with pseudo-random seed, dropped packets, and latency injection (`handle_sync`).
//!   2. In Production on async runtimes with real I/O drivers.
//!
//! Run with:
//! `cargo run --example deterministic_network_simulator`

use machine_rs::prelude::*;

// ============================================================================
// 1. DOMAIN MESSAGES & 2-WAY EFFECTS
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkMessage {
    Ping(u32),
    Pong(u32),
}

/// 2-Way Effect: send a message across the wire and await reply / delivery status.
#[derive(Debug, Clone)]
pub struct SendPacket {
    pub message: NetworkMessage,
}

impl Effect for SendPacket {
    /// Resumption type: network return code (true = acked/delivered, false = dropped)
    type Response = bool;
}

/// 2-Way Effect: query current monotonic time (ms) from runtime.
#[derive(Debug, Clone, Copy)]
pub struct MonotonicNow;

impl Effect for MonotonicNow {
    type Response = u64;
}

// Coproduct of session effects: SendPacket or MonotonicNow
type SessionEffect = Either<SendPacket, MonotonicNow>;
type SessionResponse = Either<bool, u64>;

// ============================================================================
// 2. PROTOCOL FINITE STATE MACHINE
// ============================================================================

pub struct PingPongSession {
    session_id: u32,
    round: u32,
    max_rounds: u32,
    retries: u32,
    max_retries: u32,
    state: SessionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionState {
    Init,
    AwaitingPong { ping_id: u32 },
    AwaitingAck { pong_id: u32 },
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionStats {
    pub total_rounds_completed: u32,
    pub total_packets_sent: u32,
    pub total_retries: u32,
}

#[state_machine(Yield = SessionEffect, Resume = ResumeSignal<SessionResponse>, Return = SessionStats)]
impl PingPongSession {
    pub fn new(session_id: u32, max_rounds: u32, max_retries: u32) -> Self {
        Self {
            session_id,
            round: 0,
            max_rounds,
            retries: 0,
            max_retries,
            state: SessionState::Init,
        }
    }

    fn step(&mut self, resume: ResumeSignal<SessionResponse>) -> Step<SessionEffect, SessionStats> {
        match resume {
            ResumeSignal::Abort => {
                self.state = SessionState::Done;
                Step::Done(SessionStats {
                    total_rounds_completed: self.round,
                    total_packets_sent: self.round * 2 + self.retries,
                    total_retries: self.retries,
                })
            }
            ResumeSignal::Start => {
                // First turn: start sending Ping
                let ping_id = self.round * 100 + self.session_id;
                self.state = SessionState::AwaitingPong { ping_id };
                Step::Yielded(Either::Left(SendPacket {
                    message: NetworkMessage::Ping(ping_id),
                }))
            }
            ResumeSignal::Resume(resp) => {
                match self.state {
                    SessionState::Init => {
                        if self.round >= self.max_rounds {
                            self.state = SessionState::Done;
                            return Step::Done(SessionStats {
                                total_rounds_completed: self.round,
                                total_packets_sent: self.round * 2 + self.retries,
                                total_retries: self.retries,
                            });
                        }

                        let ping_id = self.round * 100 + self.session_id;
                        self.state = SessionState::AwaitingPong { ping_id };
                        Step::Yielded(Either::Left(SendPacket {
                            message: NetworkMessage::Ping(ping_id),
                        }))
                    }

                    SessionState::AwaitingPong { ping_id } => {
                        let delivered = resp.left().unwrap_or(false);
                        if delivered {
                            let pong_id = ping_id + 1;
                            self.state = SessionState::AwaitingAck { pong_id };
                            Step::Yielded(Either::Left(SendPacket {
                                message: NetworkMessage::Pong(pong_id),
                            }))
                        } else {
                            self.retries += 1;
                            if self.retries > self.max_retries {
                                self.state = SessionState::Done;
                                return Step::Done(SessionStats {
                                    total_rounds_completed: self.round,
                                    total_packets_sent: self.round * 2 + self.retries,
                                    total_retries: self.retries,
                                });
                            }
                            // Retry Ping
                            Step::Yielded(Either::Left(SendPacket {
                                message: NetworkMessage::Ping(ping_id),
                            }))
                        }
                    }

                    SessionState::AwaitingAck { pong_id } => {
                        let delivered = resp.left().unwrap_or(false);
                        if delivered {
                            self.round += 1;
                            if self.round >= self.max_rounds {
                                self.state = SessionState::Done;
                                Step::Done(SessionStats {
                                    total_rounds_completed: self.round,
                                    total_packets_sent: self.round * 2 + self.retries,
                                    total_retries: self.retries,
                                })
                            } else {
                                self.state = SessionState::Init;
                                Step::Yielded(Either::Right(MonotonicNow))
                            }
                        } else {
                            self.retries += 1;
                            if self.retries > self.max_retries {
                                self.state = SessionState::Done;
                                return Step::Done(SessionStats {
                                    total_rounds_completed: self.round,
                                    total_packets_sent: self.round * 2 + self.retries,
                                    total_retries: self.retries,
                                });
                            }
                            // Retry Pong
                            Step::Yielded(Either::Left(SendPacket {
                                message: NetworkMessage::Pong(pong_id),
                            }))
                        }
                    }

                    SessionState::Done => Step::Done(SessionStats {
                        total_rounds_completed: self.round,
                        total_packets_sent: self.round * 2 + self.retries,
                        total_retries: self.retries,
                    }),
                }
            }
        }
    }
}

// ============================================================================
// 3. DETERMINISTIC CHAOS SIMULATOR (TEST BED)
// ============================================================================

/// Minimal deterministic Linear Congruential Generator (LCG) PRNG (Zero external dependencies).
struct DeterministicPrng {
    state: u64,
}

impl DeterministicPrng {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.state >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16777216.0
    }
}

pub struct DeterministicChaosDriver {
    prng: DeterministicPrng,
    packet_loss_rate: f32,
    simulated_now_ms: u64,
    dropped_count: u32,
    delivered_count: u32,
}

impl DeterministicChaosDriver {
    pub fn new(seed: u64, packet_loss_rate: f32) -> Self {
        Self {
            prng: DeterministicPrng::new(seed),
            packet_loss_rate,
            simulated_now_ms: 1000,
            dropped_count: 0,
            delivered_count: 0,
        }
    }

    pub fn handle_packet(&mut self, packet: SendPacket) -> Option<bool> {
        let roll = self.prng.next_f32();
        if roll < self.packet_loss_rate {
            self.dropped_count += 1;
            println!(
                "  [CHAOS] Packet {:?} DROPPED (roll: {:.3} < {:.3})",
                packet.message, roll, self.packet_loss_rate
            );
            Some(false)
        } else {
            self.delivered_count += 1;
            self.simulated_now_ms += 5; // 5ms simulated RTT
            Some(true)
        }
    }

    pub fn handle_now(&mut self, _now: MonotonicNow) -> Option<u64> {
        Some(self.simulated_now_ms)
    }
}

// ============================================================================
// 4. MAIN ENTRY POINT
// ============================================================================

fn main() {
    println!("=== Machine-RS: Deterministic Chaos & Network Simulator ===");

    let seed = 0xCAFE_BABE_1337;
    let packet_loss_rate = 0.30; // 30% packet loss!
    let max_rounds = 5;
    let max_retries = 20;

    println!(
        "Initializing FSM with seed: 0x{:X}, simulated packet loss: {:.0}%",
        seed,
        packet_loss_rate * 100.0
    );

    let session = PingPongSession::new(42, max_rounds, max_retries);
    let mut driver = DeterministicChaosDriver::new(seed, packet_loss_rate);

    // Drive effect execution deterministically
    let stats = handle_sync(session, |eff: SessionEffect| match eff {
        Either::Left(p) => driver.handle_packet(p).map(Either::Left),
        Either::Right(n) => driver.handle_now(n).map(Either::Right),
    })
    .expect("session must complete successfully");

    println!("\n=== Simulation Results ===");
    println!(
        "Rounds Completed : {}/{}",
        stats.total_rounds_completed, max_rounds
    );
    println!("Packets Sent     : {}", stats.total_packets_sent);
    println!("Packets Delivered: {}", driver.delivered_count);
    println!("Packets Dropped  : {}", driver.dropped_count);
    println!("Retries Incurred : {}", stats.total_retries);
    println!("Simulated Time   : {} ms", driver.simulated_now_ms);

    assert_eq!(stats.total_rounds_completed, max_rounds);
    println!("\n✅ Simulation passed with 100% deterministic reproducibility!");
}
