use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::api::gemini_live::setup::{LiveSetupBuilder, TranscriptionMode};
#[cfg(test)]
mod replay_tests;
mod stabilization;

pub(crate) const ROTATE_AT: Duration = Duration::from_secs(9 * 60);
#[cfg(test)]
const SPEECH_RMS: f32 = crate::api::audio::activity::MAX_SPEECH_RMS;
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

/// A dedicated transcription session follows a server `goAway` to a fresh
/// connection at once; other Live models keep theirs until it closes.
pub(crate) fn rotates_on_go_away(dedicated: bool, go_away: bool) -> bool {
    dedicated && go_away
}

/// A dedicated transcription session rotates before the server session limit
/// or after a vocabulary change, but only at a speech gap so no utterance is
/// split across connections.
pub(crate) fn proactive_rotation_due(
    dedicated: bool,
    connection_age: Duration,
    vocabulary_changed: bool,
    safe_gap: bool,
) -> bool {
    dedicated && safe_gap && (connection_age >= ROTATE_AT || vocabulary_changed)
}

/// Remember the newest resumable handle; a non-resumable update keeps the last
/// usable one so a reconnect can still continue the session.
pub(crate) fn observe_resumption(
    handle: &mut Option<String>,
    update: Option<&crate::api::gemini_live::server_frame::LiveSessionResumption>,
) {
    if let Some(update) = update
        && update.resumable
        && let Some(next) = update.handle.as_ref()
    {
        *handle = Some(next.clone());
    }
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
    delivery: stabilization::Stabilizer,
    diagnostic_id: u64,
    sequence: u64,
    finished: bool,
}

impl TranscriptState {
    /// Finals commit the bounded delivered segment, not a replacement raw result.
    /// When a frame contains both fields, the final supersedes its hypothesis.
    pub(crate) fn apply_update(
        &mut self,
        interim: Option<&str>,
        final_text: Option<&str>,
    ) -> Option<String> {
        if self.finished {
            return None;
        }
        if let Some(text) = final_text {
            Some(self.commit_final(text))
        } else {
            if let Some(text) = interim {
                self.replace_interim(text);
            }
            None
        }
    }
    pub(crate) fn replace_interim(&mut self, text: &str) {
        if self.finished {
            return;
        }
        self.delivery.update(text, false);
        self.interim.clone_from(&self.delivery.visible);
        self.log_delivery(text, false);
    }

    pub(crate) fn commit_final(&mut self, text: &str) -> String {
        if self.finished {
            return String::new();
        }
        let before = self.committed.len();
        self.delivery.update(text, true);
        self.log_delivery(text, true);
        append_segment(&mut self.committed, &self.delivery.visible);
        self.delivery.finish_segment(text);
        self.interim.clear();
        self.committed[before..].to_string()
    }

    pub(crate) fn committed(&self) -> &str {
        &self.committed
    }

    pub(crate) fn interim(&self) -> &str {
        &self.interim
    }

    /// Normal stop keeps already delivered text; cancellation does not call this.
    pub(crate) fn finish_pending(&mut self) -> String {
        if self.finished {
            return String::new();
        }
        self.finished = true;
        let before = self.committed.len();
        append_segment(&mut self.committed, &self.interim);
        self.interim.clear();
        self.delivery = Default::default();
        self.committed[before..].to_owned()
    }

