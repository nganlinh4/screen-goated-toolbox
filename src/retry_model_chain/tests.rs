use super::cooldown::{
    MODEL_TIMEOUT_COOLDOWN, claim_model_attempt_at, model_cooldown_skip_reason_at,
    rate_limit_error, record_model_failure_at, unavailable_model_error,
};
use super::{
    DEFAULT_TEXT_OUTPUT_TOKENS, DEFAULT_VISION_OUTPUT_TOKENS, INTERACTIVE_REQUEST_BYTES_PER_SECOND,
    INTERACTIVE_STARTUP_ALLOWANCE_MS, InteractiveRequestWorkload,
    MAX_INTERACTIVE_REQUEST_ALLOWANCE_MS, MAX_INTERACTIVE_TIMEOUT_MS,
    MIN_INTERACTIVE_OUTPUT_TOKENS_PER_SECOND, MIN_INTERACTIVE_TIMEOUT_MS, RetryChainKind,
    UNBENCHMARKED_FEED_QUALITY_TIER, interactive_request_timeout, preflight_skip_reason,
    record_model_success, release_model_probe, resolve_next_configured_model,
    resolve_next_retry_model, resolve_unavailable_pinned_model, workload_derived_timeout,
};
use crate::config::Config;
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[test]
fn unary_timeout_uses_structural_workload_and_safety_bounds() {
    assert_eq!(workload_derived_timeout(0, 1), Duration::from_secs(60));
    assert_eq!(
        workload_derived_timeout(1_000_000, 160),
        Duration::from_secs(102)
    );
    assert_eq!(
        workload_derived_timeout(u64::MAX, u64::MAX),
        Duration::from_secs(900)
    );
}

#[test]
fn interactive_deadline_constants_match_mobile_parity_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../parity-fixtures/preset-system/retry-runtime.json"
    ))
    .unwrap();
    let unary = &fixture["interactive_deadlines"]["non_streaming"];
    let streaming = &fixture["interactive_deadlines"]["streaming"];

    assert_eq!(streaming["response_start_timeout_ms"], 120_000);
    assert_eq!(streaming["progress_idle_timeout_ms"], 120_000);
    assert!(streaming["whole_call_timeout_ms"].is_null());
    assert_eq!(
        unary["startup_allowance_ms"],
        INTERACTIVE_STARTUP_ALLOWANCE_MS
    );
    assert_eq!(
        unary["request_bytes_per_allowance_second"],
        INTERACTIVE_REQUEST_BYTES_PER_SECOND
    );
    assert_eq!(
        unary["maximum_request_allowance_ms"],
        MAX_INTERACTIVE_REQUEST_ALLOWANCE_MS
    );
    assert_eq!(
        unary["minimum_output_tokens_per_second"],
        MIN_INTERACTIVE_OUTPUT_TOKENS_PER_SECOND
    );
    assert_eq!(
        unary["default_text_output_tokens"],
        DEFAULT_TEXT_OUTPUT_TOKENS
    );
    assert_eq!(
        unary["default_vision_output_tokens"],
        DEFAULT_VISION_OUTPUT_TOKENS
    );
    assert_eq!(unary["minimum_hard_timeout_ms"], MIN_INTERACTIVE_TIMEOUT_MS);
    assert_eq!(unary["maximum_hard_timeout_ms"], MAX_INTERACTIVE_TIMEOUT_MS);
}

#[test]
fn live_corpus_passes_enter_as_capable_fallbacks_pending_offline_benchmark() {
    assert_eq!(UNBENCHMARKED_FEED_QUALITY_TIER, 4);
}

#[test]
fn streaming_requests_do_not_receive_a_total_response_deadline() {
    let config = Config::default();
    assert_eq!(
        interactive_request_timeout(
            "google-gemini-3-5-flash-lite-vision",
            &config,
            false,
            InteractiveRequestWorkload::default(),
        ),
        Some(Duration::from_secs(62))
    );
    assert_eq!(
        interactive_request_timeout(
            "google-gemini-3-5-flash-lite-vision",
            &config,
            true,
            InteractiveRequestWorkload::default(),
        ),
        None
    );
}

#[test]
fn two_timeout_failures_open_the_model_circuit() {
    let model_id = "test-timeout-threshold-vision";
    let started = Instant::now();
    record_model_success(model_id);

    record_model_failure_at(model_id, "transport error: timeout", started);
    assert_eq!(
        model_cooldown_skip_reason_at(model_id, started + Duration::from_secs(1)),
        None
    );

    record_model_failure_at(
        model_id,
        "request timed out",
        started + Duration::from_secs(2),
    );
    let reason = model_cooldown_skip_reason_at(model_id, started + Duration::from_secs(3))
        .expect("the second timeout should open the circuit");
    assert!(reason.starts_with("MODEL_TIMEOUT_COOLDOWN:"));
    record_model_success(model_id);
}

