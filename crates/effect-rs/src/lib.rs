//! # Effect RS (`effect-rs`)
//!
//! Algebraic Effects in Rust: Uncolored, Zero-Cost, Monomorphic State Machines.
//!
//! Eliminates the "function coloring problem" (`async`/`await`, `throws`) by separating
//! domain intentions (algebraic effects) from execution environments (handlers).

#![cfg_attr(not(feature = "std"), no_std)]

pub use effect_core::*;

#[cfg(feature = "async")]
pub use effect_async::*;

pub mod prelude {
    #[cfg(feature = "async")]
    pub use effect_async::handle_async;
    pub use effect_core::prelude::*;
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    struct Ping;
    impl Effect for Ping {
        type Resume = &'static str;
    }

    enum AppEffect {
        Ping(Ping),
    }
    impl Effect for AppEffect {
        type Resume = &'static str;
    }

    enum SimpleState {
        Init,
        Done,
    }

    struct SimpleMachine(SimpleState);

    impl EffectMachine for SimpleMachine {
        type Effect = AppEffect;
        type Output = &'static str;

        fn resume(&mut self, resume: Option<&'static str>) -> Step<Self::Effect, Self::Output> {
            match self.0 {
                SimpleState::Init => {
                    self.0 = SimpleState::Done;
                    Step::Yielded(AppEffect::Ping(Ping))
                }
                SimpleState::Done => Step::Done(resume.unwrap_or("default")),
            }
        }
    }

    #[test]
    fn test_facade_and_macro_match() {
        let machine = SimpleMachine(SimpleState::Init);

        let result = effect_match!(machine, {
            perform AppEffect::Ping(_ping) => EffectDecision::Resume("Pong"),
            return greeting => greeting,
        });

        assert_eq!(result, Some("Pong"));
    }
}
