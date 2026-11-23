use super::*;

#[test]
fn unary_audio_keeps_its_explicit_transcription_configuration() {
    let transcription = serde_json::json!({
        "languageCodes": [], "mode": "SMART", "customVocabulary": ["Alpha"]
    });
    let payload = gemini_payload(
        serde_json::json!([{"inline_data": {"mime_type": "audio/wav", "data": "AA=="}}]),
        crate::model_config::GEMINI_TRANSCRIBE_BATCH_API_MODEL,
        None,
        None,
        false,
        Some(&transcription),
    );
    assert_eq!(
        payload["generationConfig"]["audioTranscriptionConfig"],
        transcription
    );
    assert!(payload.get("tools").is_none());
    assert_eq!(payload["generationConfig"].as_object().unwrap().len(), 1);
    assert!(
        !gemini_content_url(
            crate::model_config::GEMINI_TRANSCRIBE_BATCH_API_MODEL,
            false
        )
        .contains("streamGenerateContent")
    );
}

#[test]
fn cancelled_unary_transcription_never_starts_a_provider_request() {
    let cancelled = Some(Arc::new(AtomicBool::new(true)));
    let config = serde_json::json!({"mode": "SMART"});
    let result = stream_gemini_generate(
        GeminiGenerateRequest {
            parts: serde_json::json!([]),
            model: "future-audio-model",
            api_key: "synthetic-key",
            streaming: false,
            ui_language: "en",
            cancel_token: &cancelled,
            error_label: None,
            map_auth_errors: true,
            request_timeout: None,
            response_schema: None,
            audio_transcription_config: Some(&config),
            media_resolution: None,
            enable_google_search: false,
            retry_observer: None,
        },
        &mut |_| panic!("cancelled request emitted content"),
    );
    assert_eq!(result.unwrap_err().to_string(), "Cancelled");
}

#[test]
fn ordinary_generation_never_implicitly_enables_provider_tools() {
    let payload = gemini_payload(
        serde_json::json!([{"text": "plain request"}]),
        "gemini-future-model",
        None,
        None,
        false,
        None,
    );
    assert!(payload.get("tools").is_none());
}

#[test]
fn explicit_search_generation_enables_only_google_search() {
    let payload = gemini_payload(
        serde_json::json!([{"text": "current information"}]),
        "gemini-3.1-flash-lite",
        None,
        None,
        true,
        None,
    );
    assert_eq!(payload["tools"], serde_json::json!([{"google_search": {}}]));
}

#[test]
fn media_resolution_is_an_explicit_generation_policy() {
    let payload = gemini_payload(
        serde_json::json!([{"text": "plain request"}]),
        "gemini-3.5-flash-lite",
        None,
        Some(MediaResolution::Low),
        false,
        None,
    );
    assert_eq!(
        payload["generationConfig"]["mediaResolution"],
        "MEDIA_RESOLUTION_LOW"
    );
}

#[test]
fn structured_output_requires_both_schema_and_catalog_support() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": { "text": { "type": "string" } },
        "required": ["text"]
    });
    let gemma = gemini_payload(
        serde_json::json!([{"text": "extract"}]),
        "gemma-4-31b-it",
        Some(&schema),
        None,
        false,
        None,
    );
    let flash = gemini_payload(
        serde_json::json!([{"text": "extract"}]),
        "gemini-3.5-flash-lite",
        Some(&schema),
        None,
        false,
        None,
    );
    assert_eq!(
        gemma["generationConfig"]["responseMimeType"],
        "application/json"
    );
    assert_eq!(gemma["generationConfig"]["responseJsonSchema"], schema);
    assert_eq!(
        flash["generationConfig"]["responseMimeType"],
        "application/json"
    );
    assert_eq!(flash["generationConfig"]["responseJsonSchema"], schema);
}

#[test]
fn structured_error_keeps_quota_metadata_and_retry_delay() {
    let error = GeminiHttpError::parse(
        429,
        r#"{"error":{"status":"RESOURCE_EXHAUSTED","message":"quota exhausted","details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"2.25s"},{"metadata":{"quotaMetric":"generate_content","model":"future-model"}}]}}"#,
        None,
    );
    let message = error.display();
    assert!(message.contains("HTTP 429 RESOURCE_EXHAUSTED"));
    assert!(message.contains("retry_after_ms=2250"));
    assert!(message.contains("quotaMetric"));
    assert!(message.contains("future-model"));
}

#[test]
fn invalid_key_400_is_a_credential_error_not_a_schema_rejection() {
    let error = GeminiHttpError::parse(
        400,
        r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"API_KEY_INVALID","domain":"googleapis.com"}]}}"#,
        None,
    );
    assert!(error.is_invalid_api_key());
    assert!(!error.rejects_response_schema());
}

#[test]
fn only_schema_attributed_invalid_arguments_are_schema_rejections() {
    let schema = GeminiHttpError::parse(
        400,
        r#"{"error":{"code":400,"message":"Invalid JSON payload","status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.rpc.BadRequest","fieldViolations":[{"field":"generation_config.response_json_schema.properties[0]","description":"unsupported"}]}]}}"#,
        None,
    );
    assert!(schema.rejects_response_schema());
    assert!(!schema.is_invalid_api_key());

    let message_only = GeminiHttpError::parse(
        400,
        r#"{"error":{"code":400,"message":"* GenerateContentRequest.generation_config.responseSchema: too deep","status":"INVALID_ARGUMENT"}}"#,
        None,
    );
    assert!(message_only.rejects_response_schema());

    let oversized = GeminiHttpError::parse(
        400,
        r#"{"error":{"code":400,"message":"The input token count exceeds the maximum number of tokens allowed.","status":"INVALID_ARGUMENT"}}"#,
        None,
    );
    assert!(!oversized.rejects_response_schema());
    assert!(!oversized.is_invalid_api_key());
}

#[test]
fn retry_policy_is_transient_and_bounded() {
    assert!(GeminiHttpError::parse(503, "", None).retryable());
    assert!(!GeminiHttpError::parse(400, "", None).retryable());
    for attempt in 0..10 {
        assert!(gemini_retry_delay(attempt, None) <= MAX_GEMINI_RETRY_DELAY);
    }
}

#[test]
fn explicit_long_retry_delay_is_preserved_for_fail_fast_fallback() {
    let delay = Duration::from_secs(45);
    let error = GeminiHttpError::parse(429, "{}", Some(delay));
    assert_eq!(error.retry_after, Some(delay));
    assert!(
        error
            .retry_after
            .is_some_and(|value| value > MAX_GEMINI_RETRY_DELAY)
    );
}
