use criterion::{Criterion, black_box, criterion_group, criterion_main};
use effect_core::prelude::*;

struct MathAdd(u64);
impl Effect for MathAdd {
    type Resume = u64;
}

enum MathState {
    Start,
    WaitingAdd,
    Done,
}

struct MathMachine {
    state: MathState,
    initial: u64,
}

impl MathMachine {
    pub const fn new(initial: u64) -> Self {
        Self {
            state: MathState::Start,
            initial,
        }
    }
}

impl EffectMachine for MathMachine {
    type Effect = MathAdd;
    type Output = u64;

    #[inline(always)]
    fn resume(&mut self, resume: Option<u64>) -> Step<Self::Effect, Self::Output> {
        match self.state {
            MathState::Start => {
                self.state = MathState::WaitingAdd;
                Step::Yielded(MathAdd(self.initial))
            }
            MathState::WaitingAdd => {
                let added = resume.unwrap_or(0);
                self.state = MathState::Done;
                Step::Done(self.initial + added)
            }
            MathState::Done => Step::Aborted,
        }
    }
}

async fn async_math(initial: u64) -> u64 {
    let added = async { 42 }.await;
    initial + added
}

fn bench_effect_vs_async(c: &mut Criterion) {
    let mut group = c.benchmark_group("dispatch_overhead");

    group.bench_function("effect_machine_handle_sync", |b| {
        b.iter(|| {
            let machine = MathMachine::new(black_box(10));
            let res = handle_sync(machine, |eff| EffectDecision::Resume(eff.0 + 32));
            assert_eq!(res, Some(52));
        })
    });

    group.bench_function("effect_match_macro", |b| {
        b.iter(|| {
            let machine = MathMachine::new(black_box(10));
            let res = effect_match!(machine, {
                perform MathAdd(val) => EffectDecision::Resume(val + 32),
                return val => val,
            });
            assert_eq!(res, Some(52));
        })
    });

    group.bench_function("native_async_future", |b| {
        b.iter(|| {
            let fut = async_math(black_box(10));
            // Poll native future once
            let mut pinned = core::pin::pin!(fut);
            let waker = futures_lite_noop_waker();
            let mut cx = core::task::Context::from_waker(&waker);
            match pinned.as_mut().poll(&mut cx) {
                core::task::Poll::Ready(val) => assert_eq!(val, 52),
                core::task::Poll::Pending => panic!("Pending"),
            }
        })
    });

    group.finish();
}

fn futures_lite_noop_waker() -> core::task::Waker {
    use core::task::{RawWaker, RawWakerVTable, Waker};
    static VTABLE: RawWakerVTable =
        RawWakerVTable::new(|p| RawWaker::new(p, &VTABLE), |_| {}, |_| {}, |_| {});
    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
}

criterion_group!(benches, bench_effect_vs_async);
criterion_main!(benches);
