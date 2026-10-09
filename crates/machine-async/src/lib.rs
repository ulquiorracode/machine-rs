//! Asynchronous adapters and drivers for `machine-rs`.

use core::future::Future;
use machine_core::{Effect, Machine, Step};

/// Asynchronous effect driver executing an effectful [`Machine`] to completion.
pub async fn handle_async<M, E, H, Fut>(mut machine: M, mut handler: H) -> Option<M::Return>
where
    E: Effect,
    M: Machine<Yield = E, Resume = Option<E::Response>> + Send,
    H: FnMut(E) -> Fut + Send,
    Fut: Future<Output = Option<E::Response>> + Send,
{
    let mut resume_val = None;
    loop {
        match machine.step(resume_val.take()) {
            Step::Yielded(eff) => {
                let resp = handler(eff).await;
                if resp.is_none() {
                    machine.close();
                    return None;
                }
                resume_val = resp;
            }
            Step::Done(ret) => return Some(ret),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FetchEffect {
        pub url: &'static str,
    }
    impl Effect for FetchEffect {
        type Response = u32;
    }

    enum FetchState {
        Init,
        Waiting,
        Done,
    }

    struct FetchMachine {
        state: FetchState,
    }

    impl Machine for FetchMachine {
        type Yield = FetchEffect;
        type Resume = Option<u32>;
        type Return = u32;

        fn step(&mut self, resume: Self::Resume) -> Step<Self::Yield, Self::Return> {
            match self.state {
                FetchState::Init => {
                    self.state = FetchState::Waiting;
                    Step::Yielded(FetchEffect {
                        url: "https://api.example.com",
                    })
                }
                FetchState::Waiting => {
                    let code = resume.expect("Resume required");
                    self.state = FetchState::Done;
                    Step::Done(code)
                }
                FetchState::Done => panic!("Polled after Done"),
            }
        }
    }

    #[tokio::test]
    async fn test_async_tokio_driver() {
        let machine = FetchMachine {
            state: FetchState::Init,
        };

        let handle = tokio::spawn(async move {
            handle_async(machine, |eff| async move {
                assert_eq!(eff.url, "https://api.example.com");
                Some(200)
            })
            .await
        });

        let status = handle.await.unwrap();
        assert_eq!(status, Some(200));
    }
}
