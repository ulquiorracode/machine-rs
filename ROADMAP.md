# Roadmap (`machine-rs`)

## Milestone v0.1.0 — Foundational FSM Kernel (Current)

- [x] `#![no_std]` core trait definitions: `Machine`, `Step`, `Generator`, `GenIter`, `Effect`, `ResumeSignal`.
- [x] Complete Cancellation / Abort Protocol (`ResumeSignal::Abort`, `debug_assert!` invariants, `close()`).
- [x] Zero-Allocation synchronous runner (`handle_sync`).
- [x] Asynchronous driver (`handle_async`) for Tokio runtimes without function coloring.
- [x] Multi-platform CI matrix (Ubuntu, Windows, macOS) with `#![no_std]` check and docs linting.
- [x] Zero-allocation proof with `TrackingAllocator` (0 bytes net allocations).
- [x] Adversarial edge-case stress test suite (unwind safety, empty generator, multi-step chain).
- [x] Production Criterion benchmarks: 1-way generator parity, 2-way sync effect vs dyn (5.34x speedup), async effect vs Tokio future.

## Milestone v0.2.0 — Typed Open Effect Sums & Composition

- [ ] Row polymorphism / extensible open effect coproducts without manual monolithic enums.
- [ ] GAT-based typed resume channels without downcasting.
- [ ] Ergonomic declarative FSM construction macros.

## Milestone v0.3.0 — Systems & Pipeline Integration

- [ ] Seamless integration adapter for `stitch-rs` Sewing Machine Architecture (SMA) U-cycle pipelines.
- [ ] Native GoldSrc.rs plugin effect host.
- [ ] High-throughput streaming adapters over `core::iter::Iterator`.
