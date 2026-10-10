use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use machine_rs::prelude::*;

// ============================================================================
// 1. MACHINE-RS 2-WAY EFFECT FSM
// ============================================================================

enum StorageEffect {
    Read(u64),
    Write(u64, u64),
}

impl Effect for StorageEffect {
    type Response = u64;
}

enum FsmState {
    Init,
    ReadSent,
    WriteSent,
    Done,
}

struct TransactionFsm {
    state: FsmState,
    key: u64,
    delta: u64,
}

impl TransactionFsm {
    pub const fn new(key: u64, delta: u64) -> Self {
        Self {
            state: FsmState::Init,
            key,
            delta,
        }
    }
}

impl Machine for TransactionFsm {
    type Yield = StorageEffect;
    type Resume = ResumeSignal<u64>;
    type Return = u64;

    #[inline(always)]
    fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
        match self.state {
            FsmState::Init => {
                self.state = FsmState::ReadSent;
                Step::Yielded(StorageEffect::Read(self.key))
            }
            FsmState::ReadSent => match signal {
                ResumeSignal::Resume(curr_val) => {
                    self.state = FsmState::WriteSent;
                    Step::Yielded(StorageEffect::Write(self.key, curr_val + self.delta))
                }
                _ => Step::Done(0),
            },
            FsmState::WriteSent => match signal {
                ResumeSignal::Resume(new_val) => {
                    self.state = FsmState::Done;
                    Step::Done(new_val)
                }
                _ => Step::Done(0),
            },
            FsmState::Done => Step::Done(0),
        }
    }
}

// ============================================================================
// 2. DYN TRAIT (VTABLE INDIRECTION)
// ============================================================================

trait StorageService {
    fn read(&mut self, key: u64) -> u64;
    fn write(&mut self, key: u64, val: u64) -> u64;
}

struct FastInMemoryStorage;
impl StorageService for FastInMemoryStorage {
    #[inline(always)]
    fn read(&mut self, key: u64) -> u64 {
        key * 2
    }

    #[inline(always)]
    fn write(&mut self, key: u64, val: u64) -> u64 {
        key + val
    }
}

#[inline(never)]
fn run_dyn_box_transaction(storage: &mut Box<dyn StorageService>, key: u64, delta: u64) -> u64 {
    let curr = storage.read(key);
    storage.write(key, curr + delta)
}

#[inline(never)]
fn run_dyn_ref_transaction(storage: &mut dyn StorageService, key: u64, delta: u64) -> u64 {
    let curr = storage.read(key);
    storage.write(key, curr + delta)
}

// ============================================================================
// BENCHMARK SUITE
// ============================================================================

fn bench_sync_effect_vs_dyn(c: &mut Criterion) {
    let mut group = c.benchmark_group("sync_effect_vs_dyn_polymorphism");

    let iters: u64 = 10_000;

    group.bench_function(
        BenchmarkId::new("machine_rs_monomorphic_effect", iters),
        |b| {
            b.iter(|| {
                let mut total = 0u64;
                for i in 0..iters {
                    let fsm = TransactionFsm::new(black_box(i), black_box(10));
                    let res = handle_sync(fsm, |eff| match eff {
                        StorageEffect::Read(k) => Some(k * 2),
                        StorageEffect::Write(k, v) => Some(k + v),
                    });
                    total = total.wrapping_add(res.unwrap_or(0));
                }
                black_box(total)
            });
        },
    );

    group.bench_function(BenchmarkId::new("dyn_trait_ref_vtable", iters), |b| {
        let mut storage = FastInMemoryStorage;
        let storage_ref: &mut dyn StorageService = &mut storage;
        b.iter(|| {
            let mut total = 0u64;
            for i in 0..iters {
                let res = run_dyn_ref_transaction(storage_ref, black_box(i), black_box(10));
                total = total.wrapping_add(res);
            }
            black_box(total)
        });
    });

    group.bench_function(BenchmarkId::new("dyn_trait_box_vtable", iters), |b| {
        let mut storage: Box<dyn StorageService> = Box::new(FastInMemoryStorage);
        b.iter(|| {
            let mut total = 0u64;
            for i in 0..iters {
                let res = run_dyn_box_transaction(&mut storage, black_box(i), black_box(10));
                total = total.wrapping_add(res);
            }
            black_box(total)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_sync_effect_vs_dyn);
criterion_main!(benches);
