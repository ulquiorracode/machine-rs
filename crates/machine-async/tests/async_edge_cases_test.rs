use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use machine_async::handle_async;
use machine_core::prelude::*;
use std::sync::Arc;

struct AsyncImmediateDone;

impl Machine for AsyncImmediateDone {
    type Yield = AsyncDummyEffect;
    type Resume = ResumeSignal<u32>;
    type Return = u32;

    fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
        assert_eq!(signal, ResumeSignal::Start);
        Step::Done(100)
    }
}

struct AsyncDummyEffect;
impl Effect for AsyncDummyEffect {
    type Response = u32;
}

#[tokio::test]
async fn test_async_immediate_done() {
    let invoked = Arc::new(AtomicBool::new(false));
    let invoked_clone = invoked.clone();
    let res = handle_async(AsyncImmediateDone, move |_eff| {
        let flag = invoked_clone.clone();
        async move {
            flag.store(true, Ordering::SeqCst);
            Some(1)
        }
    })
    .await;

    assert_eq!(res, Some(100));
    assert!(
        !invoked.load(Ordering::SeqCst),
        "Handler must not run when machine is immediately done"
    );
}

enum AsyncChainEffect {
    Fetch(usize),
    Process(usize),
    Save(usize),
}

impl Effect for AsyncChainEffect {
    type Response = usize;
}

struct AsyncThreeStepPipeline {
    step: usize,
}

impl Machine for AsyncThreeStepPipeline {
    type Yield = AsyncChainEffect;
    type Resume = ResumeSignal<usize>;
    type Return = usize;

    fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
        match signal {
            ResumeSignal::Start => {
                self.step = 1;
                Step::Yielded(AsyncChainEffect::Fetch(10))
            }
            ResumeSignal::Resume(val) => match self.step {
                1 => {
                    assert_eq!(val, 100);
                    self.step = 2;
                    Step::Yielded(AsyncChainEffect::Process(val))
                }
                2 => {
                    assert_eq!(val, 200);
                    self.step = 3;
                    Step::Yielded(AsyncChainEffect::Save(val))
                }
                3 => {
                    assert_eq!(val, 300);
                    Step::Done(val)
                }
                _ => unreachable!(),
            },
            ResumeSignal::Abort => panic!("Unexpected abort"),
        }
    }
}

#[tokio::test]
async fn test_async_three_step_io_chain() {
    let machine = AsyncThreeStepPipeline { step: 0 };
    let res = handle_async(machine, |eff| async move {
        match eff {
            AsyncChainEffect::Fetch(n) => Some(n * 10),
            AsyncChainEffect::Process(n) => Some(n * 2),
            AsyncChainEffect::Save(n) => Some(n + 100),
        }
    })
    .await;

    assert_eq!(res, Some(300));
}

struct AsyncDropGuardMachine {
    dropped: Arc<AtomicBool>,
    closed: Arc<AtomicUsize>,
}

impl Drop for AsyncDropGuardMachine {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

impl Machine for AsyncDropGuardMachine {
    type Yield = AsyncDummyEffect;
    type Resume = ResumeSignal<u32>;
    type Return = ();

    fn step(&mut self, _signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
        Step::Yielded(AsyncDummyEffect)
    }

    fn close(&mut self) {
        self.closed.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn test_async_panic_in_handler_drops_machine_cleanly() {
    let dropped = Arc::new(AtomicBool::new(false));
    let closed = Arc::new(AtomicUsize::new(0));

    let machine = AsyncDropGuardMachine {
        dropped: dropped.clone(),
        closed: closed.clone(),
    };

    let join_res = tokio::spawn(async move {
        handle_async(machine, |_eff| async move {
            panic!("Async handler intentional panic");
        })
        .await
    })
    .await;

    assert!(join_res.is_err(), "Task must panic");
    assert!(
        dropped.load(Ordering::SeqCst),
        "Machine must be dropped upon task unwinding"
    );
}

struct AsyncCounterGen {
    curr: u64,
    limit: u64,
}

impl Machine for AsyncCounterGen {
    type Yield = u64;
    type Resume = ();
    type Return = ();

    fn step(&mut self, _resume: ()) -> Step<Self::Yield, Self::Return> {
        if self.curr < self.limit {
            let val = self.curr;
            self.curr += 1;
            Step::Yielded(val)
        } else {
            Step::Done(())
        }
    }
}

#[tokio::test]
async fn test_gen_stream_collect_and_fused() {
    use futures_util::StreamExt;
    use machine_async::MachineAsyncExt;

    let generator = AsyncCounterGen { curr: 0, limit: 5 };
    let mut stream = generator.into_stream();

    let mut items = Vec::new();
    while let Some(item) = stream.next().await {
        items.push(item);
    }

    assert_eq!(items, vec![0, 1, 2, 3, 4]);
    // Test fused property
    assert_eq!(stream.next().await, None);
    assert_eq!(stream.next().await, None);
}

#[tokio::test]
async fn test_gen_stream_empty_immediate_done() {
    use futures_util::StreamExt;
    use machine_async::MachineAsyncExt;

    let generator = AsyncCounterGen { curr: 5, limit: 5 };
    let mut stream = generator.into_stream();

    assert_eq!(stream.next().await, None);
    assert_eq!(stream.next().await, None);
}
