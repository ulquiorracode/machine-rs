//! Asynchronous drivers for `machine-rs`.

use core::future::Future;
use machine_core::{Effect, Machine, ResumeSignal, Step};

/// Asynchronous effect driver executing an effectful [`Machine`] to completion.
///
/// Drives the machine on an async runtime by evaluating effect handler futures.
/// If the handler returns `None`, the machine is signaled with [`ResumeSignal::Abort`],
/// and [`Machine::close`] is invoked deterministically.
pub async fn handle_async<M, E, H, Fut>(mut machine: M, mut handler: H) -> Option<M::Return>
where
    E: Effect + Send,
    E::Response: Send,
    M: Machine<Yield = E, Resume = ResumeSignal<E::Response>> + Send,
    H: FnMut(E) -> Fut + Send,
    Fut: Future<Output = Option<E::Response>> + Send,
{
    let mut signal = ResumeSignal::Start;
    loop {
        match machine.step(signal) {
            Step::Yielded(eff) => match handler(eff).await {
                Some(resp) => {
                    signal = ResumeSignal::Resume(resp);
                }
                None => {
                    let abort_step = machine.step(ResumeSignal::Abort);
                    debug_assert!(
                        matches!(abort_step, Step::Done(_)),
                        "machine yielded effect on abort signal rather than terminating"
                    );
                    machine.close();
                    return None;
                }
            },
            Step::Done(ret) => return Some(ret),
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
        type Response = u32;
    }

    enum FetchState {
        Init,
        Waiting,
        Done,
        Aborted,
    }

    struct FetchMachine {
        state: FetchState,
        close_counter: Option<std::sync::Arc<core::sync::atomic::AtomicUsize>>,
    }

    impl FetchMachine {
        pub const fn new() -> Self {
            Self {
                state: FetchState::Init,
                close_counter: None,
            }
        }

        pub fn with_close_counter(
            counter: std::sync::Arc<core::sync::atomic::AtomicUsize>,
        ) -> Self {
            Self {
                state: FetchState::Init,
                close_counter: Some(counter),
            }
        }
    }

    impl Machine for FetchMachine {
        type Yield = FetchEffect;
        type Resume = ResumeSignal<u32>;
        type Return = u32;

        fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
            match self.state {
                FetchState::Init => {
                    self.state = FetchState::Waiting;
                    Step::Yielded(FetchEffect {
                        url: "https://api.example.com",
                    })
                }
                FetchState::Waiting => match signal {
                    ResumeSignal::Resume(code) => {
                        self.state = FetchState::Done;
                        Step::Done(code)
                    }
                    ResumeSignal::Abort => {
                        self.state = FetchState::Aborted;
                        Step::Done(0)
                    }
                    ResumeSignal::Start => {
                        self.state = FetchState::Aborted;
                        Step::Done(0)
                    }
                },
                FetchState::Done | FetchState::Aborted => {
                    panic!("Contract violation: Polled after terminal state");
                }
            }
        }

        fn close(&mut self) {
            if let Some(ref counter) = self.close_counter {
                counter.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    #[tokio::test]
    async fn test_async_tokio_driver_success() {
        let machine = FetchMachine::new();

        let handle = tokio::spawn(async move {
            handle_async(machine, |eff| async move {
                assert_eq!(eff.url, "https://api.example.com");
                Some(200)
            })
            .await
        });

        let status = handle.await.unwrap();
        assert_eq!(status, Some(200));
    }

    #[tokio::test]
    async fn test_async_tokio_driver_abort() {
        use core::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let machine = FetchMachine::with_close_counter(counter.clone());

        let status = handle_async(machine, |_eff| async move { None }).await;
        assert_eq!(status, None);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
}
