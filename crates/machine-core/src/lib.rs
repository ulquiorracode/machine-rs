//! # Machine Core (`machine-core`)
//!
//! Universal Zero-Cost State Machine Kernel unifying:
//! - **1-Way Control Flow**: [`Generator`] & [`GenIter`] (projections into `core::iter::Iterator`).
//! - **2-Way Control Flow**: [`Effect`] and synchronous drivers.
//! - **Kernel FSM**: [`Machine`] and [`Step`].
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

// ============================================================================
// 1. UNIVERSAL KERNEL PRIMITIVES
// ============================================================================

/// The outcome of an FSM transition step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step<Y, R> {
    /// Machine yielded intermediate control / payload.
    Yielded(Y),
    /// Machine completed execution with terminal return value.
    Done(R),
}

/// Universal Finite State Machine Kernel contract.
pub trait Machine {
    /// Payload emitted at suspension points.
    type Yield;
    /// Payload accepted on transition resumption.
    type Resume;
    /// Terminal value returned upon completion.
    type Return;

    /// Advance the FSM one transition step using the supplied resumption value.
    ///
    /// # Invariants
    /// - Calling `step` after returning `Step::Done` is a contract violation (poisoned/terminal state).
    fn step(&mut self, resume: Self::Resume) -> Step<Self::Yield, Self::Return>;

    /// Optional synchronous cleanup. Invoked on early cancellation or abort.
    #[inline]
    fn close(&mut self) {}
}

// ============================================================================
// 2. 1-WAY PROJECTION: GENERATORS & ITERATORS
// ============================================================================

/// A 1-way generator whose resumption payload is the Zero-Sized Type `()`.
pub trait Generator: Machine<Resume = ()> {}
impl<T: ?Sized + Machine<Resume = ()>> Generator for T {}

/// Zero-cost Iterator adapter over a 1-way Generator.
///
/// Ensures the FSM single-return invariant: once [`Step::Done`] is returned,
/// the machine is never polled again.
pub struct GenIter<M> {
    machine: M,
    is_done: bool,
}

impl<M> GenIter<M> {
    /// Constructs a new iterator adapter wrapping a generator.
    #[inline(always)]
    pub const fn new(machine: M) -> Self {
        Self {
            machine,
            is_done: false,
        }
    }
}

impl<M: Generator> core::iter::Iterator for GenIter<M> {
    type Item = M::Yield;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.is_done {
            return None;
        }

        match self.machine.step(()) {
            Step::Yielded(val) => Some(val),
            Step::Done(_) => {
                self.is_done = true;
                None
            }
        }
    }
}

impl<M: Generator> core::iter::FusedIterator for GenIter<M> {}

// ============================================================================
// 3. 2-WAY PROJECTION: ALGEBRAIC EFFECTS
// ============================================================================

/// Base trait defining an algebraic effect request.
pub trait Effect {
    /// Statically typed response expected from the effect handler.
    type Response;
}

// ============================================================================
// CORE SYSTEM EFFECT PRIMITIVES
// ============================================================================

/// Cooperative scheduling yield effect. Requests the runtime to yield control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct YieldNow;

impl Effect for YieldNow {
    type Response = ();
}

/// Timer or sleep effect requesting the runtime to pause execution for a duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sleep {
    /// Desired sleep duration in milliseconds.
    pub millis: u64,
}

impl Sleep {
    /// Constructs a sleep effect with the specified duration in milliseconds.
    #[inline(always)]
    pub const fn from_millis(millis: u64) -> Self {
        Self { millis }
    }
}

impl Effect for Sleep {
    type Response = ();
}

/// Explicit abort effect signaling terminal cancellation with a reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AbortEffect {
    /// Human-readable reason for the abort.
    pub reason: &'static str,
}

impl AbortEffect {
    /// Constructs an abort effect with the given reason.
    #[inline(always)]
    pub const fn new(reason: &'static str) -> Self {
        Self { reason }
    }
}

impl Effect for AbortEffect {
    /// Abort is terminal; its response is mathematically uninhabited.
    type Response = core::convert::Infallible;
}

/// Input protocol for driving two-way effect state machines.
///
/// Distinguishes the first initialization turn (where no prior effect occurred)
/// from subsequent resumptions with responses, or explicit cancellation/abort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeSignal<R> {
    /// Initial startup step.
    Start,
    /// Resumption with the environment's response to the prior effect.
    Resume(R),
    /// The environment aborted the operation early.
    Abort,
}

