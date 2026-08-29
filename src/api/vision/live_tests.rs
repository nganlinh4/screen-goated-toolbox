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
fn openrouter_nemotron_uses_nested_reasoning_and_prompt_only_structure() {
    let schema = serde_json::json!({"type": "object"});
    let payload = openrouter_vision_payload(
        "nvidia/nemotron-3-super-120b-a12b:free",
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

/// Sweeps a generated OCR corpus across the image chain to find what actually
/// provokes the restatement defect.
///
/// The corpus is built by `scratchpad/ocr-probe`, one image per hypothesis, each
/// varying a single property against a control: a repeated word in the image,
/// separator density, output length, image scale, line count. Reading one
/// failure tells you a model repeats; reading a matrix tells you when.
///
///     $env:OCR_PROBE_DIR = "...\scratchpad\ocr-probe"
///     $env:OCR_PROBE_SAMPLES = "3"
///     cargo test --bin screen-goated-toolbox ocr_repetition_matrix -- --ignored --nocapture
#[test]
#[ignore = "calls live vision endpoints"]
fn ocr_repetition_matrix() {
    let Ok(dir) = std::env::var("OCR_PROBE_DIR") else {
        eprintln!("set OCR_PROBE_DIR to the generated corpus");
        return;
    };
    let samples: usize = std::env::var("OCR_PROBE_SAMPLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let spacing_ms: u64 = std::env::var("OCR_PROBE_SPACING_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3_000);
    const PROMPT: &str =
        "Extract all text from this image exactly as it appears. Output ONLY the text.";
    let models: Vec<String> = std::env::var("OCR_PROBE_MODELS")
        .unwrap_or_else(|_| "qwen/qwen3.6-27b".to_string())
        .split(',')
        .map(str::to_string)
        .collect();

    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&dir).join("corpus.json"))
            .expect("corpus.json is written by the generator"),
    )
    .expect("corpus.json must parse");
    let credentials =
        crate::catalog_benchmark::setup::Credentials::load().expect("load benchmark credentials");

    println!(
        "groq key slots in rotation: {}",
        credentials.groq_pool_size()
    );
    for model_name in &models {
        let provider = if model_name.starts_with("gemini") {
            "google"
        } else {
            "groq"
        };
        println!(
            "
=== {model_name}"
        );
        let only: Option<Vec<String>> = std::env::var("OCR_PROBE_CASES")
            .ok()
            .map(|value| value.split(',').map(str::to_string).collect());
        for case in corpus.as_array().expect("corpus is an array") {
            let name = case["name"].as_str().unwrap();
            if only
                .as_ref()
                .is_some_and(|wanted| !wanted.iter().any(|id| id == name))
            {
                continue;
            }
            let expected = case["text"].as_str().unwrap();
            let path = std::path::Path::new(&dir).join(format!("{name}.png"));
            let bytes = std::fs::read(&path).expect("corpus image");
            let image = image::load_from_memory(&bytes).expect("png").to_rgba8();

            let mut verdicts = Vec::new();
            for _ in 0..samples {
                // Free vision tiers meter tightly, and a sweep is exactly the
                // shape that trips them: many small calls in quick succession.
                // Without pacing the matrix fills with 429s and measures the
                // quota rather than the model.
                std::thread::sleep(std::time::Duration::from_millis(spacing_ms));
                let mut reply = credentials.with_provider_key(provider, |key| {
                    translate_image_streaming(
                        TranslateImageRequest {
                            groq_api_key:
                                crate::catalog_benchmark::setup::Credentials::groq_key_for(
                                    provider, key,
                                ),
                            gemini_api_key: key,
                            prompt: PROMPT.to_string(),
                            model: model_name.clone(),
                            provider: provider.to_string(),
                            image: image.clone(),
                            original_bytes: Some(bytes.clone()),
                            streaming_enabled: false,
                            response_schema: None,
                            cancel_token: None,
                            request_timeout: Some(std::time::Duration::from_secs(60)),
                        },
                        |_| {},
                    )
                });
                if reply
                    .as_ref()
                    .err()
                    .is_some_and(|error| error.to_string().contains("429"))
                {
                    std::thread::sleep(std::time::Duration::from_secs(20));
                    reply = credentials.with_provider_key(provider, |key| {
                        translate_image_streaming(
                            TranslateImageRequest {
                                groq_api_key:
                                    crate::catalog_benchmark::setup::Credentials::groq_key_for(
                                        provider, key,
                                    ),
                                gemini_api_key: key,
                                prompt: PROMPT.to_string(),
                                model: model_name.clone(),
                                provider: provider.to_string(),
                                image: image.clone(),
                                original_bytes: Some(bytes.clone()),
                                streaming_enabled: false,
                                response_schema: None,
                                cancel_token: None,
                                request_timeout: Some(std::time::Duration::from_secs(60)),
                            },
                            |_| {},
                        )
                    });
                }
                verdicts.push(match reply {
                    Err(error) => format!(
                        "ERR({})",
                        error.to_string().chars().take(24).collect::<String>()
                    ),
                    Ok(text) => {
                        let tidy = text.trim();
                        if tidy == expected.trim() {
                            "ok".to_string()
                        } else {
                            format!("{:?}", tidy.chars().take(48).collect::<String>())
                        }
                    }
                });
            }
            println!("  {name:<22} {}", verdicts.join("  |  "));
        }
    }
}

#[test]
fn only_the_endpoint_measured_to_restate_is_salvaged() {
    use crate::model_config::vision_request_profile;

    // Measured on 2026-08-21 with a generated corpus driven through this path:
    // qwen restated its own output on 6 of 6 samples of one image and 4 of 6 of
    // another, while gemini-3.5-flash-lite was clean on 16 samples of the same
    // images. The salvage edits replies, so it runs only where the fault is.
    assert!(
        vision_request_profile("groq", "qwen/qwen3.6-27b").restates_output,
        "the endpoint the salvage exists for must still be flagged"
    );

    // Everything else is trusted until measured. An edit applied to a sound
    // endpoint can only remove correct text.
    for (provider, model) in [
        ("groq", "qwen/qwen3.8-27b"),
        ("google", "gemini-3.5-flash-lite"),
        ("google", "gemini-3.5-flash"),
        ("google", "gemma-4-26b-a4b-it"),
    ] {
        assert!(
            !vision_request_profile(provider, model).restates_output,
            "{model} was never measured to restate; flag it only with evidence"
        );
    }

    // An endpoint with no profile at all must not be guarded by accident.
    assert!(!vision_request_profile("groq", "not-a-real-model").restates_output);
}
