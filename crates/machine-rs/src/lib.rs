//! # Machine RS (`machine-rs`)
//!
//! Universal State Machine Kernel for Rust:
//! Unifying Generators, Iterators, and Algebraic Effects under a single Zero-Cost abstraction.

#![cfg_attr(not(feature = "std"), no_std)]

pub use machine_core::*;

#[cfg(feature = "async")]
pub use machine_async::*;

pub mod prelude {
    #[cfg(feature = "async")]
    pub use machine_async::handle_async;
    pub use machine_core::prelude::*;
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    struct Fibonacci {
        a: u64,
        b: u64,
        count: usize,
        max: usize,
    }

    impl Fibonacci {
        pub const fn new(max: usize) -> Self {
            Self {
                a: 0,
                b: 1,
                count: 0,
                max,
            }
        }
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

    impl FusedMachine for Fibonacci {}

    #[test]
    fn test_fibonacci_into_iter() {
        let fib = Fibonacci::new(5);
        let collected: std::vec::Vec<u64> = fib.into_iter().collect();
        assert_eq!(collected, [0, 1, 1, 2, 3]);
    }
}
