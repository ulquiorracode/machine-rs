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
                    let _ = machine.step(ResumeSignal::Abort);
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
        close_count: usize,
    }

    impl FetchMachine {
        pub const fn new() -> Self {
            Self {
                state: FetchState::Init,
                close_count: 0,
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
            self.close_count += 1;
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
        let machine = FetchMachine::new();

        let status = handle_async(machine, |_eff| async move { None }).await;
        assert_eq!(status, None);
    }
}
