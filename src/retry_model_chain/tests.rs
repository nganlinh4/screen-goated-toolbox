use super::{
    MODEL_TIMEOUT_COOLDOWN, RetryChainKind, benchmark_derived_timeout, claim_model_attempt_at,
    interactive_request_timeout, model_cooldown_skip_reason_at, preflight_skip_reason,
    rate_limit_error, record_model_failure_at, record_model_success, release_model_probe,
    resolve_next_configured_model, resolve_next_retry_model,
};
use crate::config::Config;
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[test]
fn benchmark_timeout_is_multiplied_and_bounded() {
    assert_eq!(
        benchmark_derived_timeout(Some(200)),
        Duration::from_secs(10)
    );
    assert_eq!(
        benchmark_derived_timeout(Some(1_414)),
        Duration::from_millis(14_140)
    );
    assert_eq!(
        benchmark_derived_timeout(Some(5_000)),
        Duration::from_secs(30)
    );
    assert_eq!(benchmark_derived_timeout(None), Duration::from_secs(30));
}

#[test]
fn streaming_requests_do_not_receive_a_total_response_deadline() {
    let config = Config::default();
    assert_eq!(
        interactive_request_timeout("google-gemini-3-6-flash-vision", &config, false),
        Some(Duration::from_millis(14_140))
    );
    assert_eq!(
        interactive_request_timeout("google-gemini-3-6-flash-vision", &config, true),
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

    assert_eq!(next.id, "google-gemini-3-1-flash-lite-vision");
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
