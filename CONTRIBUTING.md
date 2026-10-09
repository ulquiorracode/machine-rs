# Contributing to `effect-rs`

Thank you for your interest in contributing to `effect-rs`!

## Code Quality Standards

1. **Strict `#![no_std]` Core**: `effect-core` must remain zero-dependency and 100% `#![no_std]`.
2. **Zero-Allocation Discipline**: The hot dispatch path must not allocate heap memory (`Box`, `Vec`, `String`).
3. **No Unsafe without Explanation**: Every `unsafe` block requires a `// SAFETY:` invariant comment.
4. **Formatting & Lints**:
   - `cargo fmt --all -- --check`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - `cargo test --all-targets`

## Branching Strategy

- `main` — Production/stable releases only.
- `dev` — Active development integration branch. All PRs must target `dev`.
- `feature/<name>` — Feature branches.
