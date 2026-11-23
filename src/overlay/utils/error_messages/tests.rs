use super::{extract_http_status_code, should_advance_retry_chain, should_block_retry_provider};

#[test]
fn extracts_status_codes_from_provider_phrasings() {
    assert_eq!(
        extract_http_status_code("OpenRouter API Error HTTP 402: Payment required"),
        Some(402)
    );
    assert_eq!(
        extract_http_status_code("request failed with status code 429"),
        Some(429)
    );
    assert_eq!(
        extract_http_status_code("upstream returned: 503"),
        Some(503)
    );
    assert_eq!(
        extract_http_status_code("Error 422: unprocessable"),
        Some(422)
    );
}

#[test]
fn ignores_digits_embedded_in_model_names() {
    assert_eq!(extract_http_status_code("model qwen3-235b failed"), None);
    assert_eq!(extract_http_status_code("llama-3.3-70b-versatile"), None);
    assert_eq!(extract_http_status_code("connection reset by peer"), None);
}

#[test]
fn advances_chain_for_billing_and_payload_failures() {
    assert!(should_advance_retry_chain(
        "OpenRouter API Error HTTP 402: Payment required to access this resource. Visit your billing tab."
    ));
    assert!(should_advance_retry_chain("insufficient_quota"));
    assert!(should_advance_retry_chain(
        "request failed with status code 422"
    ));
    assert!(should_advance_retry_chain("deadline exceeded"));
    assert!(should_advance_retry_chain(
        "PROVIDER_TRANSPORT_UNAVAILABLE: TLS handshake failed"
    ));
}

#[test]
fn blocks_provider_for_billing_exhaustion() {
    assert!(should_block_retry_provider(
        "OpenRouter API Error HTTP 402: Payment required to access this resource. Visit your billing tab."
    ));
    assert!(should_block_retry_provider("credit balance is too low"));
    assert!(!should_block_retry_provider(
        "Error 429: quota exceeded, please retry"
    ));
}

#[test]
fn advances_chain_for_auth_and_not_found_failures() {
    assert!(should_advance_retry_chain("NO_API_KEY:google"));
    assert!(should_advance_retry_chain("INVALID_API_KEY"));
    assert!(should_advance_retry_chain(
        "request failed with status code 401"
    ));
    assert!(should_advance_retry_chain(
        "request failed with status code 404"
    ));
    assert!(should_advance_retry_chain("unsupported model"));
    assert!(should_advance_retry_chain(
        "EMPTY_MODEL_RESPONSE:qwen/qwen3.8-27b"
    ));
}

#[test]
fn blocks_provider_for_provider_wide_failures() {
    assert!(should_block_retry_provider("NO_API_KEY:groq"));
    assert!(should_block_retry_provider("INVALID_API_KEY"));
    assert!(should_block_retry_provider("PROVIDER_DISABLED:google"));
    assert!(should_block_retry_provider(
        "Groq transport error: PROVIDER_TRANSPORT_UNAVAILABLE: timeout: send request"
    ));
    assert!(should_block_retry_provider(
        "STRUCTURED_OUTPUT_REJECTED:google:HTTP 400 INVALID_ARGUMENT"
    ));
    assert!(should_block_retry_provider(
        "request failed with status code 403"
    ));
    assert!(!should_block_retry_provider(
        "request failed with status code 404"
    ));
    assert!(!should_block_retry_provider(
        "Groq transport error: timeout: receive response"
    ));
    assert!(!should_block_retry_provider(
        "Groq transport error: timeout: receive body"
    ));
    assert!(!should_block_retry_provider(
        "Groq transport error: timeout: global"
    ));
}

#[test]
fn credential_errors_keep_the_shared_notice_text() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/api-key-notifications/triggers.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let raw = case["raw_error"].as_str().unwrap();
        if !raw.contains("API_KEY") {
            continue;
        }
        assert_eq!(
            super::get_error_message(raw, "en", None),
            case["expected_en"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn http_failures_name_the_provider_apart_from_the_message_text() {
    let failure = super::HttpFailure::detect(
        "Error: https://api.groq.com/openai/v1/chat/completions: status code 429",
        Some("Llama"),
    )
    .unwrap();
    assert_eq!(failure.status_code, 429);
    assert_eq!(
        failure.provider,
        crate::model_config::provider_full_name("groq")
    );
    assert_eq!(super::HttpFailure::detect("connection reset", None), None);
}

#[test]
fn every_ui_language_fills_every_http_message() {
    for lang in ["en", "vi", "ko"] {
        let text = crate::gui::locale::LocaleText::get(lang).overlay;
        for code in [400, 401, 402, 403, 404, 418, 429, 500, 502, 503, 504] {
            for model_name in [Some("Model-X"), None] {
                let failure = super::HttpFailure {
                    status_code: code,
                    provider: "Provider-Y".to_string(),
                    model_name,
                };
                let message = failure.message(&text);
                assert!(!message.contains('{'), "{lang} {code}: {message}");
                assert!(message.contains(&code.to_string()), "{lang}: {message}");
                assert!(message.contains("Provider-Y"), "{lang}: {message}");
                if code == 404 {
                    assert!(
                        message.contains(model_name.unwrap_or(text.chain_error_unnamed_model)),
                        "{lang}: {message}"
                    );
                }
            }
        }
        let generic = super::get_error_message("socket closed", lang, None);
        assert!(generic.contains("socket closed") && !generic.contains('{'));
        let retry = super::retry_message(lang, "Model-X");
        assert!(retry.contains("Model-X") && !retry.contains('{'));
    }
}

#[test]
fn ui_languages_without_a_locale_table_read_english() {
    for lang in ["ja", "zh"] {
        assert_eq!(
            super::get_error_message("status code 503 from api.openai.com", lang, None),
            super::get_error_message("status code 503 from api.openai.com", "en", None)
        );
    }
}
