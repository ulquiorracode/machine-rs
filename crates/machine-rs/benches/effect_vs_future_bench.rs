use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use machine_rs::prelude::*;
use std::mem::size_of;
use tokio::runtime::Runtime;

// ============================================================================
// 1. MACHINE-RS 3-STEP ASYNC EFFECT FSM
// ============================================================================

enum AsyncIoEffect {
    Fetch(u64),
    Transform(u64),
    Commit(u64),
}

impl Effect for AsyncIoEffect {
    type Response = u64;
}

enum PipelineState {
    Start,
    FetchPending,
    TransformPending,
    CommitPending,
    Done,
}

struct PipelineFsm {
    state: PipelineState,
    id: u64,
}

impl PipelineFsm {
    pub const fn new(id: u64) -> Self {
        Self {
            state: PipelineState::Start,
            id,
        }
    }
}

impl Machine for PipelineFsm {
    type Yield = AsyncIoEffect;
    type Resume = ResumeSignal<u64>;
    type Return = u64;

    #[inline(always)]
    fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
        match self.state {
            PipelineState::Start => {
                self.state = PipelineState::FetchPending;
                Step::Yielded(AsyncIoEffect::Fetch(self.id))
            }
            PipelineState::FetchPending => match signal {
                ResumeSignal::Resume(val) => {
                    self.state = PipelineState::TransformPending;
                    Step::Yielded(AsyncIoEffect::Transform(val))
                }
                _ => Step::Done(0),
            },
            PipelineState::TransformPending => match signal {
                ResumeSignal::Resume(val) => {
                    self.state = PipelineState::CommitPending;
                    Step::Yielded(AsyncIoEffect::Commit(val))
                }
                _ => Step::Done(0),
            },
            PipelineState::CommitPending => match signal {
                ResumeSignal::Resume(val) => {
                    self.state = PipelineState::Done;
                    Step::Done(val)
                }
                _ => Step::Done(0),
            },
            PipelineState::Done => Step::Done(0),
        }
    }
}

// ============================================================================
// 2. NATIVE TOKIO ASYNC/AWAIT FUTURE
// ============================================================================

async fn native_fetch(id: u64) -> u64 {
    tokio::task::yield_now().await;
    id + 1
}

async fn native_transform(val: u64) -> u64 {
    tokio::task::yield_now().await;
    val * 2
}

async fn native_commit(val: u64) -> u64 {
    tokio::task::yield_now().await;
    val + 10
}

async fn run_native_future_pipeline(id: u64) -> u64 {
    let a = native_fetch(id).await;
    let b = native_transform(a).await;
    native_commit(b).await
}

// ============================================================================
// BENCHMARK SUITE
// ============================================================================

fn bench_effect_vs_future(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create Tokio runtime");

    // Print memory layout footprint analysis
    let native_fut = run_native_future_pipeline(0);
    println!("--------------------------------------------------");
    println!("MEMORY FOOTPRINT COMPARISON (size_of):");
    println!(
        "  PipelineFsm (machine-rs)   : {} bytes",
        size_of::<PipelineFsm>()
    );
    println!(
        "  Native async Future layout : {} bytes",
        size_of_val(&native_fut)
    );
    println!("--------------------------------------------------");
    drop(native_fut);

    let mut group = c.benchmark_group("effect_fsm_vs_native_future");
    let batch_size: u64 = 500;

    group.bench_function(
        BenchmarkId::new("machine_rs_handle_async", batch_size),
        |b| {
            b.to_async(&rt).iter(|| async {
                let mut total = 0u64;
                for i in 0..batch_size {
                    let fsm = PipelineFsm::new(black_box(i));
                    let res = handle_async(fsm, |eff| async move {
                        match eff {
                            AsyncIoEffect::Fetch(id) => Some(native_fetch(id).await),
                            AsyncIoEffect::Transform(val) => Some(native_transform(val).await),
                            AsyncIoEffect::Commit(val) => Some(native_commit(val).await),
                        }
                    })
                    .await;
                    total = total.wrapping_add(res.unwrap_or(0));
                }
                black_box(total)
            });
        },
    );

    group.bench_function(
        BenchmarkId::new("native_tokio_async_await", batch_size),
        |b| {
            b.to_async(&rt).iter(|| async {
                let mut total = 0u64;
                for i in 0..batch_size {
                    let res = run_native_future_pipeline(black_box(i)).await;
                    total = total.wrapping_add(res);
                }
                black_box(total)
            });
        },
    );

    group.finish();
}

criterion_group!(benches, bench_effect_vs_future);
criterion_main!(benches);
