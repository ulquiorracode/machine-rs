use machine_core::prelude::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

struct TrackingAllocator {
    alloc_count: AtomicUsize,
    dealloc_count: AtomicUsize,
    net_bytes: AtomicIsize,
}

impl TrackingAllocator {
    pub const fn new() -> Self {
        Self {
            alloc_count: AtomicUsize::new(0),
            dealloc_count: AtomicUsize::new(0),
            net_bytes: AtomicIsize::new(0),
        }
    }

    pub fn reset(&self) {
        self.alloc_count.store(0, Ordering::SeqCst);
        self.dealloc_count.store(0, Ordering::SeqCst);
        self.net_bytes.store(0, Ordering::SeqCst);
    }

    pub fn snapshot(&self) -> (usize, usize, isize) {
        (
            self.alloc_count.load(Ordering::SeqCst),
            self.dealloc_count.load(Ordering::SeqCst),
            self.net_bytes.load(Ordering::SeqCst),
        )
    }
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.alloc_count.fetch_add(1, Ordering::SeqCst);
        self.net_bytes
            .fetch_add(layout.size() as isize, Ordering::SeqCst);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.dealloc_count.fetch_add(1, Ordering::SeqCst);
        self.net_bytes
            .fetch_sub(layout.size() as isize, Ordering::SeqCst);
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
    Aborted,
}

struct EffectFSM {
    state: EffectState,
    val: u64,
}

impl Machine for EffectFSM {
    type Yield = MathEffect;
    type Resume = ResumeSignal<u64>;
    type Return = u64;

    fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
        match self.state {
            EffectState::Start => {
                self.state = EffectState::Done;
                Step::Yielded(MathEffect(self.val))
            }
            EffectState::Done => match signal {
                ResumeSignal::Resume(resp) => Step::Done(self.val + resp),
                ResumeSignal::Abort => {
                    self.state = EffectState::Aborted;
                    Step::Done(0)
                }
                ResumeSignal::Start => {
                    self.state = EffectState::Aborted;
                    Step::Done(0)
                }
            },
            EffectState::Aborted => Step::Done(0),
        }
    }
}

#[test]
fn verify_strictly_zero_allocations_on_generators_and_effects() {
    // Warmup allocator structures outside of tracked measurements
    let warmup_gen = RangeGen { curr: 0, max: 10 };
    let _warmup_sum: u64 = warmup_gen.into_iter().sum();

    ALLOC.reset();

    // 1. Generator to Iterator sum over 100,000 items
    let generator = RangeGen {
        curr: 0,
        max: 100_000,
    };
    let sum: u64 = generator.into_iter().sum();
    assert_eq!(sum, 4999950000);

    let (allocs_gen, _, net_gen) = ALLOC.snapshot();
    assert_eq!(
        allocs_gen, 0,
        "Generator into_iter performed {allocs_gen} heap allocations"
    );
    assert_eq!(
        net_gen, 0,
        "Generator into_iter allocated {net_gen} net bytes"
    );

    // 2. 2-Way Effect dispatch over 10,000 iterations
    ALLOC.reset();
    for i in 0..10_000 {
        let fsm = EffectFSM {
            state: EffectState::Start,
            val: i,
        };
        let res = handle_sync(fsm, |eff| Some(eff.0 * 2));
        assert_eq!(res, Some(i * 3));
    }

    let (allocs_eff, _, net_eff) = ALLOC.snapshot();
    assert_eq!(
        allocs_eff, 0,
        "Effect handle_sync performed {allocs_eff} heap allocations"
    );
    assert_eq!(
        net_eff, 0,
        "Effect handle_sync allocated {net_eff} net bytes"
    );

    // 3. 2-Way Effect abort over 1,000 iterations
    ALLOC.reset();
    for i in 0..1_000 {
        let fsm = EffectFSM {
            state: EffectState::Start,
            val: i,
        };
        let res = handle_sync(fsm, |_eff| None);
        assert_eq!(res, None);
    }

    let (allocs_abort, _, net_abort) = ALLOC.snapshot();
    assert_eq!(
        allocs_abort, 0,
        "Effect handle_sync abort path performed {allocs_abort} heap allocations"
    );
    assert_eq!(
        net_abort, 0,
        "Effect handle_sync abort path allocated {net_abort} net bytes"
    );
}
