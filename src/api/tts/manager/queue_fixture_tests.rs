//! Replays the shared TTS queue contract against the Windows manager.
//!
//! Every case in the fixture is either replayed here against production
//! manager code or explicitly classified, so a case added for one platform
//! cannot silently go unenforced on the other.

use super::{RealtimeAudioSink, TtsManager};
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/parity-fixtures/tts-runtime/queue-semantics.json"
));

enum Coverage {
    /// Replayed against `TtsManager`; the closure returns the observed
    /// values for every key of the case's `expected` object.
    Replayed(fn() -> Value),
    /// Behaviour that exists only in the Android runtime.
    AndroidOnly,
    /// Windows owns this behaviour above the manager: committed-text offsets
    /// come from the realtime overlay's read cursor, and terminal delivery,
    /// failure retry and preview resume are Android coordinator protocols the
    /// Windows manager has no counterpart for.
    AboveWindowsManager,
}

fn coverage(name: &str) -> Option<Coverage> {
    use Coverage::*;
    Some(match name {
        "hide_during_playback_then_resume" => Replayed(hide_during_playback_then_resume),
        "new_text_after_global_stop" => Replayed(new_text_after_global_stop),
        "explicit_stop_fences_later_transcript_updates" => {
            Replayed(explicit_stop_fences_later_transcript_updates)
        }
        "realtime_completion_waits_for_request_registration" => {
            Replayed(realtime_completion_waits_for_request_registration)
        }
        "realtime_reset_retires_only_prior_generation" => {
            Replayed(realtime_reset_retires_only_prior_generation)
        }
        "announcement_interrupts_final_realtime_text"
        | "google_translate_fast_is_coerced_to_normal"
        | "google_translate_settings_preview_preserves_current_speech"
        | "edge_ssml_volume_is_not_applied_again_by_player"
        | "android_visible_tts_methods_follow_windows_selector"
        | "legacy_voxtral_tts_coerces_to_android_supported_method"
        | "android_open_weight_methods_are_feature_excluded"
        | "android_open_weight_settings_are_not_serialized" => AndroidOnly,
        "replacement_completes_before_interrupted_realtime_event"
        | "preset_speech_admission_does_not_block_session_retirement"
        | "bounded_terminal_delivery_isolated_by_owner"
        | "bounded_terminal_delivery_isolated_across_owner_tokens"
        | "realtime_failure_cannot_skip_later_committed_text"
        | "realtime_preview_resumes_unheard_final_segment"
        | "realtime_skips_existing_history_on_first_enable"
        | "explicit_interrupt_replays_unsent_realtime_tail" => AboveWindowsManager,
        _ => return None,
    })
}

fn cases() -> Vec<Value> {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("queue fixture parses");
    fixture["cases"].as_array().expect("fixture cases").clone()
}

#[test]
fn every_queue_fixture_case_is_replayed_or_classified() {
    let mut replayed = 0;
    for case in cases() {
        let name = case["name"].as_str().expect("case name");
        let coverage = coverage(name).unwrap_or_else(|| panic!("unclassified case {name}"));
        if let Coverage::Replayed(replay) = coverage {
            let expected = case["expected"].as_object().expect("expected fields");
            let observed = replay();
            for (field, value) in expected {
                assert_eq!(&observed[field], value, "{name}.{field}");
            }
            replayed += 1;
        }
    }
    assert_eq!(replayed, 5);
}

#[test]
fn queue_fixture_classifications_name_real_cases() {
    let names: Vec<String> = cases()
        .iter()
        .map(|case| case["name"].as_str().unwrap().to_string())
        .collect();
    let android_only = names
        .iter()
        .filter(|name| matches!(coverage(name), Some(Coverage::AndroidOnly)))
        .count();
    let above_manager = names
        .iter()
        .filter(|name| matches!(coverage(name), Some(Coverage::AboveWindowsManager)))
        .count();
    assert_eq!(android_only, 8);
    assert_eq!(above_manager, 8);
    assert_eq!(names.len(), 21);
}

