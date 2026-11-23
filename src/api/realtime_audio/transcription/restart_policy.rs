//! Restart decisions for the realtime transcription session thread.

use std::thread::JoinHandle;
use std::time::Duration;

const DEVICE_RECONNECT_BASE: Duration = Duration::from_millis(750);
const DEVICE_RECONNECT_ATTEMPTS: u32 = 6;
/// An engine run this long had a working device, so pacing starts over.
const HEALTHY_RUN: Duration = Duration::from_secs(60);

/// Capped exponential backoff for device-reconnect restarts. Each restart
/// reopens the engine (a Live socket or local models), so a device that keeps
/// failing must not drive an unbounded fixed-rate loop.
#[derive(Default)]
pub(super) struct DeviceReconnectBackoff {
    attempts: u32,
}

impl DeviceReconnectBackoff {
    /// Returns the delay before the next restart, or `None` once the attempt
    /// budget is spent and the device failure should be reported.
    pub fn next_delay(&mut self, ran_for: Duration) -> Option<Duration> {
        if ran_for >= HEALTHY_RUN {
            self.attempts = 0;
        }
        if self.attempts >= DEVICE_RECONNECT_ATTEMPTS {
            return None;
        }
        let delay = DEVICE_RECONNECT_BASE.saturating_mul(1 << self.attempts);
        self.attempts += 1;
        Some(delay)
    }
}

/// The text translation loop exits on the shared stop signal during a restart,
/// so it is respawned whenever it is wanted and no live loop remains.
pub(super) fn should_spawn_text_translation(
    wanted: bool,
    running: Option<&JoinHandle<()>>,
) -> bool {
    wanted && running.is_none_or(JoinHandle::is_finished)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_reconnect_backs_off_then_gives_up() {
        let mut backoff = DeviceReconnectBackoff::default();
        let delays: Vec<_> = std::iter::from_fn(|| backoff.next_delay(Duration::ZERO))
            .map(|delay| delay.as_millis())
            .collect();
        assert_eq!(delays, [750, 1_500, 3_000, 6_000, 12_000, 24_000]);
        assert_eq!(backoff.next_delay(Duration::ZERO), None);
        assert_eq!(
            backoff.next_delay(HEALTHY_RUN),
            Some(DEVICE_RECONNECT_BASE),
            "a healthy run restores the budget"
        );
    }

    #[test]
    fn exited_translation_loop_is_respawned() {
        assert!(should_spawn_text_translation(true, None));
        assert!(!should_spawn_text_translation(false, None));

        let finished = std::thread::spawn(|| {});
        while !finished.is_finished() {
            std::thread::yield_now();
        }
        assert!(should_spawn_text_translation(true, Some(&finished)));

        let (release, wait) = std::sync::mpsc::channel::<()>();
        let running = std::thread::spawn(move || {
            let _ = wait.recv();
        });
        assert!(!should_spawn_text_translation(true, Some(&running)));
        release.send(()).unwrap();
        running.join().unwrap();
    }
}
