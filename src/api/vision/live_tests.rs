use super::*;

#[test]
fn qwen_payload_stays_below_tpm_and_disables_reasoning() {
    let payload = groq_vision_payload(
        "qwen/qwen3.6-27b",
        "prompt",
        "image/png",
        "AA==",
        false,
        None,
        false,
    );
    assert_eq!(payload["max_completion_tokens"], 512);
    assert_eq!(payload["reasoning_format"], "hidden");
    assert_eq!(payload["reasoning_effort"], "none");
    assert_eq!(payload["temperature"], 0.7);
    assert_eq!(payload["top_p"], 0.8);
    assert_eq!(payload["presence_penalty"], 1.5);
    assert!(payload.get("top_k").is_none());
    assert!(payload.get("min_p").is_none());

    let generic = groq_vision_payload(
        "future-vision-model",
        "prompt",
        "image/png",
        "AA==",
        false,
        None,
        false,
    );
    assert!(generic.get("max_completion_tokens").is_none());
    assert!(generic.get("reasoning_format").is_none());
}

#[test]
fn vision_schema_uses_generic_json_mode() {
    let schema = serde_json::json!({"type": "object"});
    let generic = groq_vision_payload(
        "future-vision-model",
        "prompt",
        "image/png",
        "AA==",
        false,
        Some(&schema),
        false,
    );
    let qwen = groq_vision_payload(
        "qwen/qwen3.6-27b",
        "prompt",
        "image/png",
        "AA==",
        false,
        Some(&schema),
        false,
    );
    assert_eq!(generic["response_format"]["type"], "json_object");
    assert_eq!(qwen["response_format"]["type"], "json_object");
}

#[test]
fn json_object_endpoints_wrap_plain_text_and_unwrap_the_reply() {
    use crate::model_config::vision_request_profile;

    let qwen = vision_request_profile("groq", "qwen/qwen3.6-27b");
    let generic = vision_request_profile("groq", "future-vision-model");

    // Only json-object endpoints, only plain text, only when not streaming.
    assert!(request_policy::needs_plain_text_envelope(
        qwen, false, false
    ));
    assert!(!request_policy::needs_plain_text_envelope(
        qwen, true, false
    ));
    assert!(!request_policy::needs_plain_text_envelope(
        qwen, false, true
    ));
    assert!(!request_policy::needs_plain_text_envelope(
        generic, false, false
    ));

    let payload = groq_vision_payload(
        "qwen/qwen3.6-27b",
        "Extract all text.",
        "image/png",
        "AA==",
        false,
        None,
        true,
    );
    assert_eq!(payload["response_format"]["type"], "json_object");
    let sent = payload["messages"][0]["content"][0]["text"]
        .as_str()
        .expect("text part is first for this endpoint");
    assert!(sent.starts_with("Extract all text."));
    assert!(sent.contains("\"text\""));

    assert_eq!(
        request_policy::unwrap_plain_text_envelope("{\"text\": \"Điều khiển máy tính\"}"),
        "Điều khiển máy tính"
    );
    // Fails open rather than losing text.
    assert_eq!(
        request_policy::unwrap_plain_text_envelope("Điều khiển máy tính"),
        "Điều khiển máy tính"
    );
    assert_eq!(
        request_policy::unwrap_plain_text_envelope("{\"other\": 1}"),
        "{\"other\": 1}"
    );
}

#[test]
fn openrouter_nemotron_uses_nested_reasoning_and_prompt_only_structure() {
    let schema = serde_json::json!({"type": "object"});
    let payload = openrouter_vision_payload(
        "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free",
        "prompt",
        "image/png",
        "AA==",
        false,
        Some(&schema),
    );

    assert_eq!(payload["reasoning"]["effort"], "none");
    assert!(payload.get("reasoning_effort").is_none());
    assert!(payload.get("response_format").is_none());
    assert_eq!(payload["messages"][0]["content"][0]["type"], "text");
}

#[test]
fn groq_retry_headers_and_error_bodies_are_structural() {
    let mut headers = ureq::http::HeaderMap::new();
    headers.insert("retry-after", "14.2".parse().unwrap());
    assert_eq!(retry_after_seconds(&headers), Some(15));
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/preset-system/vision-payload.json"
    )))
    .expect("vision payload parity fixture parses");
    assert_eq!(
        fixture["groq"]["short_retry_after_max_seconds"],
        GROQ_MAX_RATE_LIMIT_WAIT_SECS
    );
    assert_eq!(groq_rate_limit_retry_delay(429, 0, Some(2)), Some(2));
    assert_eq!(groq_rate_limit_retry_delay(429, 0, Some(3)), None);
    assert_eq!(groq_rate_limit_retry_delay(429, 1, Some(1)), None);
    assert_eq!(groq_rate_limit_retry_delay(503, 0, Some(1)), None);
    assert_eq!(
        groq_error_message(429, r#"{"error":{"message":"TPM exhausted"}}"#),
        "TPM exhausted"
    );
    assert_eq!(groq_error_message(500, "not json"), "HTTP 500");
}

#[test]
#[ignore = "requires GROQ_API_KEY and calls the live Groq vision endpoint"]
fn groq_rust_pipeline_live() {
    let api_key = std::env::var("GROQ_API_KEY").expect("GROQ_API_KEY is required");
    let image = if let Ok(path) = std::env::var("GROQ_TEST_IMAGE") {
        image::open(path).unwrap().to_rgba8()
    } else {
        let dimension = std::env::var("GROQ_TEST_DIMENSION")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1200);
        let width = std::env::var("GROQ_TEST_WIDTH")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(dimension);
        let height = std::env::var("GROQ_TEST_HEIGHT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(dimension);
        let mut state = 0x89ab_cdef_u32;
        ImageBuffer::from_fn(width, height, |_, _| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            Rgba([state as u8, (state >> 8) as u8, (state >> 16) as u8, 255])
        })
    };
    let answer = translate_image_streaming(
        TranslateImageRequest {
            groq_api_key: &api_key,
            gemini_api_key: "",
            prompt: std::env::var("GROQ_TEST_PROMPT")
                .unwrap_or_else(|_| "Reply with only OK.".to_string()),
            model: "qwen/qwen3.6-27b".to_string(),
            provider: "groq".to_string(),
            image,
            original_bytes: None,
            streaming_enabled: false,
            response_schema: None,
            cancel_token: None,
            request_timeout: None,
        },
        |_| {},
    )
    .unwrap();
    println!("[groq_rust_pipeline_live] answer={answer:?}");
    assert!(!answer.trim().is_empty());
    assert!(!answer.contains("<think>"));
}
