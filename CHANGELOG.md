# Changelog (`machine-rs`)

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-10-10

### Added

- Universal State Machine Kernel unifying 1-way Generators and 2-way Algebraic Effects under `Machine<Yield, Resume, Return>`.
- Core foundational contracts (`Machine`, `Step`, `Generator`, `GenIter`, `Effect`, `ResumeSignal`) with full `#![no_std]` support.
- Termination cache in `GenIter` guaranteeing single-return invariant and idempotent `core::iter::FusedIterator` behavior.
- Deterministic Cancellation / Abort protocol with `ResumeSignal::Abort`, `debug_assert!` validation, and `Machine::close()` lifecycle contract.
- Thread-safe Tokio asynchronous driver (`handle_async`) enabling pure domain FSM execution without function coloring.
- Adversarial edge-case stress test suite verifying empty generators, multi-step effect chains, double aborts, and unwind/panic safety.
- Comprehensive Criterion benchmark suite verifying 100% assembly loop equivalence with native iterators (754 ps) and 5.34x speedup over `&mut dyn Trait`.
- Tracking allocator integration proving strictly 0 heap allocations across 100,000 steps.
