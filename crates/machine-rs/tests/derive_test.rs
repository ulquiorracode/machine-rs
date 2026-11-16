use machine_rs::prelude::*;

#[state_machine(Yield = u32, Resume = bool, Return = &'static str)]
enum CounterFsm {
    Initial(u32),
    Active(u32),
}

impl CounterFsm {
    fn transition(&mut self, resume: bool) -> Step<u32, &'static str> {
        match *self {
            Self::Initial(val) => {
                *self = Self::Active(val + 1);
                Step::Yielded(val)
            }
            Self::Active(val) => {
                if resume && val < 3 {
                    *self = Self::Active(val + 1);
                    Step::Yielded(val)
                } else {
                    *self = Self::Terminal;
                    Step::Done("finished")
                }
            }
            Self::Terminal => panic!("cannot step terminal"),
        }
    }
}

#[test]
fn test_state_machine_macro_enum_transitions() {
    let mut fsm = CounterFsm::Initial(10);

    // Step 1: Initial -> Active(11), yields 10
    match fsm.step(false) {
        Step::Yielded(v) => assert_eq!(v, 10),
        Step::Done(_) => panic!("unexpected done"),
    }

    // Step 2: Active(11) -> Terminal, yields done
    match fsm.step(false) {
        Step::Done(res) => assert_eq!(res, "finished"),
        Step::Yielded(_) => panic!("unexpected yield"),
    }
}

struct StatefulMachine {
    count: u32,
    max: u32,
}

#[state_machine(Yield = u32, Resume = (), Return = u32)]
impl StatefulMachine {
    fn step(&mut self, _resume: ()) -> Step<u32, u32> {
        if self.count < self.max {
            let current = self.count;
            self.count += 1;
            Step::Yielded(current)
        } else {
            Step::Done(self.count)
        }
    }
}

#[test]
fn test_state_machine_macro_impl_block() {
    let mut machine = StatefulMachine { count: 0, max: 3 };

    assert_eq!(machine.step(()), Step::Yielded(0));
    assert_eq!(machine.step(()), Step::Yielded(1));
    assert_eq!(machine.step(()), Step::Yielded(2));
    assert_eq!(machine.step(()), Step::Done(3));
}
