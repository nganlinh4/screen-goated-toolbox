use std::collections::{BTreeMap, VecDeque};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use tungstenite::Message;
use windows::Win32::Foundation::HWND;

use crate::APP;
use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::AudioEvent;
use crate::config::Preset;
use crate::overlay::realtime_webview::{
    AUDIO_SOURCE_CHANGE, LANGUAGE_CHANGE, SELECTED_APP_PID, TRANSCRIPTION_MODEL_CHANGE,
};

use super::capture::{start_mic_capture_resilient, start_per_app_capture};
use super::state::SharedRealtimeState;
use super::utils::{update_overlay_text, update_translation_text};
use super::websocket::{
    connect_websocket, is_recoverable_socket_error, is_transient_socket_read_error,
    send_audio_chunk, send_audio_stream_end, set_socket_nonblocking, set_socket_short_timeout,
};
use super::{REALTIME_RMS, WM_VOLUME_UPDATE};

const SESSION_COUNT: usize = 3;
const FRAME_SAMPLES: usize = 1600;
const PREROLL_SAMPLES: usize = 4000;
const MIN_SEGMENT_SAMPLES: usize = 16_000;
const TARGET_SEGMENT_SAMPLES: usize = 48_000;
const MAX_SEGMENT_SAMPLES: usize = 80_000;
const END_SILENCE_FRAMES: usize = 3;
const AUDIO_IDLE_FINISH_MS: u128 = 1_200;
const FIRST_AUDIO_SILENT_RETRY_MS: u128 = 3_800;
const FIRST_AUDIO_ACTIVE_RETRY_MS: u128 = 5_200;
const S2S_HEDGE_TIMEOUT_MS: u128 = 45_000;
const S2S_HEDGE_FINAL_TIMEOUT_MS: u128 = 60_000;
const S2S_ORDERED_PENDING_SKIP_MS: u128 = 8_000;
const S2S_ORDERED_TRANSCRIPT_PENDING_SKIP_MS: u128 = 28_000;
const CONTEXT_SEGMENT_LIMIT: usize = 5;
const CONTEXT_LINE_CHAR_LIMIT: usize = 240;
const CONTEXT_TOTAL_CHAR_LIMIT: usize = 1_500;
const SPEECH_THRESHOLD_MULTIPLIER: f32 = 2.2;
const MIN_SPEECH_THRESHOLD: f32 = 0.012;
const MAX_SPEECH_THRESHOLD: f32 = 0.035;
const ABSOLUTE_SPEECH_RMS: f32 = 0.045;
const NOISE_LEARN_MAX_RMS: f32 = 0.018;
const NOISE_LEARN_THRESHOLD_RATIO: f32 = 0.60;
const MIN_SEGMENT_SPEECH_FRAMES: usize = 4;
const MIN_SEGMENT_PEAK_RMS: f32 = 0.025;
const MIN_SEGMENT_SPEECH_RATIO: f32 = 0.08;
const MIN_SPEECH_LIKE_RATIO: f32 = 0.18;
const STRICT_MIN_SPEECH_LIKE_RATIO: f32 = 0.32;
const STRICT_MIN_SPEECH_CONFIDENCE: f32 = 0.38;
const MIN_TEXT_OVERLAP_CHARS: usize = 3;
const S2S_BATCH_SEGMENT_ATTEMPTS: usize = 4;

fn grouped_first_audio_timeout_ms(source_audio_ms: u128, text_updates: usize) -> u128 {
    let base = if text_updates == 0 {
        FIRST_AUDIO_SILENT_RETRY_MS
    } else {
        FIRST_AUDIO_ACTIVE_RETRY_MS
    };
    (base + source_audio_ms.saturating_mul(2)).clamp(5_500, 30_000)
}

fn grouped_hard_timeout_ms(source_audio_ms: u128, final_attempt: bool) -> u128 {
    let base = if final_attempt {
        S2S_HEDGE_FINAL_TIMEOUT_MS
    } else {
        S2S_HEDGE_TIMEOUT_MS
    };
    (base + source_audio_ms.saturating_mul(4)).min(180_000)
}

static S2S_PLAYBACK_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct S2sContextEntry {
    id: u64,
    source: String,
    target: String,
}

#[derive(Default)]
struct S2sContextMemory {
    completed: VecDeque<S2sContextEntry>,
}

#[derive(Clone)]
struct S2sContextSnapshot {
    text: String,
}

