//! Bounded optional audio attachment for continuous transcription.
const MAX_ATTACHMENT_SAMPLES: usize = 16_000 * 60 * 10;
pub(crate) const MAX_PENDING_SAMPLES: usize = 16_000 * 60;

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
}
