# Benchmark Suite & Empirical Performance Analysis (`machine-rs`)

This document presents rigorous, reproducible benchmarks for `machine-rs`, demonstrating zero-cost compilation, memory footprint savings, and throughput comparisons against native Rust primitives (`core::iter::Iterator`, `std::future::Future`, `Box<dyn Trait>`, `&mut dyn Trait`).

---

## 1. Test Harness & Environment Specifications

All benchmarks are measured using [Criterion.rs](https://github.com/bheisler/criterion.rs) with statistical outlier detection, per-iteration compiler fences (`std::hint::black_box`), and verified release optimization flags (`opt-level = 3`, `lto = "fat"`, `codegen-units = 1`).

- **OS**: Windows 11 Enterprise (x86_64)
- **Rust Toolchain**: Stable Rust 2024 (`rustc 1.85+`)
- **Compilation Flags**: `RUSTFLAGS="-C target-cpu=native"`
- **Allocation Verification**: Custom `TrackingAllocator` measuring net allocations (`alloc - dealloc`) over 100,000 continuous steps.
- **Benchmark Run Timestamp**: 2026-10-10

---

## 2. Benchmark 1: 1-Way Control Flow (FSM Generator vs Native Rust `Iterator`)

### Objective

Prove that projecting a 1-Way Finite State Machine (`Machine<Resume = ()>`) into an `Iterator` via [`GenIter`] incurs **zero instruction and zero clock cycle overhead** compared to hand-written native Rust iterators.

### Methodology

To prevent LLVM from scalar-replacing the accumulation loop into Gauss's closed-form arithmetic progression $n(n - 1) / 2$ in $O(1)$ time, every single iteration is explicitly fenced using `black_box(val)`:

```rust
let mut sum = 0u64;
for val in generator.into_iter() {
    sum = sum.wrapping_add(black_box(val));
}
black_box(sum)
```

### Empirical Results (1,000 iterations per sample)

| Implementation | Batch Latency (1k ops) | Per-Step Latency | Net Allocations | Relative Overhead |
| :--- | :---: | :---: | :---: | :---: |
| **`machine-rs` (`GenIter<CounterGen>`)** | **256.64 ns** | **0.256 ns** (~1.1 CPU cycles) | **0 bytes** | **Baseline (0.0%)** |
| **Native Rust `Iterator` (`NativeCounter`)** | **256.90 ns** | **0.256 ns** (~1.1 CPU cycles) | **0 bytes** | **+0.1% (Parity)** |

> [!NOTE]
> When loops are prevented from being folded into algebraic formulas, both implementations execute in **256.6 ns** across 1,000 steps. The Zero-Sized Type `Resume = ()` is fully eliminated by LLVM, yielding bit-for-bit performance parity with hand-coded iterators.

---

## 3. Benchmark 2: 2-Way Sync Effects vs `dyn Trait` Polymorphism

### Objective

Evaluate the performance gain of monomorphic 2-way algebraic effects driven by [`handle_sync`] over classic dynamic dispatch (`&mut dyn Service` and `Box<dyn Service>`).

### Methodology

Both baselines and the effect driver execute identical 2-step storage transactions (Read key $\rightarrow$ Compute $\rightarrow$ Write updated value) across 10,000 iterations without artificial `#[inline(never)]` handicaps:

### Empirical Results (10,000 iterations per sample)

| Dispatch Mechanism | Total Latency (10k ops) | Per-Transaction Latency | Speedup vs vtable | Memory Indirection |
| :--- | :---: | :---: | :---: | :---: |
| **`machine-rs` (`handle_sync` Monomorphic Effect)** | **6.10 µs** | **0.61 ns** | **3.33x faster** | **0 pointer hops** |
| `dyn_trait_ref_vtable` (`&mut dyn StorageService`) | 20.34 µs | 2.03 ns | Baseline (1.00x) | vtable call indirection |
| `dyn_trait_box_vtable` (`Box<dyn StorageService>`) | 20.74 µs | 2.07 ns | 1.02x | 1 heap hop + vtable |

> [!TIP]
> Monomorphic effect handlers inline directly at the callsite. Because calls do not jump through a virtual method table (`vtable`), the CPU Branch Target Buffer (BTB) experiences zero indirect branch mispredictions, resulting in a **3.33x throughput increase**.

---

## 4. Benchmark 3: 2-Way Async Effects vs Tokio Native `async/await` Future

### Objective

Compare pure FSM domain logic driven via [`handle_async`] against native Rust `async fn / block` compiler-generated state machines on the Tokio runtime.

### Benchmark Target

A 3-step asynchronous I/O transaction pipeline (`FetchData` $\rightarrow$ `TransformData` $\rightarrow$ `CommitData`) across 500 batches.

| Architecture | Struct Footprint (`size_of`) | Cache Line Occupancy | Execution Latency (500 ops) | Relative Speedup | Function Coloring (`async`) |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **`machine-rs` (`handle_async_direct`)** | **16 bytes** | **25% (0.25 cache lines)** | **24.55 µs** (~49.1 ns/op) | **1.15x faster (+15.4%)** | **Zero (pure sync FSM)** |
| Native Tokio `async/await` Future | **56 bytes** | **87.5% (0.87 cache lines)** | **28.33 µs** (~56.6 ns/op) | Baseline (1.00x) | Required (`async/await`) |
| `machine-rs` (`handle_async` Closure) | **16 bytes** | **25% (0.25 cache lines)** | **33.49 µs** (~66.9 ns/op) | 0.85x | **Zero (pure sync FSM)** |

### Key Architectural Takeaways

1. **Beating Native `async/await` by 15.4%**: By utilizing [`AsyncEffectHandler::poll_effect`], intermediate Future wrappers are eliminated completely. The compiler monomorphizes the direct step loop into a single flat state machine with zero intermediate allocations.
2. **3.5x Smaller Memory Footprint**: Native `async` blocks generated by the Rust compiler capture all registers and local state across yield points (56 bytes). In contrast, the `machine-rs` FSM is strictly packed into **16 bytes**, easily fitting into two 64-bit registers (`RAX`/`RDX`) or a quarter of an L1 cache line.
3. **Zero Function Coloring**: Domain logic is written as pure, testable, deterministic state machines without contagious `async` keywords across internal libraries.
4. **Pure Future Implementation**: Both `handle_async` and `handle_async_direct` return zero-cost types implementing `core::future::Future` directly via manual `poll`, completely avoiding compiler-generated `async fn` wrappers under the hood.

---

## 5. How to Reproduce

Run the full benchmark suite locally:

```bash
# 1. Benchmark 1-Way Generator vs Native Iterator
cargo bench --bench generator_bench

# 2. Benchmark 2-Way Monomorphic Effects vs dyn Trait
cargo bench --bench sync_effect_vs_dyn_bench

# 3. Benchmark 2-Way Async Effects vs Tokio async/await
cargo bench --bench effect_vs_future_bench --features async
```
