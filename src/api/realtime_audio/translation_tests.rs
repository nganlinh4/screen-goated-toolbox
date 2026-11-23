use super::*;
use std::sync::{Mutex, mpsc};

#[test]
fn direct_or_pending_engine_transition_disables_text_force_commit() {
    assert!(text_translation_allowed(false, false, false, false, false));
    assert!(!text_translation_allowed(true, false, false, false, false));
    assert!(!text_translation_allowed(false, true, false, false, false));
    assert!(!text_translation_allowed(false, false, true, false, false));
    assert!(!text_translation_allowed(false, false, false, true, false));
    assert!(!text_translation_allowed(false, false, false, false, true));
}

#[test]
fn target_change_preserves_text_engine_but_retires_old_translation_and_direct_audio() {
    use std::sync::atomic::AtomicU64;
    let publication = Mutex::new(());
    let session = AtomicU64::new(1);
    let stopped = AtomicBool::new(false);
    let engine = AtomicU64::new(7);
    let options = AtomicU64::new(4);
    let changed = AtomicBool::new(false);
    let state = state_with_source();
    let request = state.get_translation_request().unwrap();
    assert!(translation_admission_matches(&state, &request, 0, 4, 4));

    crate::overlay::realtime_webview::target_change::update_target_revisions(
        false, &options, &engine, &changed,
    );
    assert!(
        crate::overlay::realtime_webview::state::publish_if_engine_current_with(
            &publication,
            &session,
            &engine,
            1,
            &stopped,
            7,
            || {},
        )
    );
    assert!(!translation_admission_matches(
        &state,
        &request,
        0,
        4,
        options.load(Ordering::SeqCst)
    ));
    assert!(!changed.load(Ordering::SeqCst));

    crate::overlay::realtime_webview::target_change::update_target_revisions(
        true, &options, &engine, &changed,
    );
    assert!(
        !crate::overlay::realtime_webview::state::publish_if_engine_current_with(
            &publication,
            &session,
            &engine,
            1,
            &stopped,
            7,
            || panic!("stale direct speech"),
        )
    );
    assert!(changed.load(Ordering::SeqCst));
}

fn state_with_source() -> super::super::state::RealtimeState {
    let mut state = super::super::state::RealtimeState::new();
    state.append_transcript("Same source.");
    state
}

#[test]
fn paused_old_provider_cannot_apply_to_new_session_with_same_source() {
    let session_id =
        crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst);
    let state = Arc::new(Mutex::new(state_with_source()));
    let request = state.lock().unwrap().get_translation_request().unwrap();
    let response = ValidatedTranslationResponse {
        finalized_translation: "old result".into(),
        draft_translation: String::new(),
    };
    let mut proof = state_with_source();
    assert!(proof.apply_translation_result(&request, &response.finalized_translation, ""));

    let old_stop = Arc::new(AtomicBool::new(false));
    let worker_stop = old_stop.clone();
    let worker_state = state.clone();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        ready_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        apply_translation_update(
            &worker_state,
            HWND::default(),
            &request,
            &response,
            session_id,
            &worker_stop,
            0,
            crate::overlay::realtime_webview::state::TRANSLATION_OPTIONS_REVISION
                .load(Ordering::SeqCst),
            &crate::overlay::realtime_webview::state::TRANSLATION_OPTIONS_REVISION,
        )
    });
    ready_rx.recv().unwrap();
    old_stop.store(true, Ordering::SeqCst);
    *state.lock().unwrap() = state_with_source();
    release_tx.send(()).unwrap();
    assert!(!worker.join().unwrap());
    assert!(state.lock().unwrap().committed_translation.is_empty());
}

#[test]
fn same_session_source_reset_rejects_old_response_even_for_same_text() {
    let session_id =
        crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst);
    let mut old = state_with_source();
    let request = old.get_translation_request().unwrap();
    old.freeze_current_transcript();
    old.append_transcript("Same source.");
    let state = Arc::new(Mutex::new(old));
    let response = ValidatedTranslationResponse {
        finalized_translation: "old result".into(),
        draft_translation: String::new(),
    };
    assert!(!apply_translation_update(
        &state,
        HWND::default(),
        &request,
        &response,
        session_id,
        &AtomicBool::new(false),
        0,
        crate::overlay::realtime_webview::state::TRANSLATION_OPTIONS_REVISION
            .load(Ordering::SeqCst),
        &crate::overlay::realtime_webview::state::TRANSLATION_OPTIONS_REVISION
    ));
    assert!(state.lock().unwrap().committed_translation.is_empty());
}

#[test]
fn fallback_does_not_replace_a_new_user_selected_model() {
    let requested = crate::model_config::REALTIME_TRANSLATION_MODEL_GTX;
    let selected = crate::model_config::REALTIME_TRANSLATION_MODEL_LLM;
    assert_eq!(fallback_model_if_current(requested, selected), None);
    assert_eq!(
        fallback_model_if_current(requested, requested),
        Some(selected)
    );
}

