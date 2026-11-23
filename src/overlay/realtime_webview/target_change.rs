use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub(crate) fn update_target_revisions(
    direct_speech: bool,
    options: &AtomicU64,
    engine: &AtomicU64,
    language_change: &AtomicBool,
) {
    options.fetch_add(1, Ordering::SeqCst);
    if direct_speech {
        engine.fetch_add(1, Ordering::SeqCst);
        language_change.store(true, Ordering::SeqCst);
    }
}

pub(super) fn retry_untranslated_tail(state: &mut crate::api::realtime_audio::RealtimeState) {
    state.last_processed_len = state.last_committed_pos;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_change_preserves_text_transcription_and_restarts_direct_speech() {
        let options = AtomicU64::new(4);
        let engine = AtomicU64::new(7);
        let language_change = AtomicBool::new(false);
        update_target_revisions(false, &options, &engine, &language_change);
        assert_eq!(options.load(Ordering::SeqCst), 5);
        assert_eq!(engine.load(Ordering::SeqCst), 7);
        assert!(!language_change.load(Ordering::SeqCst));
        update_target_revisions(true, &options, &engine, &language_change);
        assert_eq!(options.load(Ordering::SeqCst), 6);
        assert_eq!(engine.load(Ordering::SeqCst), 8);
        assert!(language_change.load(Ordering::SeqCst));
    }

    #[test]
    fn target_change_retries_only_untranslated_tail_from_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/live-translate/target-change.json"
        )))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let committed = case["committed"].as_str().unwrap();
            let draft = case["draft"].as_str().unwrap();
            let mut state = crate::api::realtime_audio::RealtimeState::new();
            state.full_transcript = format!("{committed}{draft}");
            state.last_committed_pos = committed.len();
            state.last_processed_len = state.full_transcript.len();
            retry_untranslated_tail(&mut state);
            assert_eq!(
                state
                    .get_translation_request()
                    .map(|request| request.pending_source),
                case["requestSource"].as_str().map(str::to_owned),
                "{}",
                case["name"]
            );
            assert_eq!(
                state.is_transcript_unchanged(),
                draft.is_empty(),
                "{}",
                case["name"]
            );
        }
    }
}
