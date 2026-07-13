use std::collections::{BTreeMap, VecDeque};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use anyhow::Result;
use windows::Win32::Foundation::HWND;

use crate::APP;
use crate::api::gemini_live::lifecycle::{
    LiveBackoffPolicy, LiveClassifiedError, LiveLifecycleEffect, LiveLifecycleEvent,
    LiveLifecycleFrame, LiveLifecyclePolicy, LiveReconnectReason, LiveSessionLifecycle,
    LiveSessionPhase,
};
use crate::api::gemini_live::ready_session::{
    ConnectedLiveSocket, LivePoll, LiveSetupServerError, OpenOptions, ReadyLiveSession,
};
use crate::api::gemini_live::server_frame::LiveServerFrame;
use crate::api::gemini_live::transport::is_recoverable_anyhow_socket_error;
use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::AudioEvent;
use crate::config::Preset;
use crate::overlay::realtime_webview::{
    AUDIO_SOURCE_CHANGE, LANGUAGE_CHANGE, SELECTED_APP_PID, TRANSCRIPTION_MODEL_CHANGE,
};

use super::capture::{start_mic_capture_resilient, start_per_app_capture};
use super::state::SharedRealtimeState;
use super::utils::{update_overlay_text, update_translation_text};
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

mod context;
use context::{S2sContextMemory, S2sContextSnapshot};

mod events;
use events::{S2sEvent, S2sRaceEvent, format_s2s_attempt_counts, s2s_attempt_counts};

mod segment;
use segment::{
    AdaptiveS2sVadSnapshot, AdaptiveS2sVadState, Segment, SegmentOutcome, SegmentSampleMetrics,
    segment_audio_ms, segment_peak_sample,
};

mod settings;
pub use settings::S2sBatchSettings;
use settings::{S2sMode, S2sSettings, load_settings};

mod types;
pub use types::S2sBatchSegment;
use types::{
    HedgedAttemptRequest, HedgedAttemptResources, HedgedSegmentRequest, ProcessSegmentParams,
    S2sSessionResources, TimedSegment,
};

mod live;
pub use live::run_gemini_live_s2s;

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

    /// Lock the is_segment_worth_sending accept RULE (baseline gate + strict/lenient
    /// paths + the 0.08 speech-like floor) against the shared fixture the Android
    /// side asserts too. See .claude/parity/gemini-s2s-vad.md.
    #[test]
    fn s2s_accept_rule_matches_parity_fixture() {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Doc {
            cases: Vec<Case>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Case {
            name: String,
            frame_count: usize,
            speech_frames: usize,
            speech_like_frames: usize,
            energetic_frames: usize,
            peak_rms: f32,
            mean_rms: f32,
            strictness: f32,
            expect_accept: bool,
        }

        let doc: Doc = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/parity-fixtures/gemini-s2s-vad/accept-rule.json"
            ))
            .expect("fixture file"),
        )
        .expect("fixture json");

        for c in doc.cases {
            let segment = super::Segment {
                id: 0,
                samples: vec![0i16; c.frame_count * FRAME_SAMPLES],
                speech_frames: c.speech_frames,
                peak_rms: c.peak_rms,
                mean_rms: c.mean_rms,
                energetic_frames: c.energetic_frames,
                speech_like_frames: c.speech_like_frames,
            };
            let vad = AdaptiveS2sVadSnapshot {
                strictness: c.strictness,
            };
            assert_eq!(
                is_segment_worth_sending(&segment, vad),
                c.expect_accept,
                "case {}",
                c.name
            );
        }
    }

    /// Lock the grouped-timeout formulas against the shared fixture.
    #[test]
    fn s2s_grouped_timeouts_match_parity_fixture() {
        use serde::Deserialize;

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Doc {
            first_audio: Vec<FirstAudio>,
            hard: Vec<Hard>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct FirstAudio {
            source_audio_ms: u128,
            text_updates: usize,
            expect_ms: u128,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Hard {
            source_audio_ms: u128,
            final_attempt: bool,
            expect_ms: u128,
        }

        let doc: Doc = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/parity-fixtures/gemini-s2s-vad/timeouts.json"
            ))
            .expect("fixture file"),
        )
        .expect("fixture json");

        for c in doc.first_audio {
            assert_eq!(
                super::grouped_first_audio_timeout_ms(c.source_audio_ms, c.text_updates),
                c.expect_ms,
                "first_audio src={} updates={}",
                c.source_audio_ms,
                c.text_updates
            );
        }
        for c in doc.hard {
            assert_eq!(
                super::grouped_hard_timeout_ms(c.source_audio_ms, c.final_attempt),
                c.expect_ms,
                "hard src={} final={}",
                c.source_audio_ms,
                c.final_attempt
            );
        }
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
