//! Level-relative activity evidence; never changes PCM gain or claims recognition.
use std::time::Instant;

pub(crate) const MAX_SPEECH_RMS: f32 = 0.015;

/// Recording termination deliberately uses the original fixed sensitivity.
/// Permissive live activity evidence must not keep resetting the silence timer.
pub(crate) fn auto_stop_activity(rms: f32) -> bool {
    rms.is_finite() && rms > MAX_SPEECH_RMS
}
pub(crate) struct SpeechActivity {
    noise: f32,
    last: Option<Instant>,
}
impl Default for SpeechActivity {
    fn default() -> Self {
        Self {
            noise: 0.0003,
            last: None,
        }
    }
}
impl SpeechActivity {
    pub(crate) fn observe(&mut self, rms: f32, now: Instant) -> bool {
        let elapsed = self
            .last
            .map(|last| now.saturating_duration_since(last).as_secs_f32())
            .unwrap_or(0.01)
            .min(0.2);
        self.last = Some(now);
        if !rms.is_finite() || rms < 0.0 {
            return false;
        }
        let threshold = (self.noise * 3.0 + 0.0002).clamp(0.001, MAX_SPEECH_RMS);
        let active = rms >= threshold;
        if !active {
            let alpha = 1.0 - (-elapsed / 0.5).exp();
            self.noise += (rms - self.noise) * alpha;
        }
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_stop_sensitivity_does_not_learn_background_as_speech() {
        for _ in 0..500 {
            assert!(!auto_stop_activity(0.003));
        }
        for _ in 0..500 {
            assert!(auto_stop_activity(0.04));
        }
        for _ in 0..500 {
            assert!(!auto_stop_activity(0.003));
        }
    }

    #[test]
    fn shared_level_activity_cases() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/preset-system/microphone-activity.json"
        )))
        .unwrap();
        for case in fixture["autoStopCases"].as_array().unwrap() {
            assert_eq!(
                auto_stop_activity(case["rms"].as_f64().unwrap() as f32),
                case["active"].as_bool().unwrap()
            );
        }
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!auto_stop_activity(invalid));
        }
        for case in fixture["cases"].as_array().unwrap() {
            let mut state = SpeechActivity::default();
            let start = Instant::now();
            for frame in case["frames"].as_array().unwrap() {
                assert_eq!(
                    state.observe(
                        frame["rms"].as_f64().unwrap() as f32,
                        start + std::time::Duration::from_millis(frame["ms"].as_u64().unwrap())
                    ),
                    frame["active"].as_bool().unwrap(),
                    "{}",
                    case["name"]
                );
            }
        }
    }
}