#[test]
fn success_resets_timeout_failures() {
    let model_id = "test-timeout-success-reset-text";
    let started = Instant::now();
    record_model_success(model_id);

    record_model_failure_at(model_id, "timeout", started);
    record_model_success(model_id);
    record_model_failure_at(model_id, "timeout", started + Duration::from_secs(1));

    assert_eq!(
        model_cooldown_skip_reason_at(model_id, started + Duration::from_secs(2)),
        None
    );
    record_model_success(model_id);
}

#[test]
fn open_circuit_does_not_extend_and_allows_only_one_probe() {
    let model_id = "test-timeout-half-open-vision";
    let started = Instant::now();
    record_model_success(model_id);
    record_model_failure_at(model_id, "timeout", started);
    record_model_failure_at(model_id, "timeout", started + Duration::from_secs(1));
    record_model_failure_at(model_id, "timeout", started + Duration::from_secs(60));

    let expiry = started + Duration::from_secs(1) + MODEL_TIMEOUT_COOLDOWN;
    assert_eq!(model_cooldown_skip_reason_at(model_id, expiry), None);
    assert_eq!(claim_model_attempt_at(model_id, expiry), None);
    assert_eq!(
        claim_model_attempt_at(model_id, expiry),
        Some(format!("MODEL_COOLDOWN_PROBE_IN_FLIGHT:{model_id}"))
    );

    release_model_probe(model_id);
    assert_eq!(claim_model_attempt_at(model_id, expiry), None);
    record_model_success(model_id);
}

#[test]
fn rate_limit_still_opens_its_existing_cooldown_immediately() {
    let model_id = "test-rate-limit-cooldown-text";
    let started = Instant::now();
    record_model_success(model_id);
    record_model_failure_at(model_id, "HTTP 429: quota exceeded", started);

    let reason = model_cooldown_skip_reason_at(model_id, started + Duration::from_secs(1))
        .expect("rate limits should open a cooldown immediately");
    assert!(reason.starts_with("MODEL_RATE_LIMIT_COOLDOWN:"));
    record_model_success(model_id);
}

#[test]
fn unavailable_model_opens_a_cooldown_immediately() {
    let model_id = "test-unavailable-model-text";
    let started = Instant::now();
    record_model_success(model_id);

    let error = "API HTTP 404: Model example is archived and unavailable";
    assert!(unavailable_model_error(error));
    record_model_failure_at(model_id, error, started);

    let reason = model_cooldown_skip_reason_at(model_id, started + Duration::from_secs(1))
        .expect("an unavailable model should open its circuit");
    assert!(reason.starts_with("MODEL_UNAVAILABLE_COOLDOWN:"));
    record_model_success(model_id);
}

#[test]
fn unrelated_not_found_errors_do_not_disable_a_model() {
    assert!(!unavailable_model_error("API HTTP 404: file not found"));
}

#[test]
fn skips_disabled_provider_in_preflight() {
    let config = Config {
        use_gemini: false,
        ..Default::default()
    };

    let reason = preflight_skip_reason(
        "google-gemini-3-1-flash-lite-vision",
        "google",
        &config,
        &HashSet::new(),
        None,
    );

    assert_eq!(reason.as_deref(), Some("PROVIDER_DISABLED:google"));
}

#[test]
fn distinguishes_rate_limits_from_transient_server_errors() {
    assert!(rate_limit_error("vision API HTTP 429: quota exceeded"));
    assert!(!rate_limit_error("vision API HTTP 503"));
}

#[test]
fn search_capable_retry_skips_incompatible_priority_candidates() {
    let config = Config {
        api_key: "test-groq-key".to_string(),
        gemini_api_key: "test-gemini-key".to_string(),
        ..Default::default()
    };
    let failed = vec!["google-gemini-3-5-flash-lite-vision".to_string()];

    let next = resolve_next_retry_model(
        "google-gemini-3-5-flash-lite-vision",
        &failed,
        &HashSet::new(),
        RetryChainKind::ImageToText,
        &config,
    )
    .expect("image chain should produce a next model");

    assert_eq!(next.id, "google-gemini-3-5-flash-vision");
    assert!(crate::model_config::model_supports_search_by_id_with_custom(&next.id, &[]));
    assert_ne!(next.id, "google-gemma-4-31b-vision");
}