/// Realtime offset cases are produced above the manager (on Windows by the
/// realtime overlay's read cursor), but their offsets must at least agree with
/// the committed text they describe.
#[test]
fn realtime_offset_expectations_agree_with_their_committed_text() {
    let mut checked = 0;
    for case in cases() {
        let Some(text) = case["initial_committed_text"].as_str() else {
            continue;
        };
        let name = case["name"].as_str().unwrap();
        let expected = &case["expected"];
        let spoken = expected["spoken_length"].as_u64().unwrap() as usize;
        let queued = expected["queued_length"].as_u64().unwrap() as usize;
        assert!(spoken <= queued && queued <= text.len(), "{name}");
        let segments: String = expected["queued_segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|segment| segment.as_str().unwrap())
            .collect();
        assert_eq!(&text[spoken..queued], segments, "{name}");
        checked += 1;
    }
    assert_eq!(checked, 2);
}

struct SilentSink;

impl RealtimeAudioSink for SilentSink {
    fn play(&self, _: &[u8]) {}
    fn stop(&self) {}
}

/// Hiding stops the session; reopening must not replay the in-flight segment
/// that was admitted under the stopped session.
fn hide_during_playback_then_resume() -> Value {
    let manager = TtsManager::new();
    let sink: Arc<dyn RealtimeAudioSink> = Arc::new(SilentSink);
    let stopped = AtomicBool::new(false);
    let visible = AtomicBool::new(true);
    let epoch = manager.realtime_epoch();
    assert!(manager.admit_direct_realtime_audio(&sink, &[1], &stopped, &visible, epoch));
    manager.stop_realtime();
    let replayed = manager.admit_direct_realtime_audio(&sink, &[1], &stopped, &visible, epoch);
    manager.release_direct_realtime(&sink);
    serde_json::json!({ "in_flight_segment_replayed": replayed })
}

fn new_text_after_global_stop() -> Value {
    let manager = TtsManager::new();
    let epoch = manager.realtime_epoch();
    let old = manager.speak_realtime("old text", 0, epoch).unwrap();
    manager.stop_realtime();
    let old_text_replayed =
        manager.is_speaking(old) || manager.speak_realtime("old text", 0, epoch).is_some();
    let new_text_spoken = manager
        .speak_realtime("new text", 0, manager.realtime_epoch())
        .is_some_and(|request| manager.is_speaking(request));
    serde_json::json!({
        "old_text_replayed": old_text_replayed,
        "new_text_spoken": new_text_spoken,
    })
}

fn explicit_stop_fences_later_transcript_updates() -> Value {
    let manager = TtsManager::new();
    let epoch = manager.realtime_epoch();
    manager.speak_realtime("interrupted", 0, epoch).unwrap();
    manager.stop_realtime();
    let restarted = manager
        .speak_realtime("interrupted, then updated", 0, epoch)
        .is_some();
    serde_json::json!({ "same_generation_restarts_speech": restarted })
}

fn realtime_completion_waits_for_request_registration() -> Value {
    let manager = Arc::new(TtsManager::new());
    let epoch = manager.realtime_epoch();
    let request = manager.speak_realtime("committed", 0, epoch).unwrap();
    let before = manager.realtime_text_backlog();
    let completer = {
        let manager = manager.clone();
        std::thread::spawn(move || manager.finish_realtime_text(request))
    };
    completer.join().unwrap();
    let completion_recorded = manager.realtime_text_backlog() + 1 == before;
    // A late duplicate completion must not count the same request twice.
    manager.finish_realtime_text(request);
    let pending_depth = manager.realtime_text_backlog();
    serde_json::json!({
        "completion_recorded": completion_recorded,
        "duplicate_replay": pending_depth != before - 1,
        "pending_depth": pending_depth,
    })
}

fn realtime_reset_retires_only_prior_generation() -> Value {
    let manager = TtsManager::new();
    let unrelated = manager.speak("another window", 0);
    let prior = manager
        .speak_realtime("prior text", 0, manager.realtime_epoch())
        .unwrap();
    manager.stop_realtime();
    serde_json::json!({
        "prior_request_current": manager.is_speaking(prior),
        "unrelated_consumer_stopped": !manager.is_speaking(unrelated),
    })
}
