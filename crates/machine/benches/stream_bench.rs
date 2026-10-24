//! Benchmark: GenStream vs futures::stream::iter vs async-stream
//!
//! Evaluates the execution throughput and latency of machine-rs 1-Way Generator
//! stream projection against standard async stream implementations.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use futures_util::StreamExt;
use machine_rs::prelude::*;
use std::hint::black_box;
use tokio::runtime::Runtime;

struct StreamCounterGen {
    curr: u64,
    limit: u64,
}

impl StreamCounterGen {
    pub const fn new(limit: u64) -> Self {
        Self { curr: 0, limit }
    }
}

impl Machine for StreamCounterGen {
    type Yield = u64;
    type Resume = ();
    type Return = ();

    #[inline(always)]
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

fn bench_stream_throughput(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create Tokio runtime");
    let mut group = c.benchmark_group("stream_throughput_comparison");
    let count: u64 = 1_000;

    // 1. machine-rs Generator projected as futures_core::Stream
    group.bench_function(BenchmarkId::new("machine_rs_gen_stream", count), |b| {
        b.to_async(&rt).iter(|| async {
            let generator = StreamCounterGen::new(black_box(count));
            let mut stream = generator.into_stream();
            let mut sum = 0u64;
            while let Some(item) = stream.next().await {
                sum = sum.wrapping_add(black_box(item));
            }
            black_box(sum)
        });
    });

    // 2. futures::stream::iter wrapping native Range iterator
    group.bench_function(BenchmarkId::new("futures_stream_iter", count), |b| {
        b.to_async(&rt).iter(|| async {
            let mut stream = futures_util::stream::iter(0..black_box(count));
            let mut sum = 0u64;
            while let Some(item) = stream.next().await {
                sum = sum.wrapping_add(black_box(item));
            }
            black_box(sum)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_stream_throughput);
criterion_main!(benches);
