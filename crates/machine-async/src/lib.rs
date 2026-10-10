//! Asynchronous drivers for `machine-rs`.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use machine_core::{Effect, Machine, ResumeSignal, Step};

/// Internal execution state of the zero-cost [`DriveFuture`].
enum DriverState<R, Fut> {
    /// Ready to perform the next FSM transition step.
    Step(ResumeSignal<R>),
    /// Waiting on an asynchronous effect resolution future.
    Waiting(Fut),
    /// Terminal completion state.
    Terminated,
}

/// A zero-cost, stackless [`Future`] driving an effectful [`Machine`] to completion.
///
/// Implemented entirely without compiler-generated `async fn` state machines:
/// - Connects [`Machine::step`] directly to executor polling via manual [`Future::poll`].
/// - Features an **inline fast-path**: immediately ready effects loop synchronously
///   without relinquishing execution to the task waker or runtime executor.
pub struct DriveFuture<M, H, R, Fut> {
    machine: M,
    handler: H,
    state: DriverState<R, Fut>,
}

impl<M, H, R, Fut> DriveFuture<M, H, R, Fut> {
    /// Constructs a new asynchronous driver future.
    #[inline(always)]
    pub const fn new(machine: M, handler: H) -> Self {
        Self {
            machine,
            handler,
            state: DriverState::Step(ResumeSignal::Start),
        }
    }
}

impl<M, E, H, Fut> Future for DriveFuture<M, H, E::Response, Fut>
where
    E: Effect,
    M: Machine<Yield = E, Resume = ResumeSignal<E::Response>>,
    H: FnMut(E) -> Fut,
    Fut: Future<Output = Option<E::Response>>,
{
    type Output = Option<M::Return>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: We never move `machine` or `handler` out of this pinned structure.
        // The nested `fut` inside `DriverState::Waiting` is pinned in-place.
        let this = unsafe { self.get_unchecked_mut() };

        loop {
            match core::mem::replace(&mut this.state, DriverState::Terminated) {
                DriverState::Step(sig) => match this.machine.step(sig) {
                    Step::Yielded(eff) => {
                        let mut fut = (this.handler)(eff);
                        // Fast-path: immediately poll before suspending to executor
                        // SAFETY: `fut` is pinned to the local stack frame for this immediate check
                        let mut pinned = unsafe { Pin::new_unchecked(&mut fut) };
                        match pinned.as_mut().poll(cx) {
                            Poll::Ready(Some(resp)) => {
                                this.state = DriverState::Step(ResumeSignal::Resume(resp));
                                continue;
                            }
                            Poll::Ready(None) => {
                                let abort_step = this.machine.step(ResumeSignal::Abort);
                                debug_assert!(
                                    matches!(abort_step, Step::Done(_)),
                                    "machine yielded effect on abort signal rather than terminating"
                                );
                                this.machine.close();
                                this.state = DriverState::Terminated;
                                return Poll::Ready(None);
                            }
                            Poll::Pending => {
                                this.state = DriverState::Waiting(fut);
                                return Poll::Pending;
                            }
                        }
                    }
                    Step::Done(ret) => {
                        this.state = DriverState::Terminated;
                        return Poll::Ready(Some(ret));
                    }
                },
                DriverState::Waiting(mut fut) => {
                    // SAFETY: `fut` is pinned to the local stack frame during polling
                    let mut pinned = unsafe { Pin::new_unchecked(&mut fut) };
                    match pinned.as_mut().poll(cx) {
                        Poll::Ready(Some(resp)) => {
                            this.state = DriverState::Step(ResumeSignal::Resume(resp));
                            continue;
                        }
                        Poll::Ready(None) => {
                            let abort_step = this.machine.step(ResumeSignal::Abort);
                            debug_assert!(
                                matches!(abort_step, Step::Done(_)),
                                "machine yielded effect on abort signal rather than terminating"
                            );
                            this.machine.close();
                            this.state = DriverState::Terminated;
                            return Poll::Ready(None);
                        }
                        Poll::Pending => {
                            this.state = DriverState::Waiting(fut);
                            return Poll::Pending;
                        }
                    }
                }
                DriverState::Terminated => {
                    panic!("Contract violation: DriveFuture polled after completion");
                }
            }
        }
    }
}

/// Constructs a zero-cost [`DriveFuture`] driving an effect machine to completion.
///
/// Evaluates pure domain state machines on async runtimes with zero function coloring.
#[inline(always)]
pub fn handle_async<M, E, H, Fut>(machine: M, handler: H) -> DriveFuture<M, H, E::Response, Fut>
where
    E: Effect,
    M: Machine<Yield = E, Resume = ResumeSignal<E::Response>>,
    H: FnMut(E) -> Fut,
    Fut: Future<Output = Option<E::Response>>,
{
    DriveFuture::new(machine, handler)
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
