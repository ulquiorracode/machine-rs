use criterion::{Criterion, black_box, criterion_group, criterion_main};
use machine_core::prelude::*;

struct CounterGen {
    curr: u64,
    max: u64,
}

impl Machine for CounterGen {
    type Yield = u64;
    type Resume = ();
    type Return = ();

    #[inline(always)]
    fn step(&mut self, _resume: ()) -> Step<Self::Yield, Self::Return> {
        if self.curr < self.max {
            let val = self.curr;
            self.curr += 1;
            Step::Yielded(val)
        } else {
            Step::Done(())
        }
    }
}

impl FusedMachine for CounterGen {}

struct NativeCounter {
    curr: u64,
    max: u64,
}

impl Iterator for NativeCounter {
    type Item = u64;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.curr < self.max {
            let val = self.curr;
            self.curr += 1;
            Some(val)
        } else {
            None
        }
    }
}

fn bench_generator_vs_native_iterator(c: &mut Criterion) {
    let mut group = c.benchmark_group("fsm_vs_native_iterator");

    group.bench_function("machine_gen_into_iter", |b| {
        b.iter(|| {
            let generator = CounterGen {
                curr: 0,
                max: black_box(1_000),
            };
            let sum: u64 = generator.into_iter().sum();
            black_box(sum)
        })
    });

    group.bench_function("native_rust_iterator", |b| {
        b.iter(|| {
            let iter = NativeCounter {
                curr: 0,
                max: black_box(1_000),
            };
            let sum: u64 = iter.sum();
            black_box(sum)
        })
    });

    group.finish();
}

criterion_group!(benches, bench_generator_vs_native_iterator);
criterion_main!(benches);
