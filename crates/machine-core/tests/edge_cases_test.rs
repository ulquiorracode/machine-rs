use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use machine_core::prelude::*;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

// ============================================================================
// 1. IMMEDIATE DONE (EMPTY GENERATOR & ZERO-STEP EFFECT MACHINE)
// ============================================================================

struct EmptyGen {
    step_calls: usize,
}

impl Machine for EmptyGen {
    type Yield = u64;
    type Resume = ();
    type Return = &'static str;

    fn step(&mut self, _resume: ()) -> Step<Self::Yield, Self::Return> {
        self.step_calls += 1;
        if self.step_calls > 1 {
            panic!("Contract violation: EmptyGen polled after Done");
        }
        Step::Done("immediate_empty")
    }
}

#[test]
fn test_empty_generator_fused_invariants() {
    let generator = EmptyGen { step_calls: 0 };
    let mut iter = generator.into_iter();

    // First poll should be None immediately
    assert_eq!(iter.next(), None);
    // Trailing polls must never invoke step again
    for _ in 0..10 {
        assert_eq!(iter.next(), None);
    }
}

struct ImmediateDoneEffectMachine;

impl Machine for ImmediateDoneEffectMachine {
    type Yield = DummyEffect;
    type Resume = ResumeSignal<u32>;
    type Return = u32;

    fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
        assert_eq!(signal, ResumeSignal::Start);
        Step::Done(42)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct DummyEffect;
impl Effect for DummyEffect {
    type Response = u32;
}

#[test]
fn test_effect_immediate_done_on_start() {
    let mut handler_invoked = false;
    let res = handle_sync(ImmediateDoneEffectMachine, |_eff| {
        handler_invoked = true;
        Some(100)
    });

    assert_eq!(res, Some(42));
    assert!(
        !handler_invoked,
        "Handler must not be invoked if machine terminates on Start"
    );
}

// ============================================================================
// 2. MULTI-STEP HETEROGENEOUS EFFECT PIPELINE (10 STEPS)
// ============================================================================

enum StepEffect {
    Ping(usize),
}

impl Effect for StepEffect {
    type Response = usize;
}

struct TenStepMachine {
    current_step: usize,
    max_steps: usize,
}

impl Machine for TenStepMachine {
    type Yield = StepEffect;
    type Resume = ResumeSignal<usize>;
    type Return = usize;

    fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
        match signal {
            ResumeSignal::Start => {
                assert_eq!(self.current_step, 0);
                self.current_step += 1;
                Step::Yielded(StepEffect::Ping(self.current_step))
            }
            ResumeSignal::Resume(resp) => {
                assert_eq!(resp, self.current_step * 10);
                if self.current_step < self.max_steps {
                    self.current_step += 1;
                    Step::Yielded(StepEffect::Ping(self.current_step))
                } else {
                    Step::Done(self.current_step)
                }
            }
            ResumeSignal::Abort => {
                panic!("Did not expect abort in success path");
            }
        }
    }
}

#[test]
fn test_multi_step_sequential_effect_chain() {
    let machine = TenStepMachine {
        current_step: 0,
        max_steps: 10,
    };

    let mut dispatches = 0;
    let final_result = handle_sync(machine, |eff| {
        let StepEffect::Ping(seq) = eff;
        dispatches += 1;
        Some(seq * 10)
    });

    assert_eq!(dispatches, 10);
    assert_eq!(final_result, Some(10));
}

// ============================================================================
// 3. PANIC & UNWIND SAFETY (RAII DROP VERIFICATION)
// ============================================================================

struct DropGuardMachine {
    dropped: Arc<AtomicBool>,
}

impl Drop for DropGuardMachine {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

impl Machine for DropGuardMachine {
    type Yield = DummyEffect;
    type Resume = ResumeSignal<u32>;
    type Return = ();

    fn step(&mut self, _signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
        Step::Yielded(DummyEffect)
    }
}

#[test]
fn test_handler_panic_unwind_preserves_machine_raii_drop() {
    let dropped = Arc::new(AtomicBool::new(false));
    let machine = DropGuardMachine {
        dropped: dropped.clone(),
    };

    let panic_result = catch_unwind(AssertUnwindSafe(|| {
        handle_sync(machine, |_eff| {
            panic!("Intentional panic inside effect handler");
        });
    }));

    assert!(panic_result.is_err(), "Handler panic should propagate");
    assert!(
        dropped.load(Ordering::SeqCst),
        "Machine must be deterministically dropped even when handler panics during unwind"
    );
}

// ============================================================================
// 4. MULTI-ABORT & IDEMPOTENT CLOSE
// ============================================================================

struct CloseCountingMachine {
    close_count: Arc<AtomicUsize>,
    aborted: bool,
}

impl Machine for CloseCountingMachine {
    type Yield = DummyEffect;
    type Resume = ResumeSignal<u32>;
    type Return = ();

    fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
        match signal {
            ResumeSignal::Start => Step::Yielded(DummyEffect),
            ResumeSignal::Abort => {
                self.aborted = true;
                Step::Done(())
            }
            ResumeSignal::Resume(_) => Step::Yielded(DummyEffect),
        }
    }

    fn close(&mut self) {
        self.close_count.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn test_abort_with_explicit_close_calls() {
    let close_count = Arc::new(AtomicUsize::new(0));
    let mut machine = CloseCountingMachine {
        close_count: close_count.clone(),
        aborted: false,
    };

    // First abort step
    let step = machine.step(ResumeSignal::Abort);
    assert_eq!(step, Step::Done(()));
    machine.close();
    assert_eq!(close_count.load(Ordering::SeqCst), 1);

    // Idempotent secondary close call
    machine.close();
    assert_eq!(close_count.load(Ordering::SeqCst), 2);
}

// ============================================================================
// 5. OPEN EFFECT SUMS (EITHER) COMPOSITION TESTS
// ============================================================================

struct ReadEffect(&'static str);
impl Effect for ReadEffect {
    type Response = String;
}

struct WriteEffect(&'static str, usize);
impl Effect for WriteEffect {
    type Response = bool;
}

enum CompositeStepState {
    Start,
    Wrote,
    Done,
}

struct CompositeMachine {
    state: CompositeStepState,
}

impl Machine for CompositeMachine {
    type Yield = Either<ReadEffect, WriteEffect>;
    type Resume = ResumeSignal<Either<String, bool>>;
    type Return = usize;

    fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
        match self.state {
            CompositeStepState::Start => {
                self.state = CompositeStepState::Wrote;
                Step::Yielded(Either::Left(ReadEffect("config.toml")))
            }
            CompositeStepState::Wrote => match signal {
                ResumeSignal::Resume(Either::Left(data)) => {
                    self.state = CompositeStepState::Done;
                    Step::Yielded(Either::Right(WriteEffect("cache.bin", data.len())))
                }
                _ => Step::Done(0),
            },
            CompositeStepState::Done => match signal {
                ResumeSignal::Resume(Either::Right(success)) => {
                    Step::Done(if success { 42 } else { 0 })
                }
                _ => Step::Done(0),
            },
        }
    }
}

#[test]
fn test_either_coproduct_dispatch_with_combined_handlers() {
    let machine = CompositeMachine {
        state: CompositeStepState::Start,
    };

    let read_handler = |eff: ReadEffect| -> Option<String> {
        assert_eq!(eff.0, "config.toml");
        Some("data_payload".to_string())
    };

    let write_handler = |eff: WriteEffect| -> Option<bool> {
        assert_eq!(eff.0, "cache.bin");
        assert_eq!(eff.1, 12);
        Some(true)
    };

    // Use fluent .or_else() chaining
    let combined = read_handler.or_else(write_handler);
    let result = handle_sync(machine, combined);

    assert_eq!(result, Some(42));
}

// 3-way nested coproduct verifying type-level Peano CoproductInject without nested constructor brackets
struct LogEffect(&'static str);
impl Effect for LogEffect {
    type Response = ();
}

struct MetricEffect(&'static str, f64);
impl Effect for MetricEffect {
    type Response = ();
}

struct AlertEffect(&'static str);
impl Effect for AlertEffect {
    type Response = bool;
}

type ThreeWayCoproduct = Either<LogEffect, Either<MetricEffect, Either<AlertEffect, Nil>>>;

#[test]
fn test_peano_coproduct_inject_without_nested_brackets() {
    // 1. Inject into Left (first level)
    let eff1: ThreeWayCoproduct = LogEffect("hello").inject();
    assert!(matches!(eff1, Either::Left(_)));

    // 2. Inject into Right -> Left (second level) without writing Either::Right(Either::Left(...))
    let eff2: ThreeWayCoproduct = MetricEffect("cpu", 99.5).inject();
    assert!(matches!(eff2, Either::Right(Either::Left(_))));

    // 3. Inject into Right -> Right -> Left (third level)
    let eff3: ThreeWayCoproduct = AlertEffect("overheating").inject();
    assert!(matches!(
        eff3,
        Either::Right(Either::Right(Either::Left(_)))
    ));
}

#[test]
fn test_flat_match_coproduct_macro() {
    let eff: ThreeWayCoproduct = MetricEffect("fps", 144.0).inject();

    let matched_kind = match_coproduct!(eff,
        LogEffect(msg) => format!("log: {}", msg),
        MetricEffect(key, val) => format!("metric: {}={}", key, val),
        AlertEffect(alert) => format!("alert: {}", alert),
    );

    assert_eq!(matched_kind, "metric: fps=144");
}
