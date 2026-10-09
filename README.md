# Machine RS (`machine-rs`)

[![CI](https://github.com/ulquiorracode/machine-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/ulquiorracode/machine-rs/actions)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

Universal, Zero-Cost State Machine Kernel for Rust unifying **1-Way Generators/Iterators** and **2-Way Algebraic Effects/Futures** under a single monomorphic finite state machine.

---

## The Vision: Unifying Control Flow without Colored Functions

In modern programming languages, control flow is fragmented:
- **Pull streams** are written via `Iterator`.
- **Generators** are trapped in unstable RFCs (`gen` / `yield`).
- **I/O & Concurrency** infect codebases with colored `async fn` and runtime locks.

`machine-rs` provides a mathematically unified **FSM Kernel** (`Machine<Yield, Resume, Return>`):

```
                                  [ Machine<Yield, Resume, Return> ]
                                                  |
                 +--------------------------------+--------------------------------+
                 | (1-Way: Sink / Stream)                          | (2-Way: Dialogue / Protocol)
                 v                                                 v
        [ Generators ]                                   [ Algebraic Effects ]
        Resume = ()                                      Resume = Option<Response>
                 |                                                 |
                 +---> core::iter::Iterator                        +---> Pure Synchronous Mocks (1ns)
                 +---> Inlined CPU loops                           +---> Tokio / Network I/O Driver
```

---

## 1-Way Projection: Generators & Iterators

A generator is a state machine whose resumption payload is the Zero-Sized Type `()` (`Resume = ()`).
LLVM mem2reg completely eliminates `Resume = ()`, compiling `.into_iter()` loops into **zero-overhead, bit-for-bit identical assembly** to native Rust while loops.

```rust
use machine_rs::prelude::*;

struct Fibonacci {
    a: u64,
    b: u64,
    count: usize,
    max: usize,
}

impl Machine for Fibonacci {
    type Yield = u64;
    type Resume = ();
    type Return = ();

    fn step(&mut self, _resume: ()) -> Step<Self::Yield, Self::Return> {
        if self.count < self.max {
            let next = self.a;
            let sum = self.a + self.b;
            self.a = self.b;
            self.b = sum;
            self.count += 1;
            Step::Yielded(next)
        } else {
            Step::Done(())
        }
    }
}

// Convert seamlessly into a standard Rust Iterator:
let fib = Fibonacci { a: 0, b: 1, count: 0, max: 10 };
let sum: u64 = fib.into_iter().sum();
```

---

## 2-Way Projection: Typed Open Algebraic Effects

Domain logic remains uncolored and pure by yielding typed effect tokens. Execution environments handle effects deterministically:

```rust
use machine_rs::prelude::*;

struct AskName;
impl Effect for AskName {
    type Response = &'static str;
}

// Run in unit-tests with zero allocations and zero runtime:
let name = handle_sync(my_machine, |eff| {
    Some("Alice")
});
```

---

## Workspace Structure

- [`machine-core`](crates/machine-core): `#![no_std]` core kernel contracts (`Machine`, `Step`, `Generator`, `GenIter`, `Effect`, `Either`).
- [`machine-async`](crates/machine-async): Thread-safe `handle_async` Tokio driver bridge.
- [`machine-rs`](crates/machine-rs): Root facade re-exporting the full engine and extension traits (`MachineExt`).

---

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
