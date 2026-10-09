//! Asynchronous runtime bridge and driver for `effect-rs`.
//!
//! Allows running pure, uncolored effectful state machines on asynchronous runtimes
//! (e.g. Tokio) by resolving yielded effects via thread-safe async handler futures.

use core::future::Future;
use effect_core::{Effect, EffectDecision, EffectMachine, Step};

/// Drives an [`EffectMachine`] to completion asynchronously with full thread-safety (`Send`).
///
/// Yielded effects are dispatched to an async closure/future `handler`,
/// which resolves them (e.g., performing database, network, or file I/O).
///
/// The state machine itself remains completely synchronous, uncolored, and `#![no_std]`.
pub async fn handle_async<M, H, Fut>(mut machine: M, mut handler: H) -> Option<M::Output>
where
    M: EffectMachine + Send,
    H: FnMut(M::Effect) -> Fut + Send,
    Fut: Future<Output = EffectDecision<<M::Effect as Effect>::Resume>> + Send,
{
    let mut resume_val = None;
    loop {
        match machine.resume(resume_val.take()) {
            Step::Yielded(effect) => match handler(effect).await {
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

        // Ensure future is Send and can be spawned on Tokio
        let handle = tokio::spawn(async move {
            handle_async(machine, |effect| async move {
                assert_eq!(effect.url, "https://api.example.com");
                EffectDecision::Resume(200)
            })
            .await
        });

        let status = handle.await.unwrap();
        assert_eq!(status, Some(200));
    }

    #[tokio::test]
    async fn test_handle_async_abort() {
        let machine = FetchMachine {
            state: FetchState::Init,
        };

        let status = handle_async(machine, |_effect| async move { EffectDecision::Abort }).await;

        assert_eq!(status, None);
    }
}
