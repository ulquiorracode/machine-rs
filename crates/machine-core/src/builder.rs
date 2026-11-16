//! # Tier 2: Fluent Builder API for Finite State Machines
//!
//! Provides [`MachineBuilder`] for constructing executable [`Machine`] and [`Generator`](crate::Generator)
//! instances programmatically with zero heap allocations.

use core::marker::PhantomData;

use crate::{Machine, Step};

/// An executable [`Machine`] backed by a transition closure.
pub struct FnMachine<State, Resume, Yield, Return, F> {
    state: State,
    transition: F,
    _marker: PhantomData<(Resume, Yield, Return)>,
}

impl<State, Resume, Yield, Return, F> FnMachine<State, Resume, Yield, Return, F> {
    /// Returns a reference to the internal machine state.
    #[inline(always)]
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Returns a mutable reference to the internal machine state.
    #[inline(always)]
    pub fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }
}

impl<State, Resume, Yield, Return, F> Machine for FnMachine<State, Resume, Yield, Return, F>
where
    F: FnMut(&mut State, Resume) -> Step<Yield, Return>,
{
    type Yield = Yield;
    type Resume = Resume;
    type Return = Return;

    #[inline(always)]
    fn step(&mut self, resume: Self::Resume) -> Step<Self::Yield, Self::Return> {
        (self.transition)(&mut self.state, resume)
    }
}

/// Fluent builder for constructing state machines.
#[derive(Debug, Clone, Copy, Default)]
pub struct MachineBuilder<State> {
    initial_state: State,
}

impl<State> MachineBuilder<State> {
    /// Creates a new builder seeded with the given initial state.
    #[inline(always)]
    pub const fn new(initial_state: State) -> Self {
        Self { initial_state }
    }

    /// Connects a transition function, constructing an executable [`Machine`].
    #[inline(always)]
    pub fn transition<Resume, Yield, Return, F>(
        self,
        f: F,
    ) -> FnMachine<State, Resume, Yield, Return, F>
    where
        F: FnMut(&mut State, Resume) -> Step<Yield, Return>,
    {
        FnMachine {
            state: self.initial_state,
            transition: f,
            _marker: PhantomData,
        }
    }

    /// Connects a 1-way generator function (`Resume = ()`), constructing an executable [`Generator`](crate::Generator).
    #[allow(clippy::type_complexity)]
    #[inline(always)]
    pub fn generator<Yield, Return, F>(
        self,
        mut f: F,
    ) -> FnMachine<State, (), Yield, Return, impl FnMut(&mut State, ()) -> Step<Yield, Return>>
    where
        F: FnMut(&mut State) -> Step<Yield, Return>,
    {
        self.transition(move |state, ()| f(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_machine_builder_basic() {
        let mut machine =
            MachineBuilder::new(0usize).transition(|state, event: &str| match event {
                "inc" => {
                    *state += 1;
                    Step::Yielded(*state)
                }
                "stop" => Step::Done(*state),
                _ => Step::Yielded(*state),
            });

        assert_eq!(machine.step("inc"), Step::Yielded(1));
        assert_eq!(machine.step("inc"), Step::Yielded(2));
        assert_eq!(machine.step("stop"), Step::Done(2));
    }

    #[test]
    fn test_machine_builder_generator() {
        let mut generator = MachineBuilder::new(10u32).generator(|count| {
            if *count == 0 {
                Step::Done(())
            } else {
                let current = *count;
                *count -= 1;
                Step::Yielded(current)
            }
        });

        assert_eq!(generator.step(()), Step::Yielded(10));
        assert_eq!(generator.step(()), Step::Yielded(9));
    }
}
