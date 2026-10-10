//! # Machine Core (`machine-core`)
//!
//! Universal Zero-Cost State Machine Kernel unifying:
//! - **1-Way Control Flow**: [`Generator`] & [`GenIter`] (projections into `core::iter::Iterator`).
//! - **2-Way Control Flow**: [`Effect`] and synchronous drivers.
//! - **Kernel FSM**: [`Machine`] and [`Step`].
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

// ============================================================================
// 1. UNIVERSAL KERNEL PRIMITIVES
// ============================================================================

/// The outcome of an FSM transition step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step<Y, R> {
    /// Machine yielded intermediate control / payload.
    Yielded(Y),
    /// Machine completed execution with terminal return value.
    Done(R),
}

/// Universal Finite State Machine Kernel contract.
pub trait Machine {
    /// Payload emitted at suspension points.
    type Yield;
    /// Payload accepted on transition resumption.
    type Resume;
    /// Terminal value returned upon completion.
    type Return;

    /// Advance the FSM one transition step using the supplied resumption value.
    ///
    /// # Invariants
    /// - Calling `step` after returning `Step::Done` is a contract violation (poisoned/terminal state).
    fn step(&mut self, resume: Self::Resume) -> Step<Self::Yield, Self::Return>;

    /// Optional synchronous cleanup. Invoked on early cancellation or abort.
    #[inline]
    fn close(&mut self) {}
}

// ============================================================================
// 2. 1-WAY PROJECTION: GENERATORS & ITERATORS
// ============================================================================

/// A 1-way generator whose resumption payload is the Zero-Sized Type `()`.
pub trait Generator: Machine<Resume = ()> {}
impl<T: ?Sized + Machine<Resume = ()>> Generator for T {}

/// Zero-cost Iterator adapter over a 1-way Generator.
///
/// Ensures the FSM single-return invariant: once [`Step::Done`] is returned,
/// the machine is never polled again.
pub struct GenIter<M> {
    machine: M,
    is_done: bool,
}

impl<M> GenIter<M> {
    /// Constructs a new iterator adapter wrapping a generator.
    #[inline(always)]
    pub const fn new(machine: M) -> Self {
        Self {
            machine,
            is_done: false,
        }
    }
}

impl<M: Generator> core::iter::Iterator for GenIter<M> {
    type Item = M::Yield;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.is_done {
            return None;
        }

        match self.machine.step(()) {
            Step::Yielded(val) => Some(val),
            Step::Done(_) => {
                self.is_done = true;
                None
            }
        }
    }
}

impl<M: Generator> core::iter::FusedIterator for GenIter<M> {}

// ============================================================================
// 3. 2-WAY PROJECTION: ALGEBRAIC EFFECTS
// ============================================================================

/// Base trait defining an algebraic effect request.
pub trait Effect {
    /// Statically typed response expected from the effect handler.
    type Response;
}

/// Input protocol for driving two-way effect state machines.
///
/// Distinguishes the first initialization turn (where no prior effect occurred)
/// from subsequent resumptions with responses, or explicit cancellation/abort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeSignal<R> {
    /// Initial startup step.
    Start,
    /// Resumption with the environment's response to the prior effect.
    Resume(R),
    /// The environment aborted the operation early.
    Abort,
}

/// Synchronously drives an effect machine to completion using a handler closure.
///
/// Calls [`Machine::close`] deterministically if the handler aborts (returns `None`).
#[inline(always)]
pub fn handle_sync<M, E, H>(mut machine: M, mut handler: H) -> Option<M::Return>
where
    E: Effect,
    M: Machine<Yield = E, Resume = ResumeSignal<E::Response>>,
    H: FnMut(E) -> Option<E::Response>,
{
    let mut signal = ResumeSignal::Start;
    loop {
        match machine.step(signal) {
            Step::Yielded(eff) => {
                match handler(eff) {
                    Some(resp) => {
                        signal = ResumeSignal::Resume(resp);
                    }
                    None => {
                        // Notify machine of early abort, then close
                        let abort_step = machine.step(ResumeSignal::Abort);
                        debug_assert!(
                            matches!(abort_step, Step::Done(_)),
                            "machine yielded effect on abort signal rather than terminating"
                        );
                        machine.close();
                        return None;
                    }
                }
            }
            Step::Done(ret) => return Some(ret),
        }
    }
}

