# Machine RS (`machine-rs`)

<!-- Project Status & Metrics -->
![Status](https://img.shields.io/badge/status-active%20development-orange?logo=rust) [![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE) [![CI](https://github.com/ulquiorracode/machine-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/ulquiorracode/machine-rs/actions) [![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?logo=readme)](https://github.com/RichardLitt/standard-readme)  
<!-- Repository & Community -->
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md) [![MSRV: 1.85.0](https://img.shields.io/badge/MSRV-1.85.0-blue.svg?logo=rust)](https://doc.rust-lang.org/edition-guide/rust-2024/) [![no_std](https://img.shields.io/badge/no__std-compatible-brightgreen.svg)](crates/machine-core)  
<!-- Targets -->
![Targets: Windows | Linux | macOS](https://img.shields.io/badge/targets-windows%20%7C%20linux%20%7C%20macos-lightgray.svg?logo=linux&logoColor=black)

> Universal, Zero-Cost State Machine Kernel for Rust unifying **1-Way Generators/Iterators** and **2-Way Algebraic Effects/Futures** under a single monomorphic finite state machine.

---

## The Vision: Unifying Control Flow without Colored Functions

In modern programming languages, control flow is fragmented:

- **Pull streams** are written via `Iterator`.
- **Generators** are trapped in unstable RFCs (`gen` / `yield`).
- **I/O & Concurrency** infect codebases with colored `async fn` and runtime locks.

`machine-rs` provides a mathematically unified **FSM Kernel** (`Machine<Yield, Resume, Return>`):

```test
                                  [ Machine<Yield, Resume, Return> ]
                                                  |
                 +--------------------------------+--------------------------------+
                 | (1-Way: Sink / Stream)                          | (2-Way: Dialogue / Protocol)
                 v                                                 v
        [ Generators ]                                   [ Algebraic Effects ]
        Resume = ()                                      Resume = ResumeSignal<Response>
                 |                                                 |
                 +---> core::iter::Iterator                        +---> Pure Synchronous Mocks
                 +---> Inlined CPU loops                           +---> Tokio / Network I/O Driver
```

---

## 1-Way Projection: Generators & Iterators

A generator is a state machine whose resumption payload is the Zero-Sized Type `()` (`Resume = ()`).
LLVM mem2reg eliminates the zero-sized argument, compiling `.into_iter()` loops into tight, inlined assembly.

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
            let sum = self.a.saturating_add(self.b);
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

## 2-Way Projection: Typed Algebraic Effects

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

---

## Empirical Performance Scoreboard

Empirical measurements conducted with [Criterion.rs](https://github.com/bheisler/criterion.rs) on Stable Rust 1.85+ (detailed analysis in [BENCHMARKS.md](BENCHMARKS.md)):

| Benchmark Domain | `machine-rs` | Baseline (`std` / `dyn` / Tokio) | Delta / Speedup |
| :--- | :---: | :---: | :---: |
| **1-Way FSM Generator vs Native Iterator** | **256.64 ns** | 256.90 ns (`core::iter::Iterator`) | **Parity (0.256 ns/step, 100% loop parity)** |
| **2-Way Sync Effect vs `&mut dyn Trait`** | **6.10 µs** | 20.34 µs (vtable indirect dispatch) | **3.33x faster** (BTB branch-friendly) |
| **2-Way Async Effect vs Tokio Future (Latency)** | **24.55 µs** | 28.33 µs (Compiler-generated Future) | **1.15x faster (+15.4% throughput)** |
| **2-Way Async Effect vs Tokio Future (Memory)** | **16 bytes** | 56 bytes (Compiler-generated Future) | **3.5x smaller footprint** |
| **Heap Allocations on Hot Paths** | **0 bytes** | 0 bytes | **Strictly Zero Heap Allocations** |

See [BENCHMARKS.md](BENCHMARKS.md) for reproduction steps, hardware specifications, and deep architectural commentary.

---

## Workspace Structure

- [`machine-core`](crates/machine-core): `#![no_std]` core kernel contracts (`Machine`, `Step`, `Generator`, `GenIter`, `Effect`, `ResumeSignal`).
- [`machine-async`](crates/machine-async): Thread-safe `handle_async` Tokio driver bridge.
- [`machine-rs`](crates/machine-rs): Root facade re-exporting the full engine and extension traits (`MachineExt`).

---

## Deep Architecture

For memory invariants, register ABI analysis, and theoretical foundations, inspect [ARCHITECTURE.md](ARCHITECTURE.md).

---

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
