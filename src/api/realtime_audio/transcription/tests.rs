use super::*;

#[test]
fn dedicated_transcript_replaces_interim_and_commits_authoritative_text() {
    let mut transcript = DedicatedTranscriptState::default();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/gemini-transcribe-stream/events.json"
    )))
    .unwrap();
    for event in fixture["events"].as_array().unwrap() {
        let frame = crate::api::gemini_live::server_frame::parse_server_frame(
            &event["payload"].to_string(),
        )
        .unwrap();
        if let Some(final_text) = frame.input_transcript {
            transcript.commit_final(&final_text);
        } else if let Some(interim) = frame.interim_input_transcript {
            transcript.replace_interim(&interim);
        }
    }
    assert_eq!(
        transcript.delivery.committed(),
        fixture["expectedCommitted"]
    );
    assert_eq!(transcript.delivery.interim(), fixture["expectedInterim"]);
    transcript.commit_final("Bring the agenda.");
    assert_eq!(
        transcript.delivery.committed(),
        "Meet Wednesday at 2:00 PM. Bring the agenda."
    );
}

#[test]
fn dedicated_interim_uses_sentence_boundaries_without_claiming_server_finality() {
    let mut state = crate::api::realtime_audio::state::RealtimeState::new();
    state.set_transcription_method(
        crate::api::realtime_audio::state::TranscriptionMethod::GeminiTranscribe,
    );
    state.set_transcript_segments("", "Complete thought. uncertain words");
    let interim = state.get_translation_request().unwrap();
    assert_eq!(interim.finalized_source, "Complete thought.");
    assert_eq!(interim.draft_source, " uncertain words");

    state.set_transcript_segments("Corrected words without punctuation", "");
    let final_request = state.get_translation_request().unwrap();
    assert_eq!(
        final_request.finalized_source,
        "Corrected words without punctuation"
    );
    assert!(final_request.draft_source.is_empty());
}

#[test]
fn dedicated_punctuated_translation_rolls_back_after_interim_correction() {
    let mut state = crate::api::realtime_audio::state::RealtimeState::new();
    state.set_transcription_method(
        crate::api::realtime_audio::state::TranscriptionMethod::GeminiTranscribe,
    );
    state.set_transcript_segments("", "Wrong day. trailing words");
    let request = state.get_translation_request().unwrap();
    assert!(state.apply_translation_result(&request, "Sai ngày.", "từ tiếp theo"));
    assert_eq!(state.last_committed_pos, "Wrong day.".len());
    assert_eq!(state.transcript_committed_pos, "Wrong day.".len());

    state.set_transcript_segments("", "Wrong day. trailing words continue");
    assert_eq!(state.transcript_committed_pos, "Wrong day.".len());

    state.set_transcript_segments("", "Right day. trailing words");
    assert_eq!(state.last_committed_pos, 0);
    assert_eq!(state.transcript_committed_pos, 0);
    assert!(state.committed_translation.is_empty());
    assert!(state.uncommitted_translation.is_empty());
}

#[test]
fn dedicated_unpunctuated_translation_uses_bounded_silence_fallback() {
    let mut state = crate::api::realtime_audio::state::RealtimeState::new();
    state.set_transcription_method(
        crate::api::realtime_audio::state::TranscriptionMethod::GeminiTranscribe,
    );
    state.set_transcript_segments("", "stable words without punctuation");
    let request = state.get_translation_request().unwrap();
    assert!(state.apply_translation_result(&request, "", "bản dịch ổn định"));
    state.last_transcript_append_time = Instant::now() - Duration::from_millis(900);
    state.last_translation_update_time = Instant::now() - Duration::from_millis(1_100);
    assert!(state.should_force_commit_on_timeout());
}

#[test]
fn dedicated_transcription_streams_continuously_without_periodic_silence() {
    assert!(!uses_periodic_silence_cycle(true));
    assert!(uses_periodic_silence_cycle(false));
}