/// Synchronously drives an effect machine to completion using a handler closure.
///
/// Calls [`Machine::close`] deterministically if the handler aborts (returns `None`).
#[inline(always)]
pub fn handle_sync<M, E, H>(mut machine: M, mut handler: H) -> Option<M::Return>
where
    E: Effect,
    M: Machine<Yield = E, Resume = ResumeSignal<E::Response>>,
    H: FnMut(E) -> Option<E::Response>,
{
    let mut signal = ResumeSignal::Start;
    loop {
        match machine.step(signal) {
            Step::Yielded(eff) => {
                match handler(eff) {
                    Some(resp) => {
                        signal = ResumeSignal::Resume(resp);
                    }
                    None => {
                        // Notify machine of early abort, then close
                        let abort_step = machine.step(ResumeSignal::Abort);
                        debug_assert!(
                            matches!(abort_step, Step::Done(_)),
                            "machine yielded effect on abort signal rather than terminating"
                        );
                        machine.close();
                        return None;
                    }
                }
            }
            Step::Done(ret) => return Some(ret),
        }
    }
}

// ============================================================================
// 4. OPEN EFFECT SUMS & COPRODUCTS (ROW POLYMORPHISM)
// ============================================================================

/// Represents a binary coproduct (open sum) of two effects or responses.
///
/// Enables composition of independent effect domain models without requiring
/// a monolithic manual application enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Either<A, B> {
    /// Left variant.
    Left(A),
    /// Right variant.
    Right(B),
}

impl<A, B> Either<A, B> {
    /// Returns `true` if this is the `Left` variant.
    #[inline(always)]
    pub const fn is_left(&self) -> bool {
        matches!(self, Self::Left(_))
    }

    /// Returns `true` if this is the `Right` variant.
    #[inline(always)]
    pub const fn is_right(&self) -> bool {
        matches!(self, Self::Right(_))
    }

    /// Converts into `Option<A>` if this is `Left`.
    #[inline(always)]
    pub fn left(self) -> Option<A> {
        match self {
            Self::Left(a) => Some(a),
            Self::Right(_) => None,
        }
    }

    /// Converts into `Option<B>` if this is `Right`.
    #[inline(always)]
    pub fn right(self) -> Option<B> {
        match self {
            Self::Left(_) => None,
            Self::Right(b) => Some(b),
        }
    }
}

impl<A: Effect, B: Effect> Effect for Either<A, B> {
    type Response = Either<A::Response, B::Response>;
}

/// Helper trait for wrapping a left effect in a binary coproduct.
pub trait IntoLeft<B>: Sized {
    /// Injects `self` into the `Left` branch of [`Either`].
    #[inline(always)]
    fn left(self) -> Either<Self, B> {
        Either::Left(self)
    }
}

impl<A, B> IntoLeft<B> for A {}

/// Helper trait for wrapping a right effect in a binary coproduct.
pub trait IntoRight<A>: Sized {
    /// Injects `self` into the `Right` branch of [`Either`].
    #[inline(always)]
    fn right(self) -> Either<A, Self> {
        Either::Right(self)
    }
}

impl<A, B> IntoRight<A> for B {}

/// Empty terminal type for coproduct trees (analogous to `void` or empty sum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Nil {}

impl Effect for Nil {
    type Response = Nil;
}

/// Type macro for declaring flat, clean coproduct signatures without nested `Either` brackets.
///
/// # Example
/// ```rust
/// use machine_core::prelude::*;
///
/// struct E1; impl Effect for E1 { type Response = (); }
/// struct E2; impl Effect for E2 { type Response = (); }
/// struct E3; impl Effect for E3 { type Response = (); }
///
/// // Expands to: Either<E1, Either<E2, Either<E3, Nil>>>
/// type MyEffects = Coproduct!(E1, E2, E3);
/// ```
#[macro_export]
macro_rules! Coproduct {
    () => {
        $crate::Nil
    };
    ($head:ty $(,)?) => {
        $crate::Either<$head, $crate::Nil>
    };
    ($head:ty, $($tail:ty),+ $(,)?) => {
        $crate::Either<$head, $crate::Coproduct!($($tail),+)>
    };
}

