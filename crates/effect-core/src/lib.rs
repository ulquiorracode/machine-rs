//! # Effect Core (`effect-core`)
//!
//! Zero-cost, `#![no_std]` foundational engine and contracts for **Algebraic Effects**.
//!
//! # Core Taxonomical Dimensions
//! - [`Effect`]: Marker contract defining an algebraic effect and its corresponding `Resume` type.
//! - [`EffectMachine`]: A stackless, monomorphic finite state machine yielding effects.
//! - [`Step`]: Result of advancing an effect machine: `Yielded`, `Done`, or `Aborted`.
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
    /// The computation was aborted or completed early without returning `T`.
    Aborted,
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

    /// Closes the state machine early upon cancellation or abort,
    /// dropping internal resources deterministically without panic.
    fn close(&mut self) {}
}

/// The control flow decision made by an effect handler.
#[derive(Debug, PartialEq, Eq)]
pub enum EffectDecision<R> {
    /// Resume the computation with the provided value.
    Resume(R),
    /// Abort the computation immediately.
    Abort,
}

/// Synchronously handles and drives an [`EffectMachine`] to completion using a closure.
///
/// Supports graceful cancellation via [`EffectDecision::Abort`].
///
/// # Mechanical Sympathy
/// - Allocates **0 bytes** of heap memory.
/// - The handler closure is statically inlined into the dispatch loop.
#[inline(always)]
pub fn handle_sync<M, H>(mut machine: M, mut handler: H) -> Option<M::Output>
where
    M: EffectMachine,
    H: FnMut(M::Effect) -> EffectDecision<<M::Effect as Effect>::Resume>,
{
    let mut resume_val = None;
    loop {
        match machine.resume(resume_val.take()) {
            Step::Yielded(effect) => match handler(effect) {
                EffectDecision::Resume(response) => {
                    resume_val = Some(response);
                }
                EffectDecision::Abort => {
                    machine.close();
                    return None;
                }
            },
            Step::Done(output) => return Some(output),
            Step::Aborted => {
                machine.close();
                return None;
            }
        }
    }
}

/// Declarative macro for handling algebraic effects synchronously with pattern matching.
///
/// Provides a zero-dependency, zero-compile-overhead alternative to heavy proc-macros.
///
/// ```rust
/// use effect_core::prelude::*;
///
/// # struct Ask;
/// # impl Effect for Ask { type Resume = &'static str; }
/// # enum MyEffect { Ask(Ask) }
/// # impl Effect for MyEffect { type Resume = &'static str; }
/// # struct MyMachine;
/// # impl EffectMachine for MyMachine {
/// #     type Effect = MyEffect;
/// #     type Output = &'static str;
/// #     fn resume(&mut self, _: Option<&'static str>) -> Step<Self::Effect, Self::Output> {
/// #         Step::Done("Done")
/// #     }
/// # }
/// # let machine = MyMachine;
/// let res = effect_match!(machine, {
///     perform MyEffect::Ask(_a) => {
///         EffectDecision::Resume("Answer")
///     },
///     return val => {
///         val
///     }
/// });
/// ```
#[macro_export]
macro_rules! effect_match {
    (
        $machine:expr, {
            $( perform $pat:pat => $body:expr ),* ,
            return $ret_pat:pat => $ret_body:expr $(,)?
        }
    ) => {{
        let mut __machine = $machine;
        let mut __resume_val = None;
        loop {
            match $crate::EffectMachine::resume(&mut __machine, __resume_val.take()) {
                $crate::Step::Yielded(__effect) => {
                    let __decision = match __effect {
                        $( $pat => $body, )*
                        #[allow(unreachable_patterns)]
                        _ => $crate::EffectDecision::Abort,
                    };
                    match __decision {
                        $crate::EffectDecision::Resume(__val) => {
                            __resume_val = Some(__val);
                        }
                        $crate::EffectDecision::Abort => {
                            $crate::EffectMachine::close(&mut __machine);
                            break None;
                        }
                    }
                }
                $crate::Step::Done($ret_pat) => {
                    break Some($ret_body);
                }
                $crate::Step::Aborted => {
                    $crate::EffectMachine::close(&mut __machine);
                    break None;
                }
            }
        }
    }};
}

/// Common prelude for algebraic effects.
pub mod prelude {
    pub use crate::{Effect, EffectDecision, EffectMachine, Step, effect_match, handle_sync};
}

#[cfg(test)]
mod tests {
    use super::prelude::*;

    struct Ask {
        pub prompt: &'static str,
    }
    impl Effect for Ask {
        type Resume = &'static str;
    }

    struct FatalError;
    impl Effect for FatalError {
        type Resume = ();
    }

    enum AppEffect {
        Ask(Ask),
        Fatal(FatalError),
    }

    impl Effect for AppEffect {
        type Resume = AppResume;
    }

    enum AppResume {
        Str(&'static str),
    }

    enum GreetState {
        Start,
        WaitingName,
        Dead,
        Finished,
    }

    struct GreetMachine {
        state: GreetState,
        closed: bool,
    }

    impl GreetMachine {
        pub const fn new() -> Self {
            Self {
                state: GreetState::Start,
                closed: false,
            }
        }
    }

    impl EffectMachine for GreetMachine {
        type Effect = AppEffect;
        type Output = &'static str;

        fn resume(&mut self, resume: Option<AppResume>) -> Step<Self::Effect, Self::Output> {
            match self.state {
                GreetState::Start => {
                    self.state = GreetState::WaitingName;
                    Step::Yielded(AppEffect::Ask(Ask {
                        prompt: "Your name?",
                    }))
                }
                GreetState::WaitingName => match resume {
                    Some(AppResume::Str("AbortedUser")) => {
                        self.state = GreetState::Dead;
                        Step::Yielded(AppEffect::Fatal(FatalError))
                    }
                    Some(AppResume::Str(name)) => {
                        self.state = GreetState::Finished;
                        Step::Done(name)
                    }
                    _ => Step::Aborted,
                },
                GreetState::Dead => Step::Aborted,
                GreetState::Finished => panic!("Polled after completion"),
            }
        }

        fn close(&mut self) {
            self.closed = true;
        }
    }

    #[test]
    fn test_handle_sync_success() {
        let machine = GreetMachine::new();
        let name = handle_sync(machine, |effect| match effect {
            AppEffect::Ask(ask) => {
                assert_eq!(ask.prompt, "Your name?");
                EffectDecision::Resume(AppResume::Str("Alice"))
            }
            AppEffect::Fatal(_) => EffectDecision::Abort,
        });

        assert_eq!(name, Some("Alice"));
    }

    #[test]
    fn test_handle_sync_abort_protocol() {
        let machine = GreetMachine::new();
        let res = handle_sync(machine, |effect| match effect {
            AppEffect::Ask(_) => EffectDecision::Abort,
            _ => EffectDecision::Abort,
        });

        assert_eq!(res, None);
    }

    #[test]
    fn test_effect_match_macro_success() {
        let machine = GreetMachine::new();
        let name = effect_match!(machine, {
            perform AppEffect::Ask(_a) => EffectDecision::Resume(AppResume::Str("Bob")),
            return val => val,
        });

        assert_eq!(name, Some("Bob"));
    }

    #[test]
    fn test_effect_match_macro_abort() {
        let machine = GreetMachine::new();
        let res = effect_match!(machine, {
            perform AppEffect::Ask(_a) => EffectDecision::Abort,
            return val => val,
        });

        assert_eq!(res, None);
    }
}
