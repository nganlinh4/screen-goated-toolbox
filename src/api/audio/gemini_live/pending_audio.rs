//! Keeps unsent replay ahead of newly captured audio when a recording stops.

use std::ops::{Deref, DerefMut};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct PendingAudio {
    samples: Vec<i16>,
    capture_tail: Arc<Mutex<Vec<i16>>>,
    abort: Arc<AtomicBool>,
}

impl PendingAudio {
    pub(super) fn new(capture_tail: Arc<Mutex<Vec<i16>>>, abort: Arc<AtomicBool>) -> Self {
        Self {
            samples: Vec::new(),
            capture_tail,
            abort,
        }
    }
}

impl Deref for PendingAudio {
    type Target = Vec<i16>;

    fn deref(&self) -> &Self::Target {
        &self.samples
    }
}

impl DerefMut for PendingAudio {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.samples
    }
}

impl Drop for PendingAudio {
    fn drop(&mut self) {
        if self.samples.is_empty() || self.abort.load(Ordering::Relaxed) || std::thread::panicking()
        {
            return;
        }
        if let Ok(mut tail) = self.capture_tail.lock() {
            let replay_samples = self.samples.len();
            let dropped = crate::api::audio::retention::append_pending(&mut self.samples, &tail);
            if dropped > 0 {
                crate::log_info!(
                    "[AudioCapture] owner=pending-handoff pending_dropped_samples={dropped}"
                );
            }
            *tail = std::mem::take(&mut self.samples);
            crate::log_info!("[GeminiLiveStream] pending_audio_handoff samples={replay_samples}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_hands_pending_replay_to_tail_in_wire_order() {
        let tail = Arc::new(Mutex::new(vec![30, 40]));
        {
            let mut pending = PendingAudio::new(tail.clone(), Arc::new(AtomicBool::new(false)));
            pending.extend([10, 20]);
        }
        assert_eq!(*tail.lock().unwrap(), [10, 20, 30, 40]);
    }

    #[test]
    fn abort_discards_pending_replay_without_flushing() {
        let tail = Arc::new(Mutex::new(vec![30, 40]));
        let abort = Arc::new(AtomicBool::new(false));
        {
            let mut pending = PendingAudio::new(tail.clone(), abort.clone());
            pending.extend([10, 20]);
            abort.store(true, Ordering::Relaxed);
        }
        assert_eq!(*tail.lock().unwrap(), [30, 40]);
    }

    #[test]
    fn fully_sent_replay_is_not_repeated_during_stop() {
        let tail = Arc::new(Mutex::new(vec![30, 40]));
        {
            let mut pending = PendingAudio::new(tail.clone(), Arc::new(AtomicBool::new(false)));
            pending.extend([10, 20]);
            assert_eq!(std::mem::take(&mut *pending), [10, 20]);
        }
        assert_eq!(*tail.lock().unwrap(), [30, 40]);
    }

    #[test]
    fn stop_during_reconnect_preserves_older_replay_before_new_capture() {
        let tail = Arc::new(Mutex::new(vec![50]));
        {
            let mut pending = PendingAudio::new(tail.clone(), Arc::new(AtomicBool::new(false)));
            pending.extend([10, 20]);
            let mut reconnect = std::mem::take(&mut *pending);
            reconnect.extend([30, 40]);
            pending.extend(reconnect);
        }
        assert_eq!(*tail.lock().unwrap(), [10, 20, 30, 40, 50]);
    }
}