// Marker types for type-level Peano indexing into nested Either coproducts
#[doc(hidden)]
pub struct Here;
#[doc(hidden)]
pub struct There<Next>(core::marker::PhantomData<Next>);

/// Trait for automatically injecting a variant into an arbitrary-depth [`Either`] coproduct.
///
/// Discovered at compile-time via type-level Peano indexing without runtime overhead.
pub trait CoproductInject<Target, Index> {
    /// Injects `self` into the target coproduct at compile-time.
    fn inject(self) -> Target;
}

// Base case 1: item is the Left variant of Either<Self, R>
impl<L, R> CoproductInject<Either<L, R>, Here> for L {
    #[inline(always)]
    fn inject(self) -> Either<L, R> {
        Either::Left(self)
    }
}

// Inductive step: item is inside the Right branch of Either<L, R>
impl<L, R, T, Next> CoproductInject<Either<L, R>, There<Next>> for T
where
    T: CoproductInject<R, Next>,
{
    #[inline(always)]
    fn inject(self) -> Either<L, R> {
        Either::Right(self.inject())
    }
}

/// Extension trait enabling ergonomic `.inject()` calls on any injectible effect.
pub trait InjectExt: Sized {
    /// Injects `self` into the target coproduct type `Target`.
    #[inline(always)]
    fn inject_coproduct<Target, Index>(self) -> Target
    where
        Self: CoproductInject<Target, Index>,
    {
        CoproductInject::inject(self)
    }
}

impl<T: Sized> InjectExt for T {}

/// Combinator combining two synchronous effect handlers into a single branched handler.
#[inline(always)]
#[allow(clippy::type_complexity)]
pub fn combine_handlers<E1, E2, H1, H2>(
    mut h1: H1,
    mut h2: H2,
) -> impl FnMut(Either<E1, E2>) -> Option<Either<E1::Response, E2::Response>>
where
    E1: Effect,
    E2: Effect,
    H1: FnMut(E1) -> Option<E1::Response>,
    H2: FnMut(E2) -> Option<E2::Response>,
{
    move |eff| match eff {
        Either::Left(e1) => h1(e1).map(Either::Left),
        Either::Right(e2) => h2(e2).map(Either::Right),
    }
}

/// Fluent extension trait for composing synchronous effect handlers.
pub trait HandlerExt<E: Effect>: Sized {
    /// Chains this handler with another handler, producing a composite handler over [`Either`].
    #[inline(always)]
    #[allow(clippy::type_complexity)]
    fn or_else<E2: Effect, H2>(
        self,
        other: H2,
    ) -> impl FnMut(Either<E, E2>) -> Option<Either<E::Response, E2::Response>>
    where
        Self: FnMut(E) -> Option<E::Response>,
        H2: FnMut(E2) -> Option<E2::Response>,
    {
        combine_handlers(self, other)
    }
}

impl<E: Effect, F: FnMut(E) -> Option<E::Response>> HandlerExt<E> for F {}

/// Declarative macro for matching and dispatching open effect sums.
#[macro_export]
macro_rules! match_effect {
    ($val:expr, Left($bind_l:ident) => $arm_l:expr, Right($bind_r:ident) => $arm_r:expr $(,)?) => {
        match $val {
            $crate::Either::Left($bind_l) => $arm_l,
            $crate::Either::Right($bind_r) => $arm_r,
        }
    };
}

/// Declarative macro for flat pattern matching across arbitrarily nested [`Either`] coproducts.
#[macro_export]
macro_rules! match_coproduct {
    // Base case: single arm remaining for Nil-terminated tree: match Either::Left($arm)
    ($val:expr, $arm:pat => $body:expr $(,)?) => {
        match $val {
            $crate::Either::Left($arm) => $body,
            $crate::Either::Right(_) => unreachable!(),
        }
    };
    // Recursive decomposition: Left matches first pattern, Right recurses on remaining arms
    ($val:expr, $first_pat:pat => $first_body:expr, $($tail_pat:pat => $tail_body:expr),+ $(,)?) => {
        match $val {
            $crate::Either::Left($first_pat) => $first_body,
            $crate::Either::Right(rest) => $crate::match_coproduct!(rest, $($tail_pat => $tail_body),+),
        }
    };
}

