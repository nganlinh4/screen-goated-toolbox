//! Worker warm-up badge: an estimated, never-complete percentage until ready.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

pub(super) struct WarmupProgress {
    state: Arc<AtomicU8>,
}

impl WarmupProgress {
    pub(super) fn start(title: &'static str, message: &'static str) -> Self {
        let badge =
            crate::overlay::auto_copy_badge::DownloadProgressBadge::with_text(title, message);
        badge.set_phase(message, 0.0);
        let state = Arc::new(AtomicU8::new(0));
        let worker_state = Arc::clone(&state);
        std::thread::spawn(move || {
            let started = Instant::now();
            loop {
                match worker_state.load(Ordering::Acquire) {
                    0 => badge.set_phase(message, estimated_loading_percent(started.elapsed())),
                    1 => {
                        badge.set_phase(message, 100.0);
                        std::thread::sleep(Duration::from_millis(350));
                        break;
                    }
                    _ => break,
                }
                std::thread::sleep(Duration::from_millis(80));
            }
            badge.finish();
        });
        Self { state }
    }

    pub(super) fn finish(self) {
        self.state.store(1, Ordering::Release);
    }
}

impl Drop for WarmupProgress {
    fn drop(&mut self) {
        let _ = self
            .state
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire);
    }
}

fn estimated_loading_percent(elapsed: Duration) -> f32 {
    let seconds = (elapsed.as_secs_f32() - 0.35).max(0.0);
    (99.0 * (1.0 - (-seconds / 3.0).exp())).min(99.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_starts_at_zero_and_never_claims_readiness() {
        assert_eq!(estimated_loading_percent(Duration::ZERO), 0.0);
        let mut previous = 0.0;
        for millis in (0..120_000).step_by(80) {
            let percent = estimated_loading_percent(Duration::from_millis(millis));
            assert!((previous..100.0).contains(&percent));
            previous = percent;
        }
    }

    #[test]
    fn dropping_completed_progress_preserves_its_completion_hold() {
        let state = Arc::new(AtomicU8::new(0));
        WarmupProgress {
            state: Arc::clone(&state),
        }
        .finish();
        assert_eq!(state.load(Ordering::Acquire), 1);
        state.store(0, Ordering::Release);
        drop(WarmupProgress {
            state: Arc::clone(&state),
        });
        assert_eq!(state.load(Ordering::Acquire), 2);
    }
}
