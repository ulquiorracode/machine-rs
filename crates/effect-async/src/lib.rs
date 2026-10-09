//! Asynchronous runtime bridge and driver for `effect-rs`.
//!
//! Allows running pure, uncolored effectful state machines on asynchronous runtimes
//! (e.g. Tokio) by resolving yielded effects via async handler futures.

use core::future::Future;
use effect_core::{Effect, EffectMachine, Step};

/// Drives an [`EffectMachine`] to completion asynchronously.
///
/// Yielded effects are dispatched to an async closure/future `handler`,
/// which resolves them (e.g., performing database, network, or file I/O).
///
/// The state machine itself remains completely synchronous, uncolored, and `#![no_std]`.
pub async fn handle_async<M, H, Fut>(mut machine: M, mut handler: H) -> M::Output
where
    M: EffectMachine,
    H: FnMut(M::Effect) -> Fut,
    Fut: Future<Output = <M::Effect as Effect>::Resume>,
{
    let mut resume_val = None;
    loop {
        match machine.resume(resume_val.take()) {
            Step::Yielded(effect) => {
                let response = handler(effect).await;
                resume_val = Some(response);
            }
            Step::Done(output) => return output,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FetchEffect {
        pub url: &'static str,
    }
    impl Effect for FetchEffect {
        type Resume = u32;
    }

    enum FetchState {
        Init,
        Waiting,
        Done,
    }

    struct FetchMachine {
        state: FetchState,
    }

    impl EffectMachine for FetchMachine {
        type Effect = FetchEffect;
        type Output = u32;

        fn resume(&mut self, resume: Option<u32>) -> Step<Self::Effect, Self::Output> {
            match self.state {
                FetchState::Init => {
                    self.state = FetchState::Waiting;
                    Step::Yielded(FetchEffect {
                        url: "https://api.example.com",
                    })
                }
                FetchState::Waiting => {
                    let code = resume.expect("resume payload required");
                    self.state = FetchState::Done;
                    Step::Done(code)
                }
                FetchState::Done => panic!("polled after done"),
            }
        }
    }

    #[tokio::test]
    async fn test_handle_async_tokio_bridge() {
        let machine = FetchMachine {
            state: FetchState::Init,
        };

        let status = handle_async(machine, |effect| async move {
            assert_eq!(effect.url, "https://api.example.com");
            // Simulate async network I/O
            tokio::task::yield_now().await;
            200
        })
        .await;

        assert_eq!(status, 200);
    }
}