impl S2sContextMemory {
    fn push_completed(&mut self, id: u64, source: &str, target: &str) {
        let source = truncate_chars(source.trim(), CONTEXT_LINE_CHAR_LIMIT);
        let target = truncate_chars(target.trim(), CONTEXT_LINE_CHAR_LIMIT);
        if source.is_empty() && target.is_empty() {
            return;
        }
        if self.completed.iter().any(|entry| entry.id == id) {
            return;
        }
        self.completed
            .push_back(S2sContextEntry { id, source, target });
        while self.completed.len() > CONTEXT_SEGMENT_LIMIT {
            self.completed.pop_front();
        }
    }

    fn snapshot(&self) -> S2sContextSnapshot {
        let entries = self
            .completed
            .iter()
            .rev()
            .take(CONTEXT_SEGMENT_LIMIT)
            .cloned()
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return S2sContextSnapshot {
                text: String::new(),
            };
        }

        let mut ordered = entries;
        ordered.reverse();
        let mut text = String::from(
            "\n\nPrevious context for continuity only. Do not translate or speak this again.\n",
        );
        text.push_str("Use it only for pronouns, names, terminology, and topic continuity.\n");
        for (index, entry) in ordered.iter().enumerate() {
            let number = index + 1;
            if !entry.source.is_empty() {
                text.push_str(&format!("Previous source {number}: {}\n", entry.source));
            }
            if !entry.target.is_empty() {
                text.push_str(&format!(
                    "Previous translation {number}: {}\n",
                    entry.target
                ));
            }
        }
        text.push_str("Now translate only the new incoming audio segment.");
        if text.chars().count() > CONTEXT_TOTAL_CHAR_LIMIT {
            text = truncate_chars(&text, CONTEXT_TOTAL_CHAR_LIMIT);
        }
        S2sContextSnapshot { text }
    }
}

#[derive(Clone)]
struct Segment {
    id: u64,
    samples: Vec<i16>,
    speech_frames: usize,
    peak_rms: f32,
    mean_rms: f32,
    energetic_frames: usize,
    speech_like_frames: usize,
}

