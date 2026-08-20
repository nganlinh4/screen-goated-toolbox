use super::*;

fn feed(models: Vec<FeedModel>) -> AvailabilityFeed {
    AvailabilityFeed {
        schema_version: SUPPORTED_SCHEMA,
        provider: "nvidia".to_string(),
        generated_at: "2026-08-20T07:33:26Z".to_string(),
        models,
    }
}

fn model(id: &str, p50: Option<u32>, success: f32, runs: u32) -> FeedModel {
    FeedModel {
        id: id.to_string(),
        control: Some("effort-none".to_string()),
        p50_ms: p50,
        success_rate: success,
        runs,
    }
}

#[test]
fn an_unsigned_or_tampered_feed_is_refused() {
    let payload = br#"{"schemaVersion":1,"provider":"nvidia","generatedAt":"x","models":[]}"#;
    // No valid signature exists for this payload, so nothing may be returned.
    assert!(parse_verified(payload, &[0u8; 64]).is_err());
    // A malformed signature must fail on shape rather than being ignored.
    assert!(parse_verified(payload, b"short").is_err());
}

#[test]
fn a_feed_from_an_unexpected_provider_is_rejected_whole() {
    let mut other = feed(vec![model("someone/else", Some(10), 1.0, 5)]);
    other.provider = "unexpected".to_string();
    let error = validate(&other).expect_err("provider must be checked");
    assert!(error.to_string().contains("unexpected provider"), "{error}");
}

#[test]
fn a_future_schema_is_refused_rather_than_guessed_at() {
    let mut newer = feed(Vec::new());
    newer.schema_version = SUPPORTED_SCHEMA + 1;
    assert!(validate(&newer).is_err());
}

#[test]
fn model_ids_must_be_provider_qualified() {
    let bare = feed(vec![model("nemotron-3-nano-30b-a3b", Some(10), 1.0, 5)]);
    let error = validate(&bare).expect_err("a bare id could route anywhere");
    assert!(error.to_string().contains("provider-qualified"), "{error}");
}

#[test]
fn an_out_of_range_success_rate_is_rejected() {
    let impossible = feed(vec![model("nvidia/x", Some(10), 1.5, 5)]);
    assert!(validate(&impossible).is_err());
}

#[test]
fn only_fully_successful_models_are_offered_and_they_sort_by_latency() {
    let published = feed(vec![
        model("nvidia/slow", Some(900), 1.0, 12),
        model("nvidia/flaky", Some(50), 0.8, 12),
        model("nvidia/fast", Some(300), 1.0, 12),
        model("nvidia/unmeasured", None, 1.0, 0),
    ]);
    let ranked: Vec<&str> = ranked_models(&published)
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(ranked, vec!["nvidia/fast", "nvidia/slow"]);
}

#[test]
fn the_chain_head_is_never_given_to_the_feed() {
    let chain: Vec<String> = ["local-leader", "local-second", "local-third"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let offered = vec!["nvidia/fast".to_string(), "nvidia/next".to_string()];
    let merged = merge_into_chain(&chain, &offered);

    assert_eq!(merged[0], "local-leader", "position 0 must stay local");
    assert_eq!(merged[1], "nvidia/fast");
    assert_eq!(merged[2], "nvidia/next");
    // Nothing local is lost, and relative order among locals is preserved.
    assert_eq!(&merged[3..], &["local-second", "local-third"]);
}

#[test]
fn a_model_already_in_the_chain_is_not_duplicated() {
    let chain: Vec<String> = ["local-leader", "nvidia/fast", "local-third"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let merged = merge_into_chain(&chain, &["nvidia/fast".to_string()]);
    assert_eq!(merged.iter().filter(|id| *id == "nvidia/fast").count(), 1);
    assert_eq!(merged.len(), chain.len());
    assert_eq!(merged[0], "local-leader");
}

#[test]
fn an_empty_offer_leaves_the_chain_exactly_as_it_was() {
    let chain: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    assert_eq!(merge_into_chain(&chain, &[]), chain);
}

#[test]
fn a_single_entry_chain_keeps_its_only_member_first() {
    let chain = vec!["only-local".to_string()];
    let merged = merge_into_chain(&chain, &["nvidia/fast".to_string()]);
    assert_eq!(merged[0], "only-local");
    assert_eq!(merged[1], "nvidia/fast");
}

#[test]
fn an_empty_chain_is_left_alone_rather_than_seeded_remotely() {
    // With no local head to protect, accepting the feed would hand it position 0.
    assert!(merge_into_chain(&[], &["nvidia/fast".to_string()]).is_empty());
}

/// The cross-language contract: a signature produced by the Python signer must
/// verify here. These are the bytes the workflow actually published, so an
/// encoding drift between the two sides fails this test rather than silently
/// disabling the feed in the field.
#[test]
fn the_real_published_feed_verifies() {
    let payload = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/model-feed/published-feed.json"
    ));
    let signature = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/model-feed/published-feed.json.sig"
    ));
    let feed = parse_verified(payload, signature).expect("published feed must verify");
    assert_eq!(feed.provider, "nvidia");
    assert_eq!(feed.schema_version, SUPPORTED_SCHEMA);
}

#[test]
fn a_single_flipped_byte_invalidates_the_published_feed() {
    let mut payload = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/model-feed/published-feed.json"
    ))
    .to_vec();
    let signature = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/model-feed/published-feed.json.sig"
    ));
    payload[10] ^= 0x01;
    assert!(parse_verified(&payload, signature).is_err());
}
