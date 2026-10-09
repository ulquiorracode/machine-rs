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

```
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
