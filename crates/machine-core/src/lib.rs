//! # Machine Core (`machine-core`)
//!
//! Universal Zero-Cost State Machine Kernel unifying:
//! - **1-Way Control Flow**: [`Generator`] & [`GenIter`] (projections into `core::iter::Iterator`).
//! - **2-Way Control Flow**: [`Effect`], [`Request`], and Open Algebraic Effects (`Either`).
//! - **Kernel FSM**: [`Machine`], [`Step`], and [`FusedMachine`].

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

// ============================================================================
// 1. UNIVERSAL KERNEL PRIMITIVES
// ============================================================================

/// The outcome of an FSM transition step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Step<Y, R> {
    /// Machine yielded intermediate control / payload.
    Yielded(Y) = 0,
    /// Machine completed execution with terminal return value.
    Done(R) = 1,
}

impl<Y, R> Step<Y, R> {
    /// True if the step yielded intermediate payload.
    #[inline(always)]
    pub const fn is_yielded(&self) -> bool {
        matches!(self, Self::Yielded(_))
    }

    /// True if the machine has reached terminal completion.
    #[inline(always)]
    pub const fn is_done(&self) -> bool {
        matches!(self, Self::Done(_))
    }

    /// Maps the yielded payload with a transformation closure.
    #[inline(always)]
    pub fn map_yield<U, F: FnOnce(Y) -> U>(self, f: F) -> Step<U, R> {
        match self {
            Self::Yielded(y) => Step::Yielded(f(y)),
            Self::Done(r) => Step::Done(r),
        }
    }

    /// Maps the terminal completion value with a transformation closure.
    #[inline(always)]
    pub fn map_done<U, F: FnOnce(R) -> U>(self, f: F) -> Step<Y, U> {
        match self {
            Self::Yielded(y) => Step::Yielded(y),
            Self::Done(r) => Step::Done(f(r)),
        }
    }
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
    /// - Calling `step` after returning `Step::Done` is a contract violation (poisoned state).
    fn step(&mut self, resume: Self::Resume) -> Step<Self::Yield, Self::Return>;

    /// Optional synchronous cleanup. Transitions the machine to terminal state.
    #[inline]
    fn close(&mut self) {}
}

/// Marker trait for machines guaranteeing idempotent termination (safe double-polling).
pub trait FusedMachine: Machine {}

// ============================================================================
// 2. 1-WAY PROJECTION: GENERATORS & ITERATORS
// ============================================================================

/// A 1-way generator whose resumption payload is the Zero-Sized Type `()`.
pub trait Generator: Machine<Resume = ()> {}
impl<T: ?Sized + Machine<Resume = ()>> Generator for T {}

/// Zero-cost Iterator adapter over a 1-way Generator.
#[repr(transparent)]
pub struct GenIter<M>(pub M);

impl<M: Generator> core::iter::Iterator for GenIter<M> {
    type Item = M::Yield;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        match self.0.step(()) {
            Step::Yielded(val) => Some(val),
            Step::Done(_) => None,
        }
    }
}

impl<M: Generator + FusedMachine> core::iter::FusedIterator for GenIter<M> {}

// ============================================================================
// 3. 2-WAY PROJECTION: OPEN ALGEBRAIC EFFECTS (TYPED INVERTED SLOTS)
// ============================================================================

/// Base trait defining an algebraic effect request.
pub trait Effect {
    /// Statically typed response expected from the effect handler.
    type Response;
}

/// Affine continuation token linking an effect request to its typed response slot.
pub struct Request<'a, E: Effect> {
    pub effect: E,
    slot: &'a mut Option<E::Response>,
}

impl<'a, E: Effect> Request<'a, E> {
    /// Constructs a new effect request binding the target response slot.
    #[inline(always)]
    pub fn new(effect: E, slot: &'a mut Option<E::Response>) -> Self {
        Self { effect, slot }
    }

    /// Complete the effect resumption with the statically typed response.
    #[inline(always)]
    pub fn respond(self, response: E::Response) {
        *self.slot = Some(response);
    }
}

/// Coproduct open sum for composing independent effects without monolithic closed enums.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L: Effect, R: Effect> Effect for Either<L, R> {
    type Response = Either<L::Response, R::Response>;
}

