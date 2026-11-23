use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

const POLL_SLICE: Duration = Duration::from_millis(100);
const MAX_WAIT: Duration = Duration::from_secs(30);

pub(crate) fn wait_for_callback<T>(
    device: &wgpu::Device,
    receiver: &Receiver<T>,
    cancel: &AtomicBool,
    health: impl Fn() -> Result<(), String>,
) -> Result<T, String> {
    wait_with(receiver, cancel, MAX_WAIT, health, || {
        match device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(POLL_SLICE),
        }) {
            Ok(_) | Err(wgpu::PollError::Timeout) => Ok(()),
            Err(error) => Err(format!("GPU completion poll failed: {error}")),
        }
    })
}

fn wait_with<T>(
    receiver: &Receiver<T>,
    cancel: &AtomicBool,
    timeout: Duration,
    health: impl Fn() -> Result<(), String>,
    mut poll: impl FnMut() -> Result<(), String>,
) -> Result<T, String> {
    let start = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("GPU operation was cancelled before completion".to_string());
        }
        health()?;
        match receiver.try_recv() {
            Ok(value) => return Ok(value),
            Err(TryRecvError::Disconnected) => {
                return Err("GPU completion callback disconnected".to_string());
            }
            Err(TryRecvError::Empty) => {}
        }
        if start.elapsed() >= timeout {
            return Err("Timed out waiting for GPU completion".to_string());
        }
        poll()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_and_errors_do_not_claim_completion() {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = AtomicBool::new(true);
        assert!(
            wait_with(
                &rx,
                &cancel,
                MAX_WAIT,
                || Ok(()),
                || panic!("must not poll after cancellation")
            )
            .is_err()
        );
        cancel.store(false, Ordering::Release);
        assert!(
            wait_with(
                &rx,
                &cancel,
                MAX_WAIT,
                || Ok(()),
                || Err("device failure".into())
            )
            .is_err()
        );
        assert!(
            wait_with(
                &rx,
                &cancel,
                Duration::ZERO,
                || Ok(()),
                || panic!("must not poll after deadline")
            )
            .is_err()
        );
        tx.send(7).unwrap();
        assert_eq!(
            wait_with(&rx, &cancel, MAX_WAIT, || Ok(()), || unreachable!()).unwrap(),
            7
        );
    }
}
