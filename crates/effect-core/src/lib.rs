//! # Effect Core (`effect-core`)
//!
//! Zero-cost, `#![no_std]` foundational engine and contracts for **Algebraic Effects**.
//!
//! # Core Concepts
//! - [`Effect`]: Marker trait defining an algebraic effect and its corresponding `Resume` type.
//! - [`EffectMachine`]: A stackless, monomorphic finite state machine yielding effects.
//! - [`Step`]: Result of advancing an effect machine: either an yielded effect or final computation result.
//! - [`handle_sync`]: Synchronous driver evaluating effectful machines against a handler.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

/// An algebraic effect yielded by a computation.
///
/// Each effect specifies its `Resume` type — the value that the execution
/// environment / handler must inject back into the computation to resume it.
pub trait Effect {
    /// The type injected back into the suspended computation upon resumption.
    type Resume;
}

/// The outcome of advancing an [`EffectMachine`].
#[derive(Debug, PartialEq, Eq)]
pub enum Step<E: Effect, T> {
    /// The computation suspended and yielded an effect requiring a response from the environment.
    Yielded(E),
    /// The computation finished and produced its terminal return value.
    Done(T),
}

/// A state machine that executes an effectful computation step by step.
///
/// In release mode, state transitions are completely monomorphic and inlined by LLVM
/// with zero heap allocations.
pub trait EffectMachine {
    /// The algebraic effect type yielded by this machine.
    type Effect: Effect;

    /// The final return value produced when the computation completes.
    type Output;

    /// Advances the state machine.
    ///
    /// - On the initial call (before any suspend points), `resume` must be `None`.
    /// - On subsequent calls after a suspend point, `resume` must be `Some(value)`
    ///   containing the environment's response to the previously yielded effect.
    fn resume(
        &mut self,
        resume: Option<<Self::Effect as Effect>::Resume>,
    ) -> Step<Self::Effect, Self::Output>;
}

/// Synchronously handles and drives an [`EffectMachine`] to completion using a closure.
///
/// # Mechanical Sympathy
/// - Allocates **0 bytes** of heap memory.
/// - The handler closure is statically inlined into the dispatch loop.
#[inline(always)]
pub fn handle_sync<M, H>(mut machine: M, mut handler: H) -> M::Output
where
    M: EffectMachine,
    H: FnMut(M::Effect) -> <M::Effect as Effect>::Resume,
{
    let mut resume_val = None;
    loop {
        match machine.resume(resume_val.take()) {
            Step::Yielded(effect) => {
                let response = handler(effect);
                resume_val = Some(response);
            }
            Step::Done(output) => return output,
        }
    }
}

/// Common prelude for algebraic effects.
pub mod prelude {
    pub use crate::{Effect, EffectMachine, Step, handle_sync};
}

#[cfg(test)]
mod tests {
    use super::prelude::*;

    // 1. Define effects
    struct Ask {
        pub prompt: &'static str,
    }
    impl Effect for Ask {
        type Resume = &'static str;
    }

    struct Double {
        pub value: u64,
    }
    impl Effect for Double {
        type Resume = u64;
    }

    enum AppEffect {
        Ask(Ask),
        Double(Double),
    }

    impl Effect for AppEffect {
        type Resume = AppResume;
    }

    enum AppResume {
        Str(&'static str),
        Num(u64),
    }

    // 2. Define a hand-written state machine representing:
    // fn greet() -> String {
    //     let name = perform!(Ask { prompt: "Your name?" });
    //     let age = perform!(Double { value: 10 });
    //     format!("{}: {}", name, age)
    // }
    enum GreetState {
        Start,
        WaitingName,
        WaitingAge { name: &'static str },
        Finished,
    }

    struct GreetMachine {
        state: GreetState,
    }

    impl GreetMachine {
        pub const fn new() -> Self {
            Self {
                state: GreetState::Start,
            }
        }
    }

    impl EffectMachine for GreetMachine {
        type Effect = AppEffect;
        type Output = (&'static str, u64);

        fn resume(&mut self, resume: Option<AppResume>) -> Step<Self::Effect, Self::Output> {
            match self.state {
                GreetState::Start => {
                    self.state = GreetState::WaitingName;
                    Step::Yielded(AppEffect::Ask(Ask {
                        prompt: "Your name?",
                    }))
                }
                GreetState::WaitingName => {
                    let name = match resume {
                        Some(AppResume::Str(s)) => s,
                        _ => panic!("Illegal resume payload for Ask"),
                    };
                    self.state = GreetState::WaitingAge { name };
                    Step::Yielded(AppEffect::Double(Double { value: 10 }))
                }
                GreetState::WaitingAge { name } => {
                    let age = match resume {
                        Some(AppResume::Num(n)) => n,
                        _ => panic!("Illegal resume payload for Double"),
                    };
                    self.state = GreetState::Finished;
                    Step::Done((name, age))
                }
                GreetState::Finished => panic!("Machine polled after completion"),
            }
        }
    }

    #[test]
    fn test_handle_sync_zero_alloc() {
        let machine = GreetMachine::new();
        let (name, age) = handle_sync(machine, |effect| match effect {
            AppEffect::Ask(ask) => {
                assert_eq!(ask.prompt, "Your name?");
                AppResume::Str("Arthur")
            }
            AppEffect::Double(d) => AppResume::Num(d.value * 2),
        });

        assert_eq!(name, "Arthur");
        assert_eq!(age, 20);
    }
}
