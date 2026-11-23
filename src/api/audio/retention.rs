//! Bounded optional audio attachment for continuous transcription.
pub(crate) const MAX_RECORDING_SECONDS: usize = 60 * 10;
const MAX_ATTACHMENT_SAMPLES: usize = 16_000 * MAX_RECORDING_SECONDS;
pub(crate) const MAX_PENDING_SAMPLES: usize = 16_000 * 60;

pub(crate) struct CaptureBudget {
    claimed: std::sync::atomic::AtomicUsize,
    limit: usize,
}

impl CaptureBudget {
    pub(crate) fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            claimed: std::sync::atomic::AtomicUsize::new(0),
            limit: (sample_rate as usize)
                .saturating_mul(channels as usize)
                .saturating_mul(MAX_RECORDING_SECONDS),
        }
    }

    pub(crate) fn claim(&self, samples: usize) -> bool {
        use std::sync::atomic::Ordering;
        self.claimed
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current
                    .checked_add(samples)
                    .filter(|&next| next <= self.limit)
            })
            .is_ok()
    }
}

#[derive(Default)]
pub struct AudioAttachment {
    samples: Vec<i16>,
    exceeded: bool,
}
impl AudioAttachment {
    pub(crate) fn extend(&mut self, pcm: &[i16]) -> bool {
        if self.exceeded {
            return false;
        }
        if pcm.len() > MAX_ATTACHMENT_SAMPLES.saturating_sub(self.samples.len()) {
            self.samples = Vec::new();
            self.exceeded = true;
            return true;
        }
        self.samples.extend_from_slice(pcm);
        false
    }
    pub(crate) fn wav(&self) -> Vec<u8> {
        if self.exceeded {
            Vec::new()
        } else {
            super::encode_wav(&self.samples, 16000, 1)
        }
    }
}

pub(crate) fn append_pending(buffer: &mut Vec<i16>, pcm: &[i16]) -> usize {
    let dropped = buffer
        .len()
        .saturating_add(pcm.len())
        .saturating_sub(MAX_PENDING_SAMPLES);
    if pcm.len() >= MAX_PENDING_SAMPLES {
        buffer.clear();
        buffer.extend_from_slice(&pcm[pcm.len() - MAX_PENDING_SAMPLES..]);
    } else {
        let remove = dropped.min(buffer.len());
        buffer.drain(..remove);
        buffer.extend_from_slice(pcm);
    }
    dropped
}

const DROP_REPORT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Coalesces pending-buffer overflow so a stalled consumer produces one log
/// line per interval instead of one per audio callback.
#[derive(Default)]
pub(crate) struct DropReport {
    last_report: Option<std::time::Instant>,
    unreported: usize,
}

impl DropReport {
    pub(crate) fn note(&mut self, owner: &str, dropped: usize) {
        if let Some(total) = self.note_at(dropped, std::time::Instant::now()) {
            crate::log_info!("[AudioCapture] owner={owner} pending_dropped_samples={total}");
        }
    }

    fn note_at(&mut self, dropped: usize, now: std::time::Instant) -> Option<usize> {
        self.unreported = self.unreported.saturating_add(dropped);
        if self.unreported == 0
            || self
                .last_report
                .is_some_and(|last| now.duration_since(last) < DROP_REPORT_INTERVAL)
        {
            return None;
        }
        self.last_report = Some(now);
        Some(std::mem::take(&mut self.unreported))
    }
}

pub(crate) fn retain_pending(buffer: &mut Vec<i16>, pcm: &[i16]) {
    let dropped = append_pending(buffer, pcm);
    if dropped > 0 {
        crate::log_info!("[AudioCapture] owner=stream-replay pending_dropped_samples={dropped}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_day_attachment_is_bounded_and_never_misrepresents_partial_audio() {
        let mut attachment = AudioAttachment::default();
        let minute = vec![1; 16000 * 60];
        let mut transitions = 0;
        for _ in 0..24 * 60 {
            transitions += usize::from(attachment.extend(&minute));
        }
        assert_eq!(transitions, 1);
        assert!(attachment.samples.is_empty() && attachment.wav().is_empty());
    }
    #[test]
    fn pending_audio_retains_newest_samples_with_exact_drop_count() {
        let mut pending = vec![1; MAX_PENDING_SAMPLES - 2];
        assert_eq!(append_pending(&mut pending, &[2, 3, 4]), 1);
        assert_eq!(pending.len(), MAX_PENDING_SAMPLES);
        assert_eq!(&pending[pending.len() - 3..], &[2, 3, 4]);
    }

    #[test]
    fn a_stalled_consumer_logs_drops_once_per_interval_without_losing_counts() {
        let start = std::time::Instant::now();
        let mut report = DropReport::default();
        assert_eq!(report.note_at(0, start), None);
        assert_eq!(report.note_at(160, start), Some(160));
        let reports = (1..=100)
            .filter_map(|tick| {
                report.note_at(160, start + std::time::Duration::from_millis(10 * tick))
            })
            .count();
        assert_eq!(reports, 0);
        assert_eq!(
            report.note_at(160, start + DROP_REPORT_INTERVAL),
            Some(101 * 160)
        );
        assert_eq!(report.note_at(0, start + DROP_REPORT_INTERVAL * 3), None);
    }

    #[test]
    fn recording_budget_counts_channels_and_rejects_the_entire_excess_chunk() {
        let budget = CaptureBudget::new(2, 2);
        assert!(budget.claim(2 * 2 * MAX_RECORDING_SECONDS - 1));
        assert!(!budget.claim(2));
        assert!(budget.claim(1));
        assert!(!budget.claim(1));
    }
}