// ============================================================================
// 4. EXTENSION METHODS (DX ERGONOMICS)
// ============================================================================

/// Ergonomic extension methods for state machines.
pub trait MachineExt: Machine {
    /// Converts a 1-way generator into a fused `core::iter::Iterator`.
    #[inline(always)]
    fn into_iter(self) -> GenIter<Self>
    where
        Self: Sized + Generator,
    {
        GenIter::new(self)
    }
}

impl<M: ?Sized + Machine> MachineExt for M {}

/// Common prelude for `machine-core`.
pub mod prelude {
    pub use crate::{
        Effect, GenIter, Generator, Machine, MachineExt, ResumeSignal, Step, handle_sync,
    };
}

#[cfg(test)]
pub mod fixtures {
    use super::prelude::*;

    pub struct RangeGen {
        pub curr: u64,
        pub max: u64,
        pub polled_after_done: bool,
    }

    impl RangeGen {
        pub const fn new(start: u64, max: u64) -> Self {
            Self {
                curr: start,
                max,
                polled_after_done: false,
            }
        }
    }

    impl Machine for RangeGen {
        type Yield = u64;
        type Resume = ();
        type Return = ();

        fn step(&mut self, _resume: ()) -> Step<Self::Yield, Self::Return> {
            if self.polled_after_done {
                panic!("Contract violation: RangeGen polled after Done");
            }

            if self.curr < self.max {
                let val = self.curr;
                self.curr += 1;
                Step::Yielded(val)
            } else {
                self.polled_after_done = true;
                Step::Done(())
            }
        }
    }

    pub struct GreetEffect {
        pub prompt: &'static str,
    }
    impl Effect for GreetEffect {
        type Response = &'static str;
    }

    pub enum GreetState {
        Init,
        WaitingName,
        Done,
        Aborted,
    }

    pub struct GreetFSM<'a> {
        pub state: GreetState,
        pub close_counter: Option<&'a core::sync::atomic::AtomicUsize>,
    }

    impl Default for GreetFSM<'_> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<'a> GreetFSM<'a> {
        pub const fn new() -> Self {
            Self {
                state: GreetState::Init,
                close_counter: None,
            }
        }

        pub const fn with_close_counter(counter: &'a core::sync::atomic::AtomicUsize) -> Self {
            Self {
                state: GreetState::Init,
                close_counter: Some(counter),
            }
        }
    }

    impl Machine for GreetFSM<'_> {
        type Yield = GreetEffect;
        type Resume = ResumeSignal<&'static str>;
        type Return = &'static str;

        fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
            match self.state {
                GreetState::Init => {
                    self.state = GreetState::WaitingName;
                    Step::Yielded(GreetEffect { prompt: "Name?" })
                }
                GreetState::WaitingName => match signal {
                    ResumeSignal::Resume(name) => {
                        self.state = GreetState::Done;
                        Step::Done(name)
                    }
                    ResumeSignal::Abort => {
                        self.state = GreetState::Aborted;
                        Step::Done("aborted")
                    }
                    ResumeSignal::Start => {
                        self.state = GreetState::Aborted;
                        Step::Done("protocol_error")
                    }
                },
                GreetState::Done | GreetState::Aborted => {
                    panic!("Contract violation: GreetFSM polled after terminal state");
                }
            }
        }

        fn close(&mut self) {
            if let Some(counter) = self.close_counter {
                counter.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::prelude::*;

    #[test]
    fn test_generator_to_iterator_projection_never_double_polls() {
        let generator = RangeGen::new(1, 4);
        let mut iter = generator.into_iter();

        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(3));
        assert_eq!(iter.next(), None);
        // Trailing None calls must NOT poll the underlying generator
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_two_way_effect_dispatch_success() {
        let machine = GreetFSM::new();
        let result = handle_sync(machine, |eff| {
            assert_eq!(eff.prompt, "Name?");
            Some("Arthur")
        });
        assert_eq!(result, Some("Arthur"));
    }

    #[test]
    fn test_two_way_effect_abort_triggers_close() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        let counter = AtomicUsize::new(0);
        let machine = GreetFSM::with_close_counter(&counter);

        let result = handle_sync(machine, |_eff| None);
        assert_eq!(result, None);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
}
