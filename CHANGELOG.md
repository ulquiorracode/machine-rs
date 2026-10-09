# Changelog (`effect-rs`)

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-10-10

### Added
- Core foundational contracts (`Effect`, `EffectMachine`, `Step`) with `#![no_std]` support.
- Explicit Abort / Cancellation protocol with `Step::Aborted` and `close()` lifecycle contract.
- Lightweight zero-dependency `effect_match!` macro supporting `perform` and `return` arms.
- Asynchronous driver (`handle_async`) with thread-safe `Send` constraints for Tokio runtimes.
- Verification test suite including tracking allocator proof of strictly 0 heap bytes on execution hot paths.
- Comparative benchmarks measuring nanosecond dispatch latency against native Rust async state machines.
