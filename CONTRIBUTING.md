# Contributing to `machine-rs`

Thank you for your interest in contributing to **`machine-rs`**! We welcome contributions of all kinds: performance optimizations, bug fixes, architecture improvements, documentation, and benchmark suites.

Please take a moment to review these guidelines before submitting code.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Branching Strategy](#branching-strategy)
- [Conventional Commits](#conventional-commits)
- [Development Setup](#development-setup)
- [Coding Standards & Core Invariants](#coding-standards--core-invariants)
- [Pull Request Workflow](#pull-request-workflow)

---

## Code of Conduct

This project and everyone participating in it is governed by our [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

---

## Branching Strategy

Our repository strictly adheres to the standard GitFlow-inspired branching strategy:

- **`main`**: Production/stable releases only. Protected branch; direct commits are strictly prohibited.
- **`dev`**: Active development integration branch. Protected branch; all feature, fix, and dependency PRs must target `dev`.
- **`feature/<name>`**: New features and enhancements, branched from `dev` and merged back into `dev` via PR.
- **`fix/<name>`**: Bug fixes, branched from `dev` (or `main` for critical production hotfixes).

```text
main   ───────────────────────────● (v0.1.0)
                                 /
dev    ──────●──────────●───────●─── (Active Development)
              \        /
feature/async  ●──────●             (Feature Branch)
```

---

## Conventional Commits

Commit messages must follow the [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) specification:

```text
<type>(<optional-scope>): <description in imperative mood>

[optional body]

[optional footer(s)]
```

### Allowed Types

| Type | Description | Example |
| :--- | :--- | :--- |
| `feat` | A new feature or capability | `feat(coproduct): add flat Coproduct! type macro` |
| `fix` | A bug fix | `fix(benches): replace deprecated criterion::black_box with std::hint::black_box` |
| `perf` | A code change that improves performance | `perf(generator): eliminate bounds check in into_iter projection` |
| `refactor` | Code change that neither fixes a bug nor adds a feature | `refactor(core): decouple effect traits from std runtime` |
| `docs` | Documentation changes | `docs(readme): add control flow architecture diagram` |
| `test` | Adding or updating tests | `test(edge): add abort and drop recovery tests` |
| `build` | Changes to build configuration or dependencies | `build(deps): bump criterion from 0.5.1 to 0.8.2` |
| `ci` | Changes to CI/CD pipeline | `ci: add Windows runner to CI matrix` |
| `chore` | Routine maintenance tasks | `chore: update repository metadata` |

---

## Development Setup

### Prerequisites

- **Rust**: Rust 2024 Edition (MSRV: `1.85.0` or newer stable).
- **Cargo Tools**:

  ```bash
  rustup component add clippy rustfmt
  ```

### Local Verification Suite

Run the full local verification pipeline before submitting any PR:

```bash
# 1. Format check
cargo fmt --all -- --check

# 2. Strict Clippy check with all targets and features
cargo clippy --all-targets --all-features -- -D warnings

# 3. Comprehensive unit and integration tests
cargo test --all-targets --all-features

# 4. Strict #![no_std] verification
cargo check --no-default-features -p machine-core

# 5. Documentation build check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

---

## Coding Standards & Core Invariants

1. **Strict `#![no_std]` Core**: `machine-core` must remain zero-dependency and 100% `#![no_std]`.
2. **Zero-Allocation Discipline**: Monomorphic state transition and effect dispatch must not allocate heap memory (`Box`, `Vec`, `String`).
3. **Mechanical Sympathy**: Design states with cache lines (64 bytes) in mind. Avoid pointer-chasing and dynamic vtables on the hot execution path.
4. **Safety & Invariants**:
   - Every `unsafe` block must include a clear `// SAFETY:` explanation.
   - Never allow unhandled panics or exceptions to cross boundaries silently.
5. **Fail-Fast Error Handling**: Never silently swallow errors or return dummy default values (`0`, `""`, `None`) on failure paths.

---

## Pull Request Workflow

1. Fork or branch from `dev`.
2. Implement your changes surgically, following the coding standards above.
3. Verify all checks pass locally using the commands listed in [Local Verification Suite](#local-verification-suite).
4. Open a Pull Request targeting `dev` with a clear description of changes and test evidence.
