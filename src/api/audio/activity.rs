//! Level-relative activity evidence; never changes PCM gain or claims recognition.
use std::time::Instant;

pub(crate) const MAX_SPEECH_RMS: f32 = 0.015;
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
    fn shared_level_activity_cases() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/preset-system/microphone-activity.json"
        )))
        .unwrap();
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
