# Roadmap (`effect-rs`)

## Milestone v0.1.0 — Foundational Engine (Current)
- [x] `#![no_std]` core trait definitions: `Effect`, `EffectMachine`, `Step`.
- [x] Complete Cancellation / Abort Protocol (`Step::Aborted`, `close()`).
- [x] Zero-Allocation synchronous runner (`handle_sync`).
- [x] Asynchronous driver (`handle_async`) with full `Send + 'static` support for Tokio.
- [x] Macro `effect_match!` (`macro_rules!`) for clean syntax without compiler bloat.
- [x] Multi-platform CI matrix (Ubuntu, Windows, macOS) with `no_std` check.
- [x] Zero-allocation proof with `TrackingAllocator`.
- [x] Real Criterion benchmark comparing `EffectMachine` vs `async fn`.

## Milestone v0.2.0 — Typed Open Effect Sums & Rows
- [ ] Row polymorphism / extensible open effect unions (`Either<E1, E2>`).
- [ ] GAT-based typed resume channels without sum enum downcasting.
- [ ] Generator support (`yield` separation vs `perform`).

## Milestone v0.3.0 — Advanced Runtime & Integration
- [ ] Deep continuations / fiber stack engine for multi-shot effects.
- [ ] Seamless integration adapter for `stitch-rs` U-cycle pipelines.
- [ ] Native GoldSrc.rs plugin effect host.
