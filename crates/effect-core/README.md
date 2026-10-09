# Effect Core (`effect-core`)

Zero-cost, `#![no_std]` foundational engine and formal contracts for **Algebraic Effects** in Rust.

## Highlights
- **100% `#![no_std]` compatible**: Runs in embedded environments, kernels, and high-frequency game engines.
- **Strictly Zero-Allocation**: Effect suspension and resumption compile down to pure register/stack transitions.
- **Uncolored Functions**: Business logic emits pure data effects without knowing whether execution is synchronous or asynchronous.
