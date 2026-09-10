/// A lack of recognition is only a response timeout after input was flushed.
#[derive(Default)]
pub(crate) struct TranscriptionRecovery {
    active_ms: usize,
    boundary_ms: Option<u64>,
}

impl TranscriptionRecovery {
    pub(crate) fn audio_sent(&mut self, active_ms: usize) {
        self.active_ms = self.active_ms.saturating_add(active_ms);
    }

    pub(crate) fn input_flushed(&mut self, now_ms: u64) {
        if self.active_ms >= 4_000 && self.boundary_ms.is_none() {
            self.boundary_ms = Some(now_ms);
        }
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn should_reconnect(&self, now_ms: u64) -> bool {
        self.boundary_ms
            .is_some_and(|boundary| now_ms.saturating_sub(boundary) >= 8_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_shared_transcription_recovery_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/gemini-live-session/transcription-recovery.json"
        )))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let mut recovery = TranscriptionRecovery::default();
            for event in case["events"].as_array().unwrap() {
                if let Some(ms) = event["audioMs"].as_u64() {
                    recovery.audio_sent(ms as usize);
                }
                if let Some(ms) = event["boundaryMs"].as_u64() {
                    recovery.input_flushed(ms);
                }
                if event["reset"] == true || event["progress"] == true {
                    recovery.reset();
                }
                if let Some(ms) = event["tickMs"].as_u64() {
                    assert_eq!(
                        recovery.should_reconnect(ms),
                        event["expectReconnect"].as_bool().unwrap(),
                        "{} at {ms}",
                        case["name"]
                    );
                }
            }
        }
    }
}
