# Algebraic Effects in Rust (`effect-rs`)

[![CI](https://github.com/ulquiorracode/effect-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/ulquiorracode/effect-rs/actions)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

Zero-Cost, Monomorphic **Algebraic Effects** for Rust.

Eliminates the **function coloring problem** (`async`/`await`, `throws`, IO contagion) by strictly decoupling domain intentions (algebraic effects) from execution environments (handlers).

---

## The Vision: Uncolored, Pure Logic

In standard Rust, an IO or concurrency requirement at the leaf of a call graph contaminates the entire stack with `async fn`, `.await`, `Send + 'static` lifetimes, and runtime bindings.

With **Algebraic Effects**:
1. **Functions are uncolored and pure**: They simply yield data models representing effects.
2. **Handlers decide the environment**:
   - In unit tests: Synchronous, instant mock responses without network or mocks.
   - In embedded/game loop (`no_std`): Zero-allocation contiguous stack execution.
   - In servers: Asynchronous Tokio execution without polluting domain code with `async`.

---

## Architectural Taxonomy

```
+-----------------------------------------------------------+
| Pure Domain / Business Logic (NO_STD, ZERO ASYNC)         |
|                                                           |
| StateMachine yields Effect::Ask { prompt }                |
+-----------------------------------------------------------+
                              |
                              v
+-----------------------------------------------------------+
| Environment Handler (effect_match! / handle_sync / async) |
|                                                           |
| effect_match!(machine, {                                  |
|     perform Effect::Ask(prompt) => resume!("Alice"),      |
|     return user => println!("Done: {user}"),              |
| })                                                        |
+-----------------------------------------------------------+
```

---

## Ergonomics: `effect_match!` Syntax

Mirroring the cleanest algebraic effects syntax:

```rust
use effect_rs::prelude::*;

let result = effect_match!(machine, {
    perform AppEffect::Ping(_ping) => {
        "Pong"
    }
    perform AppEffect::Ask(prompt) => {
        "Alice"
    }
    return val => {
        val
    }
});
```

---

## Workspace Structure

- [`effect-core`](crates/effect-core): `#![no_std]` foundational contracts (`Effect`, `EffectMachine`, `Step`, `handle_sync`).
- [`effect-macros`](crates/effect-macros): Procedural macros and declarative syntactic sugar (`effect_match!`).
- [`effect-async`](crates/effect-async): Asynchronous runtime bridge and driver (`handle_async`).
- [`effect-rs`](crates/effect-rs): Root facade crate.

---

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