impl Segment {
    fn new(id: u64, samples: Vec<i16>, speech_frames: usize, peak_rms: f32) -> Self {
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

fn segment_audio_ms(segment: &Segment) -> usize {
    samples_to_ms(segment.samples.len())
}

fn segment_peak_sample(segment: &Segment) -> f32 {
    segment
        .samples
        .iter()
        .map(|sample| (*sample as f32).abs() / i16::MAX as f32)
        .fold(0.0, f32::max)
}

#[derive(Clone, Copy, Default)]
struct SegmentSampleMetrics {
    mean_rms: f32,
    peak_rms: f32,
    energetic_frames: usize,
    speech_like_frames: usize,
}

#[derive(Clone, Copy, Default)]
struct AdaptiveS2sVadSnapshot {
    strictness: f32,
}

#[derive(Default)]
struct AdaptiveS2sVadState {
    strictness: f32,
    consecutive_empty_no_input: usize,
    last_logged_bucket: i32,
}

impl AdaptiveS2sVadState {
    fn snapshot(&self) -> AdaptiveS2sVadSnapshot {
        let backlog_pressure = (s2s_backlog_ms() as f32 / 30_000.0).clamp(0.0, 0.55);
        AdaptiveS2sVadSnapshot {
            strictness: self.strictness.max(backlog_pressure),
        }
    }

    fn observe(&mut self, outcome: SegmentOutcome, segment: &Segment) {
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

#[derive(Clone, Debug)]
pub struct S2sBatchSettings {
    pub model: String,
    pub voice: String,
    pub speed: String,
    pub target_language: String,
    pub custom_instruction: String,
    pub parallel_requests: usize,
    pub vad_group_budget: usize,
}

#[derive(Clone, Debug)]
pub struct S2sBatchSegment {
    pub id: u64,
    pub source_start_sec: f64,
    pub source_end_sec: f64,
    pub source_text: String,
    pub target_text: String,
    pub audio_pcm_24k: Vec<i16>,
}

#[derive(Clone)]
struct TimedSegment {
    segment: Segment,
    start_sample: usize,
    end_sample: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SegmentOutcome {
    Healthy,
    RetryFresh,
    EmptyNoInput,
}

#[derive(Clone)]
enum S2sEvent {
    Queued {
        id: u64,
        audio_ms: usize,
        queued_at: Instant,
    },
    InputText {
        id: u64,
        text: String,
    },
    OutputText {
        id: u64,
        text: String,
    },
    Audio {
        id: u64,
        bytes: Vec<u8>,
    },
    Done {
        id: u64,
    },
    Error {
        id: u64,
        message: String,
    },
    LiveText {
        source_full: String,
        source_committed_len: usize,
        target_committed: String,
        target_draft: String,
    },
    Interrupt,
}

enum S2sRaceEvent {
    Event {
        attempt: usize,
        event: S2sEvent,
    },
    Finished {
        attempt: usize,
        outcome: SegmentOutcome,
    },
    Error {
        attempt: usize,
        message: String,
    },
}

fn s2s_event_counts(events: &[S2sEvent]) -> (usize, usize, usize) {
    let mut input_text = 0usize;
    let mut output_text = 0usize;
    let mut audio = 0usize;
    for event in events {
        match event {
            S2sEvent::InputText { .. } => input_text += 1,
            S2sEvent::OutputText { .. } => output_text += 1,
            S2sEvent::Audio { .. } => audio += 1,
            _ => {}
        }
    }
    (input_text, output_text, audio)
}

fn s2s_attempt_counts(events: &[Vec<S2sEvent>]) -> (usize, usize, usize) {
    events
        .iter()
        .map(|buffered| s2s_event_counts(buffered))
        .fold((0usize, 0usize, 0usize), |acc, counts| {
            (acc.0 + counts.0, acc.1 + counts.1, acc.2 + counts.2)
        })
}

fn format_s2s_attempt_counts(events: &[Vec<S2sEvent>]) -> String {
    events
        .iter()
        .enumerate()
        .map(|(attempt, buffered)| {
            let (input_text, output_text, audio) = s2s_event_counts(buffered);
            format!("{attempt}:in={input_text},out={output_text},audio={audio}")
        })
        .collect::<Vec<_>>()
        .join(";")
}

mod live;
pub use live::run_gemini_live_s2s;

#[derive(Clone)]
struct S2sSettings {
    api_key: String,
    model: String,
    mode: S2sMode,
    voice: String,
    speed: String,
    custom_instruction: String,
    target_language: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum S2sMode {
    LegacyInterpreter,
    LiveTranslate,
}

impl S2sMode {
    fn log_tag(self) -> &'static str {
        match self {
            Self::LegacyInterpreter => "RealtimeS2S",
            Self::LiveTranslate => "RealtimeLiveTranslate",
        }
    }
}

#[derive(Clone)]
struct S2sSessionResources {
    event_tx: mpsc::Sender<S2sEvent>,
    stop_signal: Arc<AtomicBool>,
    settings: S2sSettings,
    context_memory: Arc<Mutex<S2sContextMemory>>,
    adaptive_vad: Arc<Mutex<AdaptiveS2sVadState>>,
}

struct HedgedSegmentRequest {
    session_index: usize,
    generation: u64,
    segment: Segment,
    context: S2sContextSnapshot,
    final_attempt: bool,
}

struct HedgedAttemptRequest {
    session_index: usize,
    attempt: usize,
    generation: u64,
    segment: Segment,
    context: S2sContextSnapshot,
    final_attempt: bool,
}

struct HedgedAttemptResources {
    settings: S2sSettings,
    stop_signal: Arc<AtomicBool>,
    cancel_signal: Arc<AtomicBool>,
    race_tx: mpsc::Sender<S2sRaceEvent>,
}

struct ProcessSegmentParams<'a> {
    mode: S2sMode,
    session_index: usize,
    generation: u64,
    event_tx: &'a mpsc::Sender<S2sEvent>,
    stop_signal: &'a Arc<AtomicBool>,
    cancel_signal: Option<&'a Arc<AtomicBool>>,
    final_attempt: bool,
}

fn load_settings() -> Result<S2sSettings> {
    let app = APP.lock().unwrap();
    let api_key = app.config.gemini_api_key.trim().to_string();
    if api_key.is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:google"));
    }
    let model = app.config.tts_gemini_live_model.trim();
    let transcription_model = crate::model_config::normalize_realtime_transcription_model_id(
        &app.config.realtime_transcription_model,
    );
    let voice = app.config.tts_voice.trim();
    let speed = app.config.tts_speed.trim();
    let target_language = app.config.realtime_target_language.clone();
    let custom_instruction =
        tts_instruction_for_target(&target_language, &app.config.tts_language_conditions);
    let mode = if crate::model_config::is_gemini_live_translate_model_id(&transcription_model) {
        S2sMode::LiveTranslate
    } else {
        S2sMode::LegacyInterpreter
    };
    Ok(S2sSettings {
        api_key,
        model: if mode == S2sMode::LiveTranslate {
            crate::model_config::GEMINI_LIVE_TRANSLATE_API_MODEL.to_string()
        } else if model.is_empty() {
            crate::model_config::GEMINI_LIVE_API_MODEL_3_1.to_string()
        } else {
            crate::model_config::normalize_tts_gemini_model(model).to_string()
        },
        mode,
        voice: if voice.is_empty() {
            "Aoede".to_string()
        } else {
            voice.to_string()
        },
        speed: if speed.is_empty() {
            "Normal".to_string()
        } else {
            speed.to_string()
        },
        custom_instruction,
        target_language,
    })
}

mod batch;

pub use batch::{
    default_batch_settings_for_target, run_gemini_live_s2s_batch,
    run_gemini_live_s2s_batch_with_callbacks,
};

mod vad;
use vad::{collect_vad_segments, group_timed_segments, run_vad_loop};

mod session;
use session::{run_single_segment_session, session_worker};

pub(crate) mod transport;
use transport::{open_fresh_socket_session, process_segment};

mod output;
use output::{coordinate_output, s2s_backlog_ms};

pub(crate) mod utils;
use utils::*;

#[cfg(test)]
mod tests {
    use super::{
        AdaptiveS2sVadSnapshot, AdaptiveS2sVadState, FRAME_SAMPLES, Segment, SegmentOutcome,
        is_segment_worth_sending, merge_segment_text, segment_speech_like_ratio,
    };

    /// The Gemini S2S VAD/segmentation/timeout constants are hand-duplicated on
    /// Android (GeminiS2sVad.kt). Lock the Windows-canonical values against the
    /// shared fixture that the Android side asserts too. See
    /// .claude/parity/gemini-s2s-vad.md.
    #[test]
    fn s2s_vad_constants_match_parity_fixture() {
        use serde::Deserialize;
        use std::collections::BTreeMap;

        #[derive(Deserialize)]
        struct Fixture {
            ints: BTreeMap<String, i64>,
            floats: BTreeMap<String, f64>,
        }

        let fx: Fixture = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/parity-fixtures/gemini-s2s-vad/constants.json"
            ))
            .expect("fixture file"),
        )
        .expect("fixture json");

        let ints: &[(&str, i64)] = &[
            ("FRAME_SAMPLES", super::FRAME_SAMPLES as i64),
            ("PREROLL_SAMPLES", super::PREROLL_SAMPLES as i64),
            ("MIN_SEGMENT_SAMPLES", super::MIN_SEGMENT_SAMPLES as i64),
            (
                "TARGET_SEGMENT_SAMPLES",
                super::TARGET_SEGMENT_SAMPLES as i64,
            ),
            ("MAX_SEGMENT_SAMPLES", super::MAX_SEGMENT_SAMPLES as i64),
            ("END_SILENCE_FRAMES", super::END_SILENCE_FRAMES as i64),
            ("SESSION_COUNT", super::SESSION_COUNT as i64),
            (
                "MIN_SEGMENT_SPEECH_FRAMES",
                super::MIN_SEGMENT_SPEECH_FRAMES as i64,
            ),
            (
                "FIRST_AUDIO_SILENT_RETRY_MS",
                super::FIRST_AUDIO_SILENT_RETRY_MS as i64,
            ),
            (
                "FIRST_AUDIO_ACTIVE_RETRY_MS",
                super::FIRST_AUDIO_ACTIVE_RETRY_MS as i64,
            ),
            ("AUDIO_IDLE_FINISH_MS", super::AUDIO_IDLE_FINISH_MS as i64),
            ("S2S_HEDGE_TIMEOUT_MS", super::S2S_HEDGE_TIMEOUT_MS as i64),
            (
                "S2S_HEDGE_FINAL_TIMEOUT_MS",
                super::S2S_HEDGE_FINAL_TIMEOUT_MS as i64,
            ),
        ];
        for (name, value) in ints {
            assert_eq!(fx.ints.get(*name), Some(value), "int {name}");
        }
        assert_eq!(fx.ints.len(), ints.len(), "int count");

        let floats: &[(&str, f32)] = &[
            (
                "SPEECH_THRESHOLD_MULTIPLIER",
                super::SPEECH_THRESHOLD_MULTIPLIER,
            ),
            ("MIN_SPEECH_THRESHOLD", super::MIN_SPEECH_THRESHOLD),
            ("MAX_SPEECH_THRESHOLD", super::MAX_SPEECH_THRESHOLD),
            ("ABSOLUTE_SPEECH_RMS", super::ABSOLUTE_SPEECH_RMS),
            ("NOISE_LEARN_MAX_RMS", super::NOISE_LEARN_MAX_RMS),
            (
                "NOISE_LEARN_THRESHOLD_RATIO",
                super::NOISE_LEARN_THRESHOLD_RATIO,
            ),
            ("MIN_SEGMENT_PEAK_RMS", super::MIN_SEGMENT_PEAK_RMS),
            ("MIN_SEGMENT_SPEECH_RATIO", super::MIN_SEGMENT_SPEECH_RATIO),
            ("MIN_SPEECH_LIKE_RATIO", super::MIN_SPEECH_LIKE_RATIO),
            (
                "STRICT_MIN_SPEECH_LIKE_RATIO",
                super::STRICT_MIN_SPEECH_LIKE_RATIO,
            ),
            (
                "STRICT_MIN_SPEECH_CONFIDENCE",
                super::STRICT_MIN_SPEECH_CONFIDENCE,
            ),
        ];
        for (name, value) in floats {
            let fixture_val = fx
                .floats
                .get(*name)
                .unwrap_or_else(|| panic!("missing float {name}"));
            assert!(
                (*fixture_val - *value as f64).abs() < 1e-6,
                "float {name}: fixture {fixture_val} vs const {value}"
            );
        }
        assert_eq!(fx.floats.len(), floats.len(), "float count");
    }

