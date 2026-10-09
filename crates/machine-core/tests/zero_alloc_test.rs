use machine_core::prelude::*;
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

struct RangeGen {
    curr: u64,
    max: u64,
}

impl Machine for RangeGen {
    type Yield = u64;
    type Resume = ();
    type Return = ();

    #[inline(always)]
    fn step(&mut self, _resume: ()) -> Step<Self::Yield, Self::Return> {
        if self.curr < self.max {
            let v = self.curr;
            self.curr += 1;
            Step::Yielded(v)
        } else {
            Step::Done(())
        }
    }
}

impl FusedMachine for RangeGen {}

struct MathEffect(u64);
impl Effect for MathEffect {
    type Response = u64;
}

enum EffectState {
    Start,
    Done,
}

struct EffectFSM {
    state: EffectState,
    val: u64,
}

impl Machine for EffectFSM {
    type Yield = MathEffect;
    type Resume = Option<u64>;
    type Return = u64;

    fn step(&mut self, resume: Self::Resume) -> Step<Self::Yield, Self::Return> {
        match self.state {
            EffectState::Start => {
                self.state = EffectState::Done;
                Step::Yielded(MathEffect(self.val))
            }
            EffectState::Done => Step::Done(self.val + resume.unwrap_or(0)),
        }
    }
}

#[test]
fn verify_strictly_zero_allocations_on_generators_and_effects() {
    ALLOC.reset();

    // 1. Generator to Iterator sum over 100,000 items
    let generator = RangeGen {
        curr: 0,
        max: 100_000,
    };
    let sum: u64 = generator.into_iter().sum();
    assert_eq!(sum, 4999950000);

    // 2. 2-Way Effect dispatch over 10,000 iterations
    for i in 0..10_000 {
        let fsm = EffectFSM {
            state: EffectState::Start,
            val: i,
        };
        let res = handle_sync(fsm, |eff| Some(eff.0 * 2));
        assert_eq!(res, Some(i * 3));
    }

    let (allocs, bytes) = ALLOC.counts();
    assert_eq!(
        allocs, 0,
        "machine-core violated zero-allocation contract: performed {allocs} heap allocations"
    );
    assert_eq!(bytes, 0, "machine-core allocated {bytes} bytes");
}