#[test]
fn fallback_routing_matches_shared_request_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/live-translate/translation-request-identity.json"
    )))
    .unwrap();
    for case in fixture["fallbackCases"].as_array().unwrap() {
        let selected = case["selected"].as_str().unwrap();
        let current = case["current"].as_str().unwrap();
        let state = state_with_source();
        let request = state.get_translation_request().unwrap();
        let request_revision = if case["requestCurrent"].as_bool().unwrap() {
            0
        } else {
            1
        };
        let alternative = translation_request_is_current(&state, &request, request_revision)
            .then(|| fallback_model_if_current(selected, current))
            .flatten();
        assert_eq!(alternative, case["fallback"].as_str(), "{}", case["name"]);
    }
}

#[test]
fn pending_source_correction_rejects_old_request_without_revision_change() {
    let session_id =
        crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst);
    let mut current = state_with_source();
    let request = current.get_translation_request().unwrap();
    current.set_transcript_segments("Newer words.", "");
    assert_eq!(current.translation_revision, 0);
    let state = Arc::new(Mutex::new(current));
    let response = ValidatedTranslationResponse {
        finalized_translation: "old result".into(),
        draft_translation: String::new(),
    };
    assert!(!apply_translation_update(
        &state,
        HWND::default(),
        &request,
        &response,
        session_id,
        &AtomicBool::new(false),
        0,
        crate::overlay::realtime_webview::state::TRANSLATION_OPTIONS_REVISION
            .load(Ordering::SeqCst),
        &crate::overlay::realtime_webview::state::TRANSLATION_OPTIONS_REVISION
    ));
    assert!(state.lock().unwrap().committed_translation.is_empty());
}

#[test]
fn delayed_response_is_rejected_after_target_or_model_change() {
    for _change in ["target", "model"] {
        let session_id =
            crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst);
        let state = Arc::new(Mutex::new(state_with_source()));
        let request = state.lock().unwrap().get_translation_request().unwrap();
        let options = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let worker_options = options.clone();
        let worker_state = state.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            apply_translation_update(
                &worker_state,
                HWND::default(),
                &request,
                &ValidatedTranslationResponse {
                    finalized_translation: "stale translation".into(),
                    draft_translation: String::new(),
                },
                session_id,
                &AtomicBool::new(false),
                0,
                1,
                &worker_options,
            )
        });
        ready_rx.recv().unwrap();
        options.fetch_add(1, Ordering::SeqCst);
        release_tx.send(()).unwrap();
        assert!(!worker.join().unwrap());
        assert!(state.lock().unwrap().committed_translation.is_empty());
    }
}

#[test]
fn request_identity_and_correction_retry_match_shared_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/live-translate/translation-request-identity.json"
    )))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut state = super::super::state::RealtimeState::new();
        state.set_transcript_segments(case["requested"].as_str().unwrap(), "");
        let request = state.get_translation_request().unwrap();
        state.set_transcript_segments(case["current"].as_str().unwrap(), "");
        assert_eq!(
            state.apply_translation_result(&request, "translated", ""),
            case["accepted"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
    for case in fixture["revisionCases"].as_array().unwrap() {
        let mut state = super::super::state::RealtimeState::new();
        state.set_transcript_segments(case["requested"].as_str().unwrap(), "");
        let request = state.get_translation_request().unwrap();
        let revision = state.translation_revision;
        let requested_options = 4;
        if case["reset"].as_bool() == Some(true) {
            state.freeze_current_transcript();
        }
        state.set_transcript_segments(case["current"].as_str().unwrap(), "");
        let current_options = if case["change"].is_string() { 5 } else { 4 };
        assert_eq!(
            translation_request_is_current(&state, &request, revision)
                && options_match(requested_options, current_options),
            case["accepted"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
    for case in fixture["forceCommitCases"].as_array().unwrap() {
        let pending = case["pending"].as_str().unwrap();
        let mut state = super::super::state::RealtimeState::new();
        state.set_transcript_segments(pending, "");
        state.uncommitted_source_end = pending.len();
        state.uncommitted_translation = "draft".into();
        state.last_transcript_append_time = Instant::now() - Duration::from_secs(3);
        state.last_translation_update_time = Instant::now() - Duration::from_secs(3);
        assert_eq!(
            state.should_force_commit_on_timeout(),
            case["commit"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
    for case in fixture["retryCases"].as_array().unwrap() {
        let mut state = super::super::state::RealtimeState::new();
        state.set_transcript_segments(case["processed"].as_str().unwrap(), "");
        let request = state.get_translation_request().unwrap();
        assert!(state.apply_translation_result(&request, "", "translated"));
        state.set_transcript_segments(case["current"].as_str().unwrap(), "");
        assert_eq!(
            !state.is_transcript_unchanged() && state.get_translation_request().is_some(),
            case["retry"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
    for case in fixture["prefixCases"].as_array().unwrap() {
        let mut state = super::super::state::RealtimeState::new();
        state.set_transcript_segments(case["previous"].as_str().unwrap(), "");
        state.last_committed_pos = state.full_transcript.len();
        state.set_transcript_segments(case["current"].as_str().unwrap(), "");
        assert_eq!(
            &state.full_transcript[..state.last_committed_pos],
            case["retained"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}
