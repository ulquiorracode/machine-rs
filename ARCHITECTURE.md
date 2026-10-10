# Architecture Specification (`machine-rs`)

## Theoretical Foundations

`machine-rs` provides a mathematically grounded, stackless **Finite State Machine (FSM) Kernel** that unifies two fundamental branches of program control flow:

1. **1-Way Control Flow (Generators & Iterators)**:
   - Data flows strictly outward from the producer to the consumer.
   - Resumption carries the Zero-Sized Type `()`.
   - Projects zero-cost into `core::iter::Iterator`.

2. **2-Way Control Flow (Algebraic Effects & Co-routines)**:
   - Computation yields an intention (`Effect`) to the execution environment.
   - Environment evaluates the effect and injects a typed response via [`ResumeSignal`].
   - Supports deterministic cancellation and cooperative abortion without process panics.

```text
                                  [ Machine<Yield, Resume, Return> ]
                                                  |
                 +--------------------------------+--------------------------------+
                 | (1-Way: Generator / Sink)                       | (2-Way: Effect / Protocol)
                 v                                                 v
        [ Generator ]                                     [ Effect ]
        Resume = ()                                       Resume = ResumeSignal<Response>
                 |                                                 |
                 +---> GenIter<M> -> Iterator                      +---> handle_sync (Deterministic in-memory)
                 +---> Inlined LLVM loop heads                     +---> handle_async (Tokio network I/O)
```

---

## The FSM Kernel Contract

All machines implement the foundational [`Machine`] trait:

```rust
pub trait Machine {
    type Yield;
    type Resume;
    type Return;

    fn step(&mut self, resume: Self::Resume) -> Step<Self::Yield, Self::Return>;

    fn close(&mut self) {}
}
```

### Transition Invariants

1. **The Single-Return Invariant**: Once `step` yields `Step::Done(return_value)`, the machine has reached terminal completion. Subsequent calls to `step` are contract violations.
2. **GenIter Termination Cache**: [`GenIter`] caches the completion state (`is_done = true`) and guarantees that trailing calls to `Iterator::next()` return `None` without ever polling the underlying machine again.
3. **Cancellation Protocol**: When an environment handler declines to service an effect (returns `None`), the machine is signaled with `ResumeSignal::Abort`, and `Machine::close()` is invoked deterministically to release external handles or locks.

---

## Microarchitectural Invariants & Mechanical Sympathy

### 1. Register ABI Allocation (`Step<Y, R>`)

`Step<Y, R>` is designed to fit directly within CPU general-purpose registers without touching stack frames:

- On x86-64 System V / Microsoft x64 ABI, small enums where payload is $\le 16$ bytes are passed and returned directly in registers (`RAX` and `RDX`).
- Zero-Sized Types (`Resume = ()`) incur zero register usage and zero ABI overhead.

### 2. Elimination of Dynamic Dispatch (`vtable`)

Traditional polymorphic patterns (`Box<dyn Service>` or `&mut dyn Service`) require:

1. Loading the `vtable` pointer from object metadata.
2. An indirect `CALL [RAX + offset]` instruction.
3. Subverting hardware Branch Target Buffer (BTB) branch prediction.

In `machine-rs`, drivers (`handle_sync`, `handle_async`) are **strictly monomorphic compile-time templates**:

- `step()` and effect handlers inline directly at call sites.
- Branching is converted by LLVM into direct unconditional or predictable conditional jumps.
- Yields a **3.33x speedup** over `&mut dyn Trait` (see [BENCHMARKS.md](BENCHMARKS.md)).

### 3. Memory Footprint vs Compiler-Generated Futures

When the Rust compiler generates an `async fn` or `async block` state machine, it must conservatively spill all local variables that cross `.await` points into an anonymous generator frame (often $56+$ bytes).
A pure domain `Machine` in `machine-rs`:

- Explicitly models only the minimal discriminant and active state fields (e.g., 16 bytes for a 3-step pipeline).
- Occupies a fraction of a 64-byte L1 cache line ($25\%$), reducing cache thrashing in high-concurrency event loops.
