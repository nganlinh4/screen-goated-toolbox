use super::*;

#[test]
fn qwen_payload_stays_below_tpm_and_hides_reasoning() {
    let payload = groq_vision_payload(
        "qwen/qwen3.6-27b",
        "prompt",
        "image/png",
        "AA==",
        false,
        None,
    );
    assert_eq!(payload["max_completion_tokens"], 2048);
    assert_eq!(payload["reasoning_format"], "hidden");
    assert!(payload.get("reasoning_effort").is_none());

    let generic = groq_vision_payload(
        "future-vision-model",
        "prompt",
        "image/png",
        "AA==",
        false,
        None,
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
    );
    let qwen = groq_vision_payload(
        "qwen/qwen3.6-27b",
        "prompt",
        "image/png",
        "AA==",
        false,
        Some(&schema),
    );
    assert_eq!(generic["response_format"]["type"], "json_object");
    assert_eq!(qwen["response_format"]["type"], "json_object");
}

#[test]
fn groq_retry_headers_and_error_bodies_are_structural() {
    let mut headers = ureq::http::HeaderMap::new();
    headers.insert("retry-after", "14.2".parse().unwrap());
    assert_eq!(retry_after_seconds(&headers), Some(15));
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
            prompt: "Reply with only OK.".to_string(),
            model: "qwen/qwen3.6-27b".to_string(),
            provider: "groq".to_string(),
            image,
            original_bytes: None,
            streaming_enabled: false,
            use_json_format: false,
            response_schema: None,
            cancel_token: None,
            request_timeout: None,
        },
        |_| {},
    )
    .unwrap();
    assert!(!answer.trim().is_empty());
    assert!(!answer.contains("<think>"));
}
