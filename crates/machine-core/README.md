# Machine Core (`machine-core`)

Universal, zero-cost, `#![no_std]` state machine kernel for Rust unifying:

- **1-Way Generators**: Native `.into_iter()` projection via [`GenIter`] with 0-cost ZST `Resume = ()` and termination state caching.
- **2-Way Algebraic Effects**: Typed [`ResumeSignal`] protocol (`Start`, `Resume`, `Abort`) and synchronous in-memory drivers with deterministic [`Machine::close`] cleanup.
