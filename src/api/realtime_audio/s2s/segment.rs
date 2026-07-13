use super::{
    analyze_segment_samples, s2s_backlog_ms, samples_to_ms, segment_speech_confidence,
    segment_speech_like_ratio, segment_speech_ratio,
};

#[derive(Clone)]
pub(super) struct Segment {
    pub(super) id: u64,
    pub(super) samples: Vec<i16>,
    pub(super) speech_frames: usize,
    pub(super) peak_rms: f32,
    pub(super) mean_rms: f32,
    pub(super) energetic_frames: usize,
    pub(super) speech_like_frames: usize,
}

impl Segment {
    pub(super) fn new(id: u64, samples: Vec<i16>, speech_frames: usize, peak_rms: f32) -> Self {
        let metrics = analyze_segment_samples(&samples);
        Self {
            id,
            samples,
            speech_frames,
            peak_rms: peak_rms.max(metrics.peak_rms),
            mean_rms: metrics.mean_rms,
            energetic_frames: metrics.energetic_frames,
            speech_like_frames: metrics.speech_like_frames,
        }
    }
}

pub(super) fn segment_audio_ms(segment: &Segment) -> usize {
    samples_to_ms(segment.samples.len())
}

pub(super) fn segment_peak_sample(segment: &Segment) -> f32 {
    segment
        .samples
        .iter()
        .map(|sample| (*sample as f32).abs() / i16::MAX as f32)
        .fold(0.0, f32::max)
}

#[derive(Clone, Copy, Default)]
pub(super) struct SegmentSampleMetrics {
    pub(super) mean_rms: f32,
    pub(super) peak_rms: f32,
    pub(super) energetic_frames: usize,
    pub(super) speech_like_frames: usize,
}

#[derive(Clone, Copy, Default)]
pub(super) struct AdaptiveS2sVadSnapshot {
    pub(super) strictness: f32,
}

#[derive(Default)]
pub(super) struct AdaptiveS2sVadState {
    strictness: f32,
    consecutive_empty_no_input: usize,
    last_logged_bucket: i32,
}

impl AdaptiveS2sVadState {
    pub(super) fn snapshot(&self) -> AdaptiveS2sVadSnapshot {
        let backlog_pressure = (s2s_backlog_ms() as f32 / 30_000.0).clamp(0.0, 0.55);
        AdaptiveS2sVadSnapshot {
            strictness: self.strictness.max(backlog_pressure),
        }
    }

    pub(super) fn observe(&mut self, outcome: SegmentOutcome, segment: &Segment) {
        match outcome {
            SegmentOutcome::Healthy => {
                self.consecutive_empty_no_input = 0;
                self.strictness = (self.strictness - 0.10).max(0.0);
            }
            SegmentOutcome::EmptyNoInput => {
                self.consecutive_empty_no_input += 1;
                let high_energy = segment.mean_rms >= 0.025
                    || segment.peak_rms >= 0.060
                    || segment_speech_ratio(segment) >= 0.60;
                let step = if high_energy { 0.22 } else { 0.12 };
                self.strictness = (self.strictness + step).min(1.0);
            }
            SegmentOutcome::RetryFresh => {}
        }
        self.log_if_changed(outcome, segment);
    }

    fn log_if_changed(&mut self, outcome: SegmentOutcome, segment: &Segment) {
        let bucket = (self.strictness * 4.0).round() as i32;
        let should_log = bucket != self.last_logged_bucket
            || matches!(outcome, SegmentOutcome::EmptyNoInput)
                && self.consecutive_empty_no_input <= 3;
        if !should_log {
            return;
        }
        self.last_logged_bucket = bucket;
        eprintln!(
            "[RealtimeS2S][AdaptiveVAD] outcome={:?} strictness={:.2} consecutive_empty={} segment={} confidence={:.2} speech_like_ratio={:.2} speech_ratio={:.2} mean_rms={:.4} peak_rms={:.4}",
            outcome,
            self.strictness,
            self.consecutive_empty_no_input,
            segment.id,
            segment_speech_confidence(segment),
            segment_speech_like_ratio(segment),
            segment_speech_ratio(segment),
            segment.mean_rms,
            segment.peak_rms
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SegmentOutcome {
    Healthy,
    RetryFresh,
    EmptyNoInput,
}