    #[test]
    fn s2s_text_merge_preserves_short_word_boundaries() {
        let mut text = String::from("anh ta đang làm");
        merge_segment_text(&mut text, "một hệ điều hành");
        assert_eq!(text, "anh ta đang làm một hệ điều hành");

        let mut text = String::from("một việc lớn chuyên");
        merge_segment_text(&mut text, "nghiệp đâu");
        assert_eq!(text, "một việc lớn chuyên nghiệp đâu");
    }

    #[test]
    fn s2s_text_merge_keeps_real_partial_overlap() {
        let mut text = String::from("cái thú vui đó phát tri");
        merge_segment_text(&mut text, "triển thành một");
        assert_eq!(text, "cái thú vui đó phát triển thành một");
    }

    #[test]
    fn s2s_adaptive_vad_skips_silence() {
        let segment = Segment::new(1, vec![0; FRAME_SAMPLES * 8], 0, 0.0);
        assert!(!is_segment_worth_sending(
            &segment,
            AdaptiveS2sVadSnapshot::default()
        ));
    }

    #[test]
    fn s2s_adaptive_vad_skips_loud_flat_noise() {
        let segment = Segment::new(1, vec![8_000; FRAME_SAMPLES * 8], 8, 0.25);
        assert_eq!(segment.speech_like_frames, 0);
        assert!(!is_segment_worth_sending(
            &segment,
            AdaptiveS2sVadSnapshot { strictness: 1.0 }
        ));
    }

