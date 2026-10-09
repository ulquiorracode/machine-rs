# Effect Async (`effect-async`)

Asynchronous runtime bridge and driver for **Algebraic Effects** in Rust.

## Highlights
- **Isolates Asynchrony**: Business logic and domain state machines remain uncolored, pure, and synchronous.
- **Asynchronous Driver**: `handle_async` bridges effect resolution to Tokio, async-std, or any async executor.
