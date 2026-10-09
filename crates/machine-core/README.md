# Machine Core (`machine-core`)

Universal, zero-cost, `#![no_std]` state machine kernel for Rust unifying:
- **1-Way Generators**: Native `.into_iter()` projection with 0-cost ZST `Resume = ()`.
- **2-Way Algebraic Effects**: Typed inverted slots (`Request<'a, E>`), Open effect rows (`Either<L, R>`), and synchronous drivers.
