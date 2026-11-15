use machine_rs::prelude::*;
use stitch_core::middleware::Terminal;

struct MockFsm {
    count: u32,
}

impl Machine for MockFsm {
    type Yield = u32;
    type Resume = u32;
    type Return = u32;

    fn step(&mut self, resume: Self::Resume) -> Step<Self::Yield, Self::Return> {
        self.count += resume;
        if self.count >= 10 {
            Step::Done(self.count)
        } else {
            Step::Yielded(self.count)
        }
    }
}

#[test]
fn test_stitch_terminal_adapter() {
    let fsm = MockFsm { count: 0 };
    let mut terminal =
        machine_rs::MachineTerminalAdapter::new(fsm, |step, _ctx: &mut ()| -> Result<u32, ()> {
            match step {
                Step::Yielded(val) => Ok(val),
                Step::Done(final_val) => Ok(final_val),
            }
        });

    let mut ctx = ();
    let r1 = terminal.execute(&mut ctx, 3).unwrap();
    assert_eq!(r1, 3);

    let r2 = terminal.execute(&mut ctx, 4).unwrap();
    assert_eq!(r2, 7);

    let r3 = terminal.execute(&mut ctx, 5).unwrap();
    assert_eq!(r3, 12);
}
