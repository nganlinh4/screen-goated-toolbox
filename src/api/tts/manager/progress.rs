use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// Position of one queued PCM request on the shared output timeline.
pub(crate) struct PlaybackProgress {
    played_frames: Arc<AtomicU64>,
    baseline: AtomicU64,
    final_frames: AtomicU64,
    finished: AtomicBool,
    cancelled: Arc<AtomicBool>,
}

impl PlaybackProgress {
    pub(crate) fn new(played_frames: Arc<AtomicU64>, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            played_frames,
            baseline: AtomicU64::new(u64::MAX),
            final_frames: AtomicU64::new(u64::MAX),
            finished: AtomicBool::new(false),
            cancelled,
        }
    }

    pub(crate) fn start(&self) {
        self.baseline.store(
            self.played_frames.load(Ordering::Acquire),
            Ordering::Release,
        );
    }

    pub(crate) fn finish(&self) {
        self.final_frames
            .store(self.played_frames(), Ordering::Release);
        self.finished.store(true, Ordering::Release);
    }

    pub(crate) fn played_frames(&self) -> u64 {
        let frozen = self.final_frames.load(Ordering::Acquire);
        if frozen != u64::MAX {
            return frozen;
        }
        let baseline = self.baseline.load(Ordering::Acquire);
        if baseline == u64::MAX {
            0
        } else {
            self.played_frames
                .load(Ordering::Acquire)
                .saturating_sub(baseline)
        }
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire) || self.cancelled.load(Ordering::SeqCst)
    }

    pub(crate) fn completed(&self) -> bool {
        self.finished.load(Ordering::Acquire) && !self.cancelled.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_clip_clock_starts_at_device_consumption_not_enqueue() {
        let played = Arc::new(AtomicU64::new(100));
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = PlaybackProgress::new(played.clone(), cancel);
        played.store(1_000, Ordering::Release);
        assert_eq!(progress.played_frames(), 0);
        assert!(!progress.is_finished());
        progress.start();
        assert_eq!(progress.played_frames(), 0);
        played.store(1_480, Ordering::Release);
        assert_eq!(progress.played_frames(), 480);
        progress.finish();
        assert!(progress.is_finished());
        assert!(progress.completed());
    }

    #[test]
    fn cancelling_queued_clip_does_not_claim_completion() {
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = PlaybackProgress::new(Arc::new(AtomicU64::new(0)), cancel.clone());
        cancel.store(true, Ordering::SeqCst);
        assert!(progress.is_finished());
        assert!(!progress.completed());
    }

    #[test]
    fn finished_or_cancelled_owner_cannot_accumulate_next_owners_frames() {
        let played = Arc::new(AtomicU64::new(1_000));
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = PlaybackProgress::new(played.clone(), cancel.clone());
        progress.start();
        played.store(1_480, Ordering::Release);
        cancel.store(true, Ordering::SeqCst);
        progress.finish();
        played.store(9_000, Ordering::Release);
        assert_eq!(progress.played_frames(), 480);
        assert!(!progress.completed());
    }
}