    #[test]
    fn s2s_adaptive_vad_keeps_speech_like_audio() {
        let mut samples = Vec::with_capacity(FRAME_SAMPLES * 8);
        for i in 0..FRAME_SAMPLES * 8 {
            let phase = i as f32 / 16_000.0;
            let envelope = if i % 1_600 < 1_100 { 1.0 } else { 0.45 };
            let sample =
                ((phase * 240.0 * std::f32::consts::TAU).sin() * 9_000.0 * envelope) as i16;
            samples.push(sample);
        }
        let segment = Segment::new(1, samples, 8, 0.20);
        assert!(segment_speech_like_ratio(&segment) > 0.7);
        assert!(is_segment_worth_sending(
            &segment,
            AdaptiveS2sVadSnapshot { strictness: 1.0 }
        ));
    }

    #[test]
    fn s2s_adaptive_vad_learns_from_gemini_feedback() {
        let segment = Segment::new(1, vec![8_000; FRAME_SAMPLES * 8], 8, 0.25);
        let mut state = AdaptiveS2sVadState::default();
        state.observe(SegmentOutcome::EmptyNoInput, &segment);
        assert!(state.snapshot().strictness > 0.0);
        state.observe(SegmentOutcome::Healthy, &segment);
        assert!(state.snapshot().strictness < 0.22);
    }
}
