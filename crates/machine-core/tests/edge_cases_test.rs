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