#[test]
fn configured_retry_does_not_escape_the_priority_chain() {
    let mut config = Config {
        api_key: "test-groq-key".to_string(),
        ..Default::default()
    };
    config.model_priority_chains.text_to_text = vec!["missing-model".to_string()];

    let next = resolve_next_configured_model(
        "",
        &[],
        &HashSet::new(),
        RetryChainKind::TextToText,
        &config,
    );

    assert!(next.is_none());
}

#[test]
fn an_unavailable_pin_silently_enters_the_configured_priority_chain() {
    let mut config = Config {
        api_key: "test-groq-key".to_string(),
        gemini_api_key: "test-gemini-key".to_string(),
        ..Default::default()
    };
    config.model_priority_chains.text_to_text = vec![
        "missing-model".to_string(),
        "groq-qwen-3-6-27b-text".to_string(),
    ];

    let fallback = resolve_unavailable_pinned_model("text", "missing-model", &config)
        .expect("the next usable priority model should be selected");

    assert_eq!(fallback.id, "groq-qwen-3-6-27b-text");
    assert!(resolve_unavailable_pinned_model("audio", "missing-model", &config).is_none());
}

#[test]
fn ordinary_text_auto_retry_excludes_search_tool_endpoints() {
    let mut config = Config {
        api_key: "test-groq-key".to_string(),
        ..Default::default()
    };
    config.model_priority_chains.text_to_text.clear();
    let failed = crate::model_config::get_all_models_with_custom(&[])
        .into_iter()
        .filter(|model| {
            model.model_type == crate::model_config::ModelType::Text
                && !model.search_tool_enabled_by_default
                && model.id != "groq-qwen-3-6-27b-text"
        })
        .map(|model| model.id)
        .collect::<Vec<_>>();

    let next = resolve_next_retry_model(
        "groq-qwen-3-6-27b-text",
        &failed,
        &HashSet::new(),
        RetryChainKind::TextToText,
        &config,
    );

    assert!(next.is_none());
}

#[test]
fn a_reported_rate_limit_window_replaces_the_fixed_cooldown() {
    use crate::retry_model_chain::cooldown::{parse_duration_seconds, reported_cooldown};

    // Groq's real 429 body and header shapes.
    assert_eq!(
        reported_cooldown(
            "Groq vision API HTTP 429: Rate limit reached ... on tokens per minute (TPM):              Limit 8000, Used 7174, Requested 2452. Please try again in 22.012s"
        ),
        Some(Duration::from_secs_f64(22.012))
    );
    assert_eq!(
        reported_cooldown("HTTP 429 retry-after: 22"),
        Some(Duration::from_secs(22))
    );
    // Gemini reports the same fact in its body instead of a header.
    assert_eq!(
        reported_cooldown(
            "HTTP 429: You exceeded your current quota ...              * Quota exceeded for metric: generate_content_free_tier_requests, limit: 0              Please retry in 32.814072061s."
        ),
        Some(Duration::from_secs_f64(32.814072061))
    );

    // A real per-day exhaustion captured from the app on 2026-08-19. The old
    // flat 300s would have re-probed a model that could not work for 19 minutes.
    assert_eq!(
        reported_cooldown(
            "Groq vision API HTTP 429: Rate limit reached for model `qwen/qwen3.6-27b`              in organization `org_x` service tier `on_demand` on tokens per day (TPD):              Limit 200000, Used 199568, Requested 3048. Please try again in 18m50.112s.              Need more tokens? Upgrade to Dev Tier today at https://console.groq.com/settings/billing"
        ),
        Some(Duration::from_secs_f64(18.0 * 60.0 + 50.112))
    );

    // An exhausted daily quota backs off far past the old five-minute constant.
    assert_eq!(
        reported_cooldown("429 rate limit, please try again in 2h15m"),
        Some(Duration::from_secs(2 * 3600 + 15 * 60))
    );

    // Units, including the millisecond form that must not be read as minutes.
    assert_eq!(parse_duration_seconds("1m30s"), Some(90.0));
    assert_eq!(parse_duration_seconds("2m"), Some(120.0));
    assert_eq!(parse_duration_seconds("22"), Some(22.0));
    assert_eq!(parse_duration_seconds("500ms"), Some(0.5));
    assert_eq!(parse_duration_seconds("1m500ms"), Some(60.5));

    // No hint at all keeps today's behaviour.
    assert_eq!(reported_cooldown("HTTP 429: rate limit reached"), None);

    // Clamped so a malformed hint cannot bench a model for ever, or not at all.
    assert_eq!(
        reported_cooldown("429 try again in 0.2s"),
        Some(Duration::from_secs(5))
    );
    assert_eq!(
        reported_cooldown("429 try again in 99h"),
        Some(Duration::from_secs(6 * 60 * 60))
    );
}