// ============================================================================
// 4. SYNCHRONOUS EFFECT DRIVERS & MACROS
// ============================================================================

/// Synchronously drives an effect machine yielding [`Request`] tokens.
#[inline(always)]
pub fn handle_sync<M, E, H>(mut machine: M, mut handler: H) -> Option<M::Return>
where
    E: Effect,
    M: Machine<Yield = E, Resume = Option<E::Response>>,
    H: FnMut(E) -> Option<E::Response>,
{
    let mut resume_val = None;
    loop {
        match machine.step(resume_val.take()) {
            Step::Yielded(eff) => {
                let resp = handler(eff);
                if resp.is_none() {
                    machine.close();
                    return None;
                }
                resume_val = resp;
            }
            Step::Done(ret) => return Some(ret),
        }
    }
}

/// Declarative pattern matching macro for driving machines and effect steps.
#[macro_export]
macro_rules! machine_match {
    ($step:expr, Yield($y:pat) => $on_yield:expr, Done($r:pat) => $on_done:expr $(,)?) => {
        match $step {
            $crate::Step::Yielded($y) => $on_yield,
            $crate::Step::Done($r) => $on_done,
        }
    };
}

// ============================================================================
// 5. EXTENSION METHODS (DX ERGONOMICS)
// ============================================================================

/// Ergonomic extension methods for any state machine.
pub trait MachineExt: Machine {
    /// Converts a 1-way generator into standard `core::iter::Iterator`.
    #[inline(always)]
    fn into_iter(self) -> GenIter<Self>
    where
        Self: Sized + Generator,
    {
        GenIter(self)
    }
}

impl<M: ?Sized + Machine> MachineExt for M {}

/// Common prelude for `machine-core`.
pub mod prelude {
    pub use crate::{
        Effect, Either, FusedMachine, GenIter, Generator, Machine, MachineExt, Request, Step,
        handle_sync, machine_match,
    };
}

#[cfg(test)]
mod tests {
    use super::prelude::*;

    // 1. Test 1-Way Generator
    struct RangeGen {
        curr: u64,
        max: u64,
    }

    impl Machine for RangeGen {
        type Yield = u64;
        type Resume = ();
        type Return = ();

        fn step(&mut self, _resume: ()) -> Step<Self::Yield, Self::Return> {
            if self.curr < self.max {
                let val = self.curr;
                self.curr += 1;
                Step::Yielded(val)
            } else {
                Step::Done(())
            }
        }
    }

    impl FusedMachine for RangeGen {}

    #[test]
    fn test_generator_to_iterator_projection() {
        let generator = RangeGen { curr: 1, max: 4 };
        let sum: u64 = generator.into_iter().sum();
        assert_eq!(sum, 6); // 1 + 2 + 3 = 6
    }

    // 2. Test 2-Way Typed Algebraic Effects
    struct GreetEffect {
        pub prompt: &'static str,
    }
    impl Effect for GreetEffect {
        type Response = &'static str;
    }

    enum GreetState {
        Init,
        WaitingName,
        Done,
    }

    struct GreetFSM {
        state: GreetState,
    }

    impl Machine for GreetFSM {
        type Yield = GreetEffect;
        type Resume = Option<&'static str>;
        type Return = &'static str;

        fn step(&mut self, resume: Self::Resume) -> Step<Self::Yield, Self::Return> {
            match self.state {
                GreetState::Init => {
                    self.state = GreetState::WaitingName;
                    Step::Yielded(GreetEffect { prompt: "Name?" })
                }
                GreetState::WaitingName => {
                    let name = resume.expect("Resume required for GreetEffect");
                    self.state = GreetState::Done;
                    Step::Done(name)
                }
                GreetState::Done => panic!("Polled after Done"),
            }
        }
    }

    #[test]
    fn test_two_way_effect_dispatch() {
        let machine = GreetFSM {
            state: GreetState::Init,
        };
        let result = handle_sync(machine, |eff| {
            assert_eq!(eff.prompt, "Name?");
            Some("Arthur")
        });
        assert_eq!(result, Some("Arthur"));
    }

    #[test]
    fn test_two_way_effect_abort() {
        let machine = GreetFSM {
            state: GreetState::Init,
        };
        let result = handle_sync(machine, |_eff| None);
        assert_eq!(result, None);
    }
}
