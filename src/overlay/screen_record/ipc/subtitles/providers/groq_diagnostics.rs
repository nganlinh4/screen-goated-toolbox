#[derive(Debug, Default)]
pub(super) struct GroqTranscriptDiagnostics {
    segment_count: usize,
    minimum_avg_logprob: Option<f64>,
    maximum_no_speech_prob: Option<f64>,
    maximum_compression_ratio: Option<f64>,
}

impl GroqTranscriptDiagnostics {
    pub(super) fn from_metrics(
        segment_count: usize,
        values: impl Iterator<Item = (Option<f64>, Option<f64>, Option<f64>)>,
    ) -> Self {
        let values: Vec<_> = values.collect();
        Self {
            segment_count,
            minimum_avg_logprob: finite_min(values.iter().filter_map(|value| value.0)),
            maximum_no_speech_prob: finite_max(values.iter().filter_map(|value| value.1)),
            maximum_compression_ratio: finite_max(values.iter().filter_map(|value| value.2)),
        }
    }

    pub(super) fn should_retry(&self) -> bool {
        let likely_speech = self.maximum_no_speech_prob.unwrap_or(0.0) <= 0.6;
        likely_speech
            && (self.minimum_avg_logprob.is_some_and(|value| value < -1.0)
                || self
                    .maximum_compression_ratio
                    .is_some_and(|value| value > 2.4))
    }

    pub(super) fn log(&self, model: &str, part: usize, total: usize) {
        crate::log_info!(
            "[SubtitleGen][Groq] diagnostics part={}/{} model={} segments={} min_avg_logprob={} max_no_speech_prob={} max_compression_ratio={} retry_recommended={}",
            part,
            total,
            model,
            self.segment_count,
            display_metric(self.minimum_avg_logprob),
            display_metric(self.maximum_no_speech_prob),
            display_metric(self.maximum_compression_ratio),
            self.should_retry()
        );
    }
}

fn finite_min(values: impl Iterator<Item = f64>) -> Option<f64> {
    values.filter(|value| value.is_finite()).reduce(f64::min)
}

fn finite_max(values: impl Iterator<Item = f64>) -> Option<f64> {
    values.filter(|value| value.is_finite()).reduce(f64::max)
}

fn display_metric(value: Option<f64>) -> String {
    value
        .map(|metric| format!("{metric:.3}"))
        .unwrap_or_else(|| "n/a".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_only_quality_failures_with_likely_speech() {
        let low_confidence = GroqTranscriptDiagnostics::from_metrics(
            1,
            [(Some(-1.1), Some(0.2), Some(1.0))].into_iter(),
        );
        let silence = GroqTranscriptDiagnostics::from_metrics(
            1,
            [(Some(-1.1), Some(0.9), Some(1.0))].into_iter(),
        );
        let healthy = GroqTranscriptDiagnostics::from_metrics(
            1,
            [(Some(-0.2), Some(0.1), Some(1.2))].into_iter(),
        );
        assert!(low_confidence.should_retry());
        assert!(!silence.should_retry());
        assert!(!healthy.should_retry());
    }
}