    fn log_delivery(&mut self, raw: &str, final_text: bool) {
        if !crate::debug_log::diagnostics::paste_enabled() {
            return;
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        if self.diagnostic_id == 0 {
            self.diagnostic_id = NEXT.fetch_add(1, Ordering::Relaxed);
        }
        self.sequence += 1;
        static TEXT: LazyLock<bool> =
            LazyLock::new(|| std::env::var("SGT_AUTOPASTE_TEXT_DIAGNOSTICS").as_deref() == Ok("1"));
        crate::log_info!(
            "[TranscriptionDelivery] session={} sequence={} final={} raw_chars={} delivered_chars={} changed={} overlap_chars={}",
            self.diagnostic_id,
            self.sequence,
            final_text,
            raw.chars().count(),
            self.delivery.visible.chars().count(),
            raw != self.delivery.visible,
            self.delivery.overlap_chars
        );
        if *TEXT {
            crate::log_info!(
                "[TranscriptionDeliveryText] {}",
                serde_json::json!({
                    "pid": std::process::id(), "session": self.diagnostic_id, "sequence": self.sequence,
                    "final": final_text, "raw": raw.chars().take(4096).collect::<String>(),
                    "delivered": self.delivery.visible.chars().take(4096).collect::<String>(),
                    "truncated": raw.chars().count() > 4096 || self.delivery.visible.chars().count() > 4096
                })
            );
        }
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
    activity: crate::api::audio::activity::SpeechActivity,
    turn_active: bool,
    last_speech_at: Option<Instant>,
    end_sent: bool,
}

impl HybridVad {
    pub(crate) fn is_safe_gap(&self) -> bool {
        !self.turn_active
    }

    pub(crate) fn observe(&mut self, rms: f32, now: Instant) -> bool {
        if self.activity.observe(rms, now) {
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
        self.activity = Default::default();
        self.turn_active = false;
        self.last_speech_at = None;
        self.end_sent = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedicated_sessions_rotate_on_go_away_and_at_speech_gaps_only() {
        assert!(rotates_on_go_away(true, true));
        assert!(!rotates_on_go_away(true, false));
        assert!(!rotates_on_go_away(false, true));
        let young = ROTATE_AT - Duration::from_secs(1);
        assert!(!proactive_rotation_due(true, young, false, true));
        assert!(proactive_rotation_due(true, ROTATE_AT, false, true));
        assert!(proactive_rotation_due(true, young, true, true));
        assert!(!proactive_rotation_due(true, ROTATE_AT, true, false));
        assert!(!proactive_rotation_due(false, ROTATE_AT, true, true));
    }

    #[test]
    fn only_resumable_updates_replace_the_resumption_handle() {
        use crate::api::gemini_live::server_frame::LiveSessionResumption;
        let mut handle = None;
        let update = |resumable, handle: Option<&str>| LiveSessionResumption {
            resumable,
            handle: handle.map(str::to_owned),
        };
        observe_resumption(&mut handle, Some(&update(true, Some("first"))));
        observe_resumption(&mut handle, Some(&update(false, Some("ignored"))));
        observe_resumption(&mut handle, Some(&update(true, None)));
        observe_resumption(&mut handle, None);
        assert_eq!(handle.as_deref(), Some("first"));
        observe_resumption(&mut handle, Some(&update(true, Some("second"))));
        assert_eq!(handle.as_deref(), Some("second"));
    }

    #[test]
    fn shared_stabilization_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/gemini-transcribe-stream/stabilization.json"
        )))
        .unwrap();
        assert_eq!(fixture["revisionWords"], stabilization::REVISION_WORDS);
        assert_eq!(fixture["revisionScalars"], stabilization::REVISION_SCALARS);
        for case in fixture["cases"].as_array().unwrap() {
            let mut state = TranscriptState::default();
            for event in case["events"].as_array().unwrap() {
                state.apply_update(event["interim"].as_str(), event["final"].as_str());
                assert_eq!(
                    state.display(),
                    event["display"].as_str().unwrap(),
                    "{}",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn normal_stop_keeps_delivered_provisional_text_once() {
        let mut state = TranscriptState::default();
        state.replace_interim("still speaking");
        assert_eq!(state.finish_pending(), "still speaking");
        assert_eq!(state.committed(), "still speaking");
        assert!(state.finish_pending().is_empty());
    }

    #[test]
    fn continuous_typing_only_emits_authoritative_final_deltas() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/preset-system/streaming-typing.json"
        )))
        .unwrap();
        let mut transcript = TranscriptState::default();
        let mut typed = String::new();
        for event in fixture["events"].as_array().unwrap() {
            let delta = transcript.apply_update(event["interim"].as_str(), event["final"].as_str());
            assert_eq!(
                delta.as_deref().unwrap_or(""),
                event["typedDelta"].as_str().unwrap()
            );
            if let Some(delta) = delta {
                typed.push_str(&delta);
            }
            assert_eq!(transcript.display(), event["display"].as_str().unwrap());
        }
        assert_eq!(typed, fixture["finalHistory"].as_str().unwrap());
        assert_eq!(transcript.committed(), typed);
        assert_eq!(
            transcript.apply_update(None, None),
            None,
            "teardown without a new final must not repeat output"
        );
    }

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
