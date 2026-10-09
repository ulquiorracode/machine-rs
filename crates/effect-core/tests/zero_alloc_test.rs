use effect_core::prelude::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct TrackingAllocator {
    alloc_count: AtomicUsize,
    allocated_bytes: AtomicUsize,
}

impl TrackingAllocator {
    pub const fn new() -> Self {
        Self {
            alloc_count: AtomicUsize::new(0),
            allocated_bytes: AtomicUsize::new(0),
        }
    }

    pub fn reset(&self) {
        self.alloc_count.store(0, Ordering::SeqCst);
        self.allocated_bytes.store(0, Ordering::SeqCst);
    }

    pub fn counts(&self) -> (usize, usize) {
        (
            self.alloc_count.load(Ordering::SeqCst),
            self.allocated_bytes.load(Ordering::SeqCst),
        )
    }
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.alloc_count.fetch_add(1, Ordering::SeqCst);
        self.allocated_bytes
            .fetch_add(layout.size(), Ordering::SeqCst);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: TrackingAllocator = TrackingAllocator::new();

struct ComputeEffect(u64);
impl Effect for ComputeEffect {
    type Resume = u64;
}

enum ComputeState {
    Step1,
    Step2,
    Done,
}

struct ComputeMachine {
    state: ComputeState,
    acc: u64,
}

impl EffectMachine for ComputeMachine {
    type Effect = ComputeEffect;
    type Output = u64;

    fn resume(&mut self, resume: Option<u64>) -> Step<Self::Effect, Self::Output> {
        match self.state {
            ComputeState::Step1 => {
                self.state = ComputeState::Step2;
                Step::Yielded(ComputeEffect(self.acc))
            }
            ComputeState::Step2 => {
                self.acc += resume.unwrap_or(0);
                self.state = ComputeState::Done;
                Step::Done(self.acc)
            }
            ComputeState::Done => Step::Aborted,
        }
    }
}

#[test]
fn verify_strictly_zero_heap_allocations_on_effect_dispatch() {
    ALLOC.reset();

    // 1. Test handle_sync
    for i in 0..10_000 {
        let machine = ComputeMachine {
            state: ComputeState::Step1,
            acc: i,
        };
        let res = handle_sync(machine, |eff| EffectDecision::Resume(eff.0 + 1));
        assert_eq!(res, Some(i * 2 + 1));
    }

    let (allocs, bytes) = ALLOC.counts();
    assert_eq!(
        allocs, 0,
        "Effect machine violated zero-allocation contract: performed {allocs} heap allocations"
    );
    assert_eq!(
        bytes, 0,
        "Effect machine allocated {bytes} bytes of heap memory"
    );

    // 2. Test effect_match! macro
    ALLOC.reset();
    for i in 0..10_000 {
        let machine = ComputeMachine {
            state: ComputeState::Step1,
            acc: i,
        };
        let res = effect_match!(machine, {
            perform ComputeEffect(val) => EffectDecision::Resume(val + 1),
            return out => out,
        });
        assert_eq!(res, Some(i * 2 + 1));
    }

    let (allocs_macro, bytes_macro) = ALLOC.counts();
    assert_eq!(
        allocs_macro, 0,
        "effect_match! macro performed {allocs_macro} heap allocations"
    );
    assert_eq!(
        bytes_macro, 0,
        "effect_match! macro allocated {bytes_macro} bytes"
    );
}