#[test]
fn real_groq_rate_headers_populate_the_token_budget() {
    use crate::retry_model_chain::{budget, budget_key, record_token_budget};

    // Values captured verbatim from a live Groq vision response on 2026-08-19.
    let mut headers = ureq::http::HeaderMap::new();
    headers.insert("x-ratelimit-limit-tokens", "8000".parse().unwrap());
    headers.insert("x-ratelimit-remaining-tokens", "1000".parse().unwrap());
    headers.insert("x-ratelimit-reset-tokens", "22.012s".parse().unwrap());
    record_token_budget("groq", "qwen/qwen3.6-27b", &headers);

    let key = budget_key("groq", "qwen/qwen3.6-27b");
    // 1000 left cannot cover the cheapest measured call plus its output reserve.
    assert!(budget::shortfall(&key, budget::MEASURED_MIN_IMAGE_TOKENS + 512).is_some());
    // ... but it comfortably covers a hypothetical tiny one, so nothing is blocked.
    assert_eq!(budget::shortfall(&key, 500), None);

    // A response without the headers must leave admission untouched.
    record_token_budget("groq", "unmetered-model", &ureq::http::HeaderMap::new());
    assert_eq!(
        budget::shortfall(&budget_key("groq", "unmetered-model"), 9_000),
        None
    );
}

#[test]
fn disabling_adaptive_models_returns_the_authored_chain_exactly() {
    let mut config = Config::default();
    config.adaptive_model_priority.text_to_text = false;
    assert_eq!(
        RetryChainKind::TextToText.effective_chain(&config),
        config.model_priority_chains.text_to_text
    );
}

#[test]
fn user_authored_chain_is_not_runtime_truncated() {
    let mut config = Config::default();
    config.adaptive_model_priority.text_to_text = false;
    config.model_priority_chains.text_to_text =
        (0..30).map(|index| format!("user-model-{index}")).collect();

    assert_eq!(
        RetryChainKind::TextToText.effective_chain(&config),
        config.model_priority_chains.text_to_text
    );
}

#[test]
fn an_image_below_an_endpoints_floor_moves_to_the_next_model() {
    let config = Config::default();
    let leader = RetryChainKind::ImageToText.configured_chain(&config)[0].clone();
    let model = crate::model_config::get_model_by_id(&leader).expect("chain leader exists");
    let floor = crate::model_config::vision_request_profile(&model.provider, &model.full_name)
        .min_reliable_pixels
        .expect("the chain leader declares a reliable floor");

    // Any input below the catalog-declared capability boundary is routed onward.
    let reason = preflight_skip_reason(
        &leader,
        &model.provider,
        &config,
        &HashSet::new(),
        Some(59 * 15),
    )
    .expect("a crop far below the floor must be skipped");
    assert!(reason.starts_with("MODEL_INPUT_TOO_SMALL"), "{reason}");

    // Skipping is not a provider fault: the provider's other models, and this
    // model on larger images, must stay reachable.
    assert!(!crate::overlay::utils::should_block_retry_provider(&reason));

    // ... and the chain has somewhere to go, which is what makes the skip safe.
    // Checked structurally rather than by resolving a candidate, because
    // resolution needs credentials this test does not have.
    let chain = RetryChainKind::ImageToText.configured_chain(&config);
    assert!(
        chain.len() > 1,
        "skipping the leader on small input is only safe while something follows it"
    );
    assert!(
        chain[1..].iter().any(|id| {
            let model = crate::model_config::get_model_by_id(id).expect("chain member exists");
            crate::model_config::vision_request_profile(&model.provider, &model.full_name)
                .min_reliable_pixels
                .is_none_or(|other| other < floor)
        }),
        "at least one fallback must accept an input below the leader's floor"
    );

    // At and above the floor, and for a call carrying no image at all, the size
    // rule must not fire. Compared by reason rather than by `None`, because a
    // test environment without credentials is skipped for that instead.
    for pixels in [Some(floor), Some(floor + 1), None] {
        let reason =
            preflight_skip_reason(&leader, &model.provider, &config, &HashSet::new(), pixels);
        assert!(
            !reason
                .as_deref()
                .is_some_and(|reason| reason.starts_with("MODEL_INPUT_TOO_SMALL")),
            "{pixels:?} is not below the floor and must not be skipped for size"
        );
    }
}
