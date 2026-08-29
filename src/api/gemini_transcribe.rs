use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::api::gemini_live::setup::{LiveSetupBuilder, TranscriptionMode};

pub(crate) const ROTATE_AT: Duration = Duration::from_secs(9 * 60);
const SPEECH_RMS: f32 = 0.015;
const TRAILING_AUDIO: Duration = Duration::from_millis(180);
const END_SILENCE: Duration = Duration::from_millis(420);
const MAX_VOCABULARY: usize = 1_000;

static VOCABULARY: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static VOCABULARY_VERSION: AtomicU64 = AtomicU64::new(0);

pub(crate) fn is_live_transcribe(model: &str) -> bool {
    crate::model_config::live_endpoint_profile(model).and_then(|profile| profile.protocol)
        == Some("live-transcribe")
}

pub(crate) fn build_live_setup(
    model: &str,
    vocabulary: &[String],
    resumption_handle: Option<&str>,
) -> serde_json::Value {
    let builder = LiveSetupBuilder::new(model).transcription(TranscriptionMode::Input);
    if !is_live_transcribe(model) {
        return builder
            .media_resolution(crate::api::gemini_live::setup::MediaResolution::Low)
            .build();
    }
    let resumption = resumption_handle
        .map(|handle| serde_json::json!({ "handle": handle }))
        .unwrap_or_else(|| serde_json::json!({}));
    builder
        .generation_field("responseModalities", serde_json::json!(["TEXT"]))
        .setup_field(
            "inputAudioTranscription",
            serde_json::json!({
                "languageCodes": [],
                "mode": "SMART",
                "customVocabulary": vocabulary,
            }),
        )
        .setup_field("sessionResumption", resumption)
        .build()
}

pub(crate) fn compute_i16_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64 / 32768.0).powi(2)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

pub(crate) fn samples_to_ms(samples: usize) -> usize {
    samples.saturating_mul(1_000) / 16_000
}

pub(crate) fn uses_periodic_silence_cycle(uses_interim_transcripts: bool) -> bool {
    !uses_interim_transcripts
}

pub(crate) fn vocabulary_snapshot() -> (u64, Vec<String>) {
    (
        VOCABULARY_VERSION.load(Ordering::SeqCst),
        VOCABULARY.lock().unwrap().clone(),
    )
}

pub(crate) fn set_vocabulary(lines: &str) {
    let mut normalized = Vec::new();
    for value in lines
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !normalized.iter().any(|known| known == value) {
            normalized.push(value.to_string());
        }
        if normalized.len() == MAX_VOCABULARY {
            break;
        }
    }
    let mut current = VOCABULARY.lock().unwrap();
    if *current != normalized {
        *current = normalized;
        VOCABULARY_VERSION.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub(crate) struct TranscriptState {
    committed: String,
    interim: String,
}

impl TranscriptState {
    pub(crate) fn replace_interim(&mut self, text: &str) {
        self.interim = text.to_string();
    }

    pub(crate) fn commit_final(&mut self, text: &str) -> String {
        let before = self.committed.len();
        append_segment(&mut self.committed, text);
        self.interim.clear();
        self.committed[before..].to_string()
    }

    pub(crate) fn committed(&self) -> &str {
        &self.committed
    }

    pub(crate) fn display(&self) -> String {
        let mut display = self.committed.clone();
        append_segment(&mut display, &self.interim);
        display
    }
}

fn append_segment(target: &mut String, text: &str) {
    if text.is_empty() {
        return;
    }
    if target.is_empty() {
        target.push_str(text.trim_start());
    } else if !target.chars().last().is_some_and(char::is_whitespace)
        && !text.chars().next().is_some_and(char::is_whitespace)
    {
        target.push(' ');
        target.push_str(text);
    } else {
        target.push_str(text);
    }
}

#[derive(Default)]
pub(crate) struct HybridVad {
    turn_active: bool,
    last_speech_at: Option<Instant>,
    end_sent: bool,
}

impl HybridVad {
    pub(crate) fn is_safe_gap(&self) -> bool {
        !self.turn_active
    }

    pub(crate) fn observe(&mut self, rms: f32, now: Instant) -> bool {
        if rms >= SPEECH_RMS {
            self.turn_active = true;
            self.last_speech_at = Some(now);
            self.end_sent = false;
            return false;
        }
        self.poll_end(now)
    }

    pub(crate) fn poll_end(&mut self, now: Instant) -> bool {
        if !self.turn_active || self.end_sent {
            return false;
        }
        let silent_for = self.last_speech_at.map(|at| now.duration_since(at));
        if silent_for.is_some_and(|duration| duration <= TRAILING_AUDIO) {
            return false;
        }
        if silent_for.is_some_and(|duration| duration >= END_SILENCE) {
            self.turn_active = false;
            self.last_speech_at = None;
            self.end_sent = true;
            return true;
        }
        false
    }

    pub(crate) fn reset_connection(&mut self) {
        self.turn_active = false;
        self.last_speech_at = None;
        self.end_sent = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_interim_is_replaced_and_final_is_appended() {
        let mut state = TranscriptState::default();
        state.replace_interim("hello wor");
        state.replace_interim("hello world.");
        assert_eq!(state.display(), "hello world.");
        assert_eq!(state.commit_final("hello world."), "hello world.");
        state.replace_interim("next");
        assert_eq!(state.display(), "hello world. next");
    }

    #[test]
    fn lifecycle_constants_match_parity_fixture() {
        let value: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/gemini-transcribe-lifecycle/contract.json"
        )))
        .unwrap();
        assert_eq!(value["hybridVad"]["speechRms"], SPEECH_RMS);
        assert_eq!(
            value["hybridVad"]["trailingAudioMs"],
            TRAILING_AUDIO.as_millis() as u64
        );
        assert_eq!(
            value["hybridVad"]["endSilenceMs"],
            END_SILENCE.as_millis() as u64
        );
        assert_eq!(value["session"]["rotateAtMs"], ROTATE_AT.as_millis() as u64);
        assert_eq!(value["customVocabulary"]["maxEntries"], MAX_VOCABULARY);
    }
}
