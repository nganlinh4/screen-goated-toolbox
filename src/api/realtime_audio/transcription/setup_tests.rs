use super::*;

#[test]
fn gemini_transcribe_setup_matches_shared_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/gemini-transcribe-setup/live-transcribe.json"
    )))
    .expect("Gemini Transcribe setup fixture parses");
    let actual = build_realtime_transcription_setup(
        fixture["model"].as_str().expect("fixture model"),
        &[],
        None,
    );
    assert_eq!(actual["setup"], fixture["setup"]);
}

#[test]
fn native_audio_transcription_keeps_audio_response_profile() {
    let setup = build_realtime_transcription_setup(
        crate::model_config::GEMINI_LIVE_API_MODEL_3_1,
        &[],
        None,
    );
    assert_eq!(
        setup["setup"]["generationConfig"]["responseModalities"],
        serde_json::json!(["AUDIO"])
    );
    assert_eq!(
        setup["setup"]["generationConfig"]["mediaResolution"],
        "MEDIA_RESOLUTION_LOW"
    );
}

#[test]
fn gemini_transcribe_setup_includes_vocabulary_and_resumption() {
    let model = crate::model_config::realtime_transcription_api_model(
        "google-gemini-3-5-transcribe-live-audio",
    );
    let setup = build_realtime_transcription_setup(
        &model,
        &["Screen Goated".to_string(), "WebView2".to_string()],
        Some("resume-handle"),
    );
    assert_eq!(
        setup["setup"]["inputAudioTranscription"]["customVocabulary"],
        serde_json::json!(["Screen Goated", "WebView2"])
    );
    assert_eq!(
        setup["setup"]["sessionResumption"]["handle"],
        "resume-handle"
    );
}

#[test]
fn dedicated_transcription_uses_server_connection_state_for_reconnects() {
    assert!(!reconnect_on_no_results_for_protocol(Some(
        "live-transcribe"
    )));
    assert!(reconnect_on_no_results_for_protocol(Some("native-audio")));
}

#[test]
fn capture_starts_before_the_live_session_and_gates_it() {
    use std::cell::RefCell;
    let order = RefCell::new(Vec::new());
    let opened = start_capture_then_session(
        || {
            order.borrow_mut().push("capture");
            Ok(Some("stream"))
        },
        || {
            order.borrow_mut().push("session");
            Ok(Some("session"))
        },
    )
    .unwrap();
    assert_eq!(opened, Some(("stream", "session")));
    assert_eq!(*order.borrow(), ["capture", "session"]);

    let device_failure = start_capture_then_session::<(), ()>(
        || Err(anyhow::anyhow!("device unavailable")),
        || panic!("no session may open without capture"),
    );
    assert_eq!(
        device_failure.unwrap_err().to_string(),
        "device unavailable"
    );
    let selection_cancelled = start_capture_then_session::<(), ()>(
        || Ok(None),
        || panic!("no session may open after a cancelled selection"),
    );
    assert!(selection_cancelled.unwrap().is_none());
    let superseded = start_capture_then_session(|| Ok(Some("stream")), || Ok(None::<()>));
    assert!(superseded.unwrap().is_none());
}