/// Ergonomic extension methods for state machines.
pub trait MachineExt: Machine {
    /// Converts a 1-way generator into a fused `core::iter::Iterator`.
    #[inline(always)]
    fn into_iter(self) -> GenIter<Self>
    where
        Self: Sized + Generator,
    {
        GenIter::new(self)
    }
}

impl<M: ?Sized + Machine> MachineExt for M {}

/// Common prelude for `machine-core`.
pub mod prelude {
    pub use crate::{
        AbortEffect, Coproduct, CoproductInject, Effect, Either, GenIter, Generator, HandlerExt,
        InjectExt, IntoLeft, IntoRight, Machine, MachineExt, Nil, ResumeSignal, Sleep, Step,
        YieldNow, combine_handlers, handle_sync, match_coproduct, match_effect,
    };
}

#[cfg(test)]
pub mod fixtures {
    use super::prelude::*;

    pub struct RangeGen {
        pub curr: u64,
        pub max: u64,
        pub polled_after_done: bool,
    }

    impl RangeGen {
        pub const fn new(start: u64, max: u64) -> Self {
            Self {
                curr: start,
                max,
                polled_after_done: false,
            }
        }
    }

    impl Machine for RangeGen {
        type Yield = u64;
        type Resume = ();
        type Return = ();

        fn step(&mut self, _resume: ()) -> Step<Self::Yield, Self::Return> {
            if self.polled_after_done {
                panic!("Contract violation: RangeGen polled after Done");
            }

            if self.curr < self.max {
                let val = self.curr;
                self.curr += 1;
                Step::Yielded(val)
            } else {
                self.polled_after_done = true;
                Step::Done(())
            }
        }
    }

    pub struct GreetEffect {
        pub prompt: &'static str,
    }
    impl Effect for GreetEffect {
        type Response = &'static str;
    }

    pub enum GreetState {
        Init,
        WaitingName,
        Done,
        Aborted,
    }

    pub struct GreetFSM<'a> {
        pub state: GreetState,
        pub close_counter: Option<&'a core::sync::atomic::AtomicUsize>,
    }

    impl Default for GreetFSM<'_> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<'a> GreetFSM<'a> {
        pub const fn new() -> Self {
            Self {
                state: GreetState::Init,
                close_counter: None,
            }
        }

        pub const fn with_close_counter(counter: &'a core::sync::atomic::AtomicUsize) -> Self {
            Self {
                state: GreetState::Init,
                close_counter: Some(counter),
            }
        }
    }

    impl Machine for GreetFSM<'_> {
        type Yield = GreetEffect;
        type Resume = ResumeSignal<&'static str>;
        type Return = &'static str;

        fn step(&mut self, signal: Self::Resume) -> Step<Self::Yield, Self::Return> {
            match self.state {
                GreetState::Init => {
                    self.state = GreetState::WaitingName;
                    Step::Yielded(GreetEffect { prompt: "Name?" })
                }
                GreetState::WaitingName => match signal {
                    ResumeSignal::Resume(name) => {
                        self.state = GreetState::Done;
                        Step::Done(name)
                    }
                    ResumeSignal::Abort => {
                        self.state = GreetState::Aborted;
                        Step::Done("aborted")
                    }
                    ResumeSignal::Start => {
                        self.state = GreetState::Aborted;
                        Step::Done("protocol_error")
                    }
                },
                GreetState::Done | GreetState::Aborted => {
                    panic!("Contract violation: GreetFSM polled after terminal state");
                }
            }
        }

        fn close(&mut self) {
            if let Some(counter) = self.close_counter {
                counter.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::prelude::*;

    #[test]
    fn test_generator_to_iterator_projection_never_double_polls() {
        let generator = RangeGen::new(1, 4);
        let mut iter = generator.into_iter();

        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(3));
        assert_eq!(iter.next(), None);
        // Trailing None calls must NOT poll the underlying generator
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_two_way_effect_dispatch_success() {
        let machine = GreetFSM::new();
        let result = handle_sync(machine, |eff| {
            assert_eq!(eff.prompt, "Name?");
            Some("Arthur")
        });
        assert_eq!(result, Some("Arthur"));
    }

    #[test]
    fn test_two_way_effect_abort_triggers_close() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        let counter = AtomicUsize::new(0);
        let machine = GreetFSM::with_close_counter(&counter);

        let result = handle_sync(machine, |_eff| None);
        assert_eq!(result, None);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
}
