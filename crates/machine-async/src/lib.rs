//! Asynchronous drivers for `machine-rs`.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use machine_core::{Effect, Machine, ResumeSignal, Step};

/// A zero-cost, stackless [`Future`] driving an effectful [`Machine`] to completion.
///
/// Implemented entirely without compiler-generated `async fn` state machines:
/// - Connects [`Machine::step`] directly to executor polling via manual [`Future::poll`].
/// - Features an **in-place execution pipeline**: avoids copying or replacing large
///   nested future frames on the stack via `core::mem::replace`.
/// - Features an **inline fast-path**: immediately ready effects loop synchronously
///   in registers without writing to storage or relinquishing control to the runtime.
pub struct DriveFuture<M, H, R, Fut> {
    machine: M,
    handler: H,
    current_fut: Option<Fut>,
    pending_signal: Option<ResumeSignal<R>>,
    is_terminated: bool,
}

impl<M, H, R, Fut> DriveFuture<M, H, R, Fut> {
    /// Constructs a new asynchronous driver future.
    #[inline(always)]
    pub const fn new(machine: M, handler: H) -> Self {
        Self {
            machine,
            handler,
            current_fut: None,
            pending_signal: Some(ResumeSignal::Start),
            is_terminated: false,
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
        // SAFETY: We never move `machine`, `handler`, or `current_fut` out of this pinned structure.
        let this = unsafe { self.get_unchecked_mut() };

        if this.is_terminated {
            panic!("Contract violation: DriveFuture polled after completion");
        }

        // 1. In-place poll of previously pending future without stack copies
        if let Some(fut) = &mut this.current_fut {
            // SAFETY: `fut` is pinned in-place inside `this.current_fut`
            let pinned = unsafe { Pin::new_unchecked(fut) };
            match pinned.poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(resp)) => {
                    this.current_fut = None;
                    this.pending_signal = Some(ResumeSignal::Resume(resp));
                }
                Poll::Ready(None) => {
                    this.current_fut = None;
                    let abort_step = this.machine.step(ResumeSignal::Abort);
                    debug_assert!(
                        matches!(abort_step, Step::Done(_)),
                        "machine yielded effect on abort signal rather than terminating"
                    );
                    this.machine.close();
                    this.is_terminated = true;
                    return Poll::Ready(None);
                }
            }
        }

        // 2. FSM Execution Loop with inline fast-path
        loop {
            let sig = this.pending_signal.take().unwrap_or(ResumeSignal::Start);
            match this.machine.step(sig) {
                Step::Yielded(eff) => {
                    let mut fut = (this.handler)(eff);
                    // Fast-path: check immediate completion on local stack frame
                    // SAFETY: `fut` is pinned to the local stack frame during immediate poll
                    let mut pinned = unsafe { Pin::new_unchecked(&mut fut) };
                    match pinned.as_mut().poll(cx) {
                        Poll::Ready(Some(resp)) => {
                            this.pending_signal = Some(ResumeSignal::Resume(resp));
                            continue;
                        }
                        Poll::Ready(None) => {
                            let abort_step = this.machine.step(ResumeSignal::Abort);
                            debug_assert!(
                                matches!(abort_step, Step::Done(_)),
                                "machine yielded effect on abort signal rather than terminating"
                            );
                            this.machine.close();
                            this.is_terminated = true;
                            return Poll::Ready(None);
                        }
                        Poll::Pending => {
                            this.current_fut = Some(fut);
                            return Poll::Pending;
                        }
                    }
                }
                Step::Done(ret) => {
                    this.is_terminated = true;
                    return Poll::Ready(Some(ret));
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

/// Trait for zero-allocation asynchronous effect handlers polled directly by an executor.
///
/// Completely eliminates intermediate [`Future`] allocations and state machine nesting.
pub trait AsyncEffectHandler<E: Effect> {
    /// Directly polls an effect using the active executor context.
    fn poll_effect(&mut self, eff: &E, cx: &mut Context<'_>) -> Poll<Option<E::Response>>;
}

/// Direct zero-overhead Future that executes an effect machine without intermediate Future wrapping.
pub struct DriveDirectFuture<M, H, E> {
    machine: M,
    handler: H,
    current_effect: Option<E>,
    is_terminated: bool,
}

impl<M, H, E> DriveDirectFuture<M, H, E> {
    /// Constructs a new direct driver future.
    #[inline(always)]
    pub const fn new(machine: M, handler: H) -> Self {
        Self {
            machine,
            handler,
            current_effect: None,
            is_terminated: false,
        }
    }

    #[inline(always)]
    fn step_loop(
        &mut self,
        mut signal: ResumeSignal<E::Response>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<M::Return>>
    where
        E: Effect,
        M: Machine<Yield = E, Resume = ResumeSignal<E::Response>>,
        H: AsyncEffectHandler<E>,
    {
        loop {
            match self.machine.step(signal) {
                Step::Yielded(eff) => match self.handler.poll_effect(&eff, cx) {
                    Poll::Ready(Some(resp)) => {
                        signal = ResumeSignal::Resume(resp);
                        continue;
                    }
                    Poll::Ready(None) => {
                        let abort_step = self.machine.step(ResumeSignal::Abort);
                        debug_assert!(
                            matches!(abort_step, Step::Done(_)),
                            "machine yielded effect on abort signal rather than terminating"
                        );
                        self.machine.close();
                        self.is_terminated = true;
                        return Poll::Ready(None);
                    }
                    Poll::Pending => {
                        self.current_effect = Some(eff);
                        return Poll::Pending;
                    }
                },
                Step::Done(ret) => {
                    self.is_terminated = true;
                    return Poll::Ready(Some(ret));
                }
            }
        }
    }
}

impl<M, H, E> Future for DriveDirectFuture<M, H, E>
where
    E: Effect,
    M: Machine<Yield = E, Resume = ResumeSignal<E::Response>>,
    H: AsyncEffectHandler<E>,
{
    type Output = Option<M::Return>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: We never move `machine`, `handler`, or `current_effect` out of this pinned structure.
        let this = unsafe { self.get_unchecked_mut() };

        if this.is_terminated {
            panic!("Contract violation: DriveDirectFuture polled after completion");
        }

        if let Some(eff) = this.current_effect.take() {
            match this.handler.poll_effect(&eff, cx) {
                Poll::Pending => {
                    this.current_effect = Some(eff);
                    return Poll::Pending;
                }
                Poll::Ready(Some(resp)) => {
                    return this.step_loop(ResumeSignal::Resume(resp), cx);
                }
                Poll::Ready(None) => {
                    let abort_step = this.machine.step(ResumeSignal::Abort);
                    debug_assert!(
                        matches!(abort_step, Step::Done(_)),
                        "machine yielded effect on abort signal rather than terminating"
                    );
                    this.machine.close();
                    this.is_terminated = true;
                    return Poll::Ready(None);
                }
            }
        }

        this.step_loop(ResumeSignal::Start, cx)
    }
}

/// Drives an effect machine using a direct [`AsyncEffectHandler`] without intermediate future wrapping.
#[inline(always)]
pub fn handle_async_direct<M, H, E>(machine: M, handler: H) -> DriveDirectFuture<M, H, E>
where
    E: Effect,
    M: Machine<Yield = E, Resume = ResumeSignal<E::Response>>,
    H: AsyncEffectHandler<E>,
{
    DriveDirectFuture::new(machine, handler)
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
