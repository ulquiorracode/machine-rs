# Architecture Specification (`effect-rs`)

## Theoretical Foundations

Algebraic Effects decouple **what** a program intends to do (the effect) from **how** that intention is executed (the handler).

In contrast to monadic I/O or colored functions (`async`/`await`), algebraic effects preserve the purity and uncolored nature of functions across arbitrarily deep call chains.

```
+----------------------------------------------------------------------+
| Pure Domain Logic (Uncolored fn, #![no_std], Zero Dependencies)      |
|                                                                      |
| let user_id = perform!(Ask { prompt: "ID?" });                       |
| perform!(Log { message: "Loaded" });                                 |
+----------------------------------------------------------------------+
                                  |
                                  | Yielded(Effect)
                                  v
+----------------------------------------------------------------------+
| Environment Execution Driver (handle_sync / handle_async)             |
|                                                                      |
| - Sync Mode: Instant pure resolution via array / mock (0 alloc, 1ns) |
| - Async Mode: Tokio socket / DB pool resolution at application edge  |
| - Abort Protocol: Step::Aborted cleans resources without panic       |
+----------------------------------------------------------------------+
```

## State Machine Representation (FSM)

An effectful computation is modeled as a monomorphic, stackless finite state machine implementing [`EffectMachine`]:

```rust
pub trait EffectMachine {
    type Effect: Effect;
    type Output;

    fn resume(
        &mut self,
        resume: Option<<Self::Effect as Effect>::Resume>,
    ) -> Step<Self::Effect, Self::Output>;

    fn close(&mut self);
}
```

### Transition Invariants
1. **Initial Step**: Called with `resume = None`. Must either yield an effect or complete immediately.
2. **Intermediate Steps**: Called with `resume = Some(val)` corresponding to the previously yielded effect.
3. **Completion**: Transitions to `Step::Done(output)`. Subsequent resume calls are contract violations.
4. **Abort Protocol**: If the handler chooses not to resume the computation, `close()` is invoked to trigger resource cleanups deterministically without triggering panics.
