use super::*;

fn feed(models: Vec<FeedModel>) -> AvailabilityFeed {
    AvailabilityFeed {
        schema_version: 3,
        control_version: 1,
        availability_gate_version: 1,
        provider: "nvidia".to_string(),
        generated_at: "2026-08-20T07:33:26Z".to_string(),
        models,
    }
}

fn model(id: &str, p50: Option<u32>, success: f32, runs: u32) -> FeedModel {
    FeedModel {
        id: id.to_string(),
        control: Some(FeedControl::EffortNone),
        modality: Some("text".to_string()),
        p50_ms: p50,
        success_rate: success,
        runs,
    }
}

fn flat_rank(_: &str) -> CandidateRank {
    CandidateRank {
        quality_tier: 0,
        latency_ms: u32::MAX,
    }
}

fn representative_rank(id: &str) -> CandidateRank {
    match id {
        "local-second" => CandidateRank {
            quality_tier: 6,
            latency_ms: 500,
        },
        "nvidia/fast" => CandidateRank {
            quality_tier: 5,
            latency_ms: 300,
        },
        "nvidia/next" => CandidateRank {
            quality_tier: 4,
            latency_ms: 200,
        },
        "local-third" => CandidateRank {
            quality_tier: 4,
            latency_ms: 1_000,
        },
        _ => flat_rank(id),
    }
}

#[test]
fn quality_can_justify_some_latency_but_not_unbounded_slowness() {
    let fast_tier_five = CandidateRank {
        quality_tier: 5,
        latency_ms: 600,
    };
    let close_tier_six = CandidateRank {
        quality_tier: 6,
        latency_ms: 800,
    };
    let very_slow_tier_six = CandidateRank {
        quality_tier: 6,
        latency_ms: 10_000,
    };

    assert!(close_tier_six.outranks_or_ties(fast_tier_five));
    assert!(fast_tier_five.outranks_or_ties(very_slow_tier_six));
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
fn a_schema_one_feed_is_still_read_during_a_rollout() {
    let mut older = feed(Vec::new());
    older.schema_version = 1;
    assert!(validate(&older).is_ok());
}

#[test]
fn a_legacy_feed_cannot_offer_models_without_a_named_availability_gate() {
    let mut older = feed(vec![model("nvidia/stale", Some(10), 1.0, 5)]);
    older.schema_version = 1;
    older.availability_gate_version = 0;
    assert!(validate(&older).is_err());
}

#[test]
fn a_feed_from_obsolete_availability_semantics_is_refused_whole() {
    let mut weaker = feed(vec![model("nvidia/stale", Some(10), 1.0, 5)]);
    weaker.availability_gate_version = 0;
    assert!(validate(&weaker).is_err());
}

#[test]
fn preset_specific_schema_two_feed_is_refused_whole() {
    let mut preset_gated = feed(vec![model("nvidia/stale", Some(10), 1.0, 5)]);
    preset_gated.schema_version = 2;
    assert!(validate(&preset_gated).is_err());
}

#[test]
fn a_future_schema_is_refused_rather_than_guessed_at() {
    let mut newer = feed(Vec::new());
    newer.schema_version = 99;
    assert!(validate(&newer).is_err());
}

#[test]
fn a_future_reasoning_control_contract_is_refused_whole() {
    let mut newer = feed(Vec::new());
    newer.control_version = 2;
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
fn signed_publisher_owns_admission_and_clients_only_sort_its_offers() {
    let published = feed(vec![
        model("nvidia/slow", Some(900), 1.0, 12),
        model("nvidia/coinflip", Some(50), 0.5, 12),
        model("nvidia/fast", Some(300), 1.0, 12),
        model("nvidia/mostly", Some(400), 0.833, 12),
    ]);
    let ranked: Vec<&str> = ranked_models(&published)
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(
        ranked,
        vec![
            "nvidia/coinflip",
            "nvidia/fast",
            "nvidia/mostly",
            "nvidia/slow"
        ]
    );
}

#[test]
fn schema_three_cannot_offer_an_unmeasured_model() {
    let published = feed(vec![model("nvidia/unmeasured", None, 1.0, 0)]);
    assert!(validate(&published).is_err());
}

#[test]
fn feed_models_interleave_by_quality_then_speed_without_reordering_local_fallbacks() {
    let chain: Vec<String> = ["local-leader", "local-second", "local-third"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let offered = vec!["nvidia/fast".to_string(), "nvidia/next".to_string()];
    let merged = merge_into_chain(&chain, &offered, representative_rank);

    assert_eq!(merged[0], "local-leader");
    assert_eq!(
        merged,
        [
            "local-leader",
            "local-second",
            "nvidia/fast",
            "nvidia/next",
            "local-third"
        ]
    );
}

#[test]
fn one_live_feed_cannot_take_more_than_five_chain_slots() {
    let chain = vec!["local-leader".to_string(), "slow-local".to_string()];
    let offered: Vec<String> = (0..8).map(|index| format!("live-{index}")).collect();
    let merged = merge_into_chain(&chain, &offered, |id| CandidateRank {
        quality_tier: 4,
        latency_ms: if id.starts_with("live-") { 100 } else { 5_000 },
    });
    assert_eq!(
        merged.iter().filter(|id| id.starts_with("live-")).count(),
        5
    );
}

#[test]
fn adaptive_priority_policy_matches_the_shared_platform_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/model-catalog/presentation.json"
    )))
    .unwrap();
    let policy = &fixture["adaptive_priority"];
    assert_eq!(policy["default_enabled"], true);
    assert_eq!(policy["per_chain_toggle"], true);
    assert_eq!(policy["live_row_reorder_creates_pin"], true);
    assert_eq!(policy["live_row_delete_creates_exclusion"], true);
    assert_eq!(policy["non_live_edits_preserve_enabled"], true);
    assert_eq!(policy["row_overrides_preserve_enabled"], true);
    assert_eq!(policy["manual_edit_without_live_rows_disables_live"], true);
    assert_eq!(
        policy["dedicated_capabilities_excluded_from_generic_chains"],
        true
    );
    assert_eq!(policy["reset_clears_row_overrides"], true);
    assert_eq!(policy["refresh_reorders_only_while_enabled"], true);
    assert_eq!(policy["maximum_offers_per_chain"], 5);
    assert_eq!(policy["minimum_unpinned_live_position"], 3);
    assert_eq!(policy["live_rows_show_ranking_latency"], true);
    assert_eq!(policy["publisher_owns_offer_admission"], true);
    assert_eq!(
        policy["feed_absence_removes_nvidia_from_live_routing"],
        true
    );
    assert_eq!(policy["signed_feed_projects_all_nvidia_selectors"], true);
    assert_eq!(policy["reviewed_withdrawal_remains_quality_veto"], true);
    assert_eq!(policy["quality_latency_multiplier_per_tier"], 1.5);
    assert_eq!(policy["windows_live_feed"], true);
    assert_eq!(policy["android_live_feed"], true);
    assert_eq!(policy["signed_feed_schema"], 3);
    assert_eq!(policy["availability_gate_version"], 1);
    assert_eq!(
        policy["verified_cache_replace"],
        "atomic_with_same_directory_fallback"
    );
    let size = &fixture["priority_chain_size"];
    assert!(size["user_limit"].is_null());
    assert_eq!(size["prepared_image_default_target"], 10);
    assert_eq!(size["prepared_text_default_target"], 12);
}

#[test]
fn a_full_local_chain_still_gives_live_models_bounded_slots() {
    let chain: Vec<String> = (0..10).map(|index| format!("local-{index}")).collect();
    let offered = vec!["nvidia/fast".to_string(), "nvidia/next".to_string()];
    let mut merged = merge_into_chain(&chain, &offered, |id| CandidateRank {
        quality_tier: u8::from(id.starts_with("nvidia/")) + 1,
        latency_ms: 1,
    });
    merged.truncate(10);

    assert_eq!(merged[0], "local-0");
    assert_eq!(&merged[..2], &["local-0", "local-1"]);
    assert_eq!(&merged[2..4], &["nvidia/fast", "nvidia/next"]);
    assert_eq!(merged.len(), 10);
}

#[test]
fn a_model_already_in_the_chain_is_not_duplicated() {
    let chain: Vec<String> = ["local-leader", "nvidia/fast", "local-third"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let merged = merge_into_chain(&chain, &["nvidia/fast".to_string()], flat_rank);
    assert_eq!(merged.iter().filter(|id| *id == "nvidia/fast").count(), 1);
    assert_eq!(merged.len(), chain.len());
    assert_eq!(merged, ["local-leader", "local-third", "nvidia/fast"]);
}

#[test]
fn an_empty_offer_leaves_the_chain_exactly_as_it_was() {
    let chain: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    assert_eq!(merge_into_chain(&chain, &[], flat_rank), chain);
}

#[test]
fn a_single_entry_chain_is_not_extended_above_the_minimum_live_position() {
    let chain = vec!["only-local".to_string()];
    let merged = merge_into_chain(&chain, &["nvidia/fast".to_string()], flat_rank);
    assert_eq!(merged, ["only-local"]);
}

#[test]
fn enabling_live_reclaims_persisted_feed_rows_for_formula_ordering() {
    let chain: Vec<String> = ["local-leader", "nvidia/slow", "local-middle", "nvidia/fast"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let offered = vec!["nvidia/slow".to_string(), "nvidia/fast".to_string()];
    let merged = merge_into_chain(&chain, &offered, |id| CandidateRank {
        quality_tier: 4,
        latency_ms: match id {
            "nvidia/fast" => 100,
            "local-middle" => 500,
            "nvidia/slow" => 900,
            _ => 1,
        },
    });

    assert_eq!(
        merged,
        ["local-leader", "local-middle", "nvidia/fast", "nvidia/slow"]
    );
}

#[test]
fn pinned_live_rows_stay_in_the_authored_baseline_while_other_offers_refresh() {
    let chain: Vec<String> = ["leader", "nvidia/pinned", "local", "nvidia/old"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let offered = vec![
        "nvidia/pinned".to_string(),
        "nvidia/old".to_string(),
        "nvidia/new".to_string(),
    ];
    let merged = merge_into_chain_with_overrides(
        &chain,
        &offered,
        &["nvidia/pinned".to_string()],
        &[],
        |id| CandidateRank {
            quality_tier: 4,
            latency_ms: match id {
                "nvidia/new" => 50,
                "nvidia/old" => 100,
                "nvidia/pinned" => 10_000,
                _ => 500,
            },
        },
    );

    assert_eq!(
        merged,
        [
            "leader",
            "nvidia/pinned",
            "nvidia/new",
            "nvidia/old",
            "local"
        ]
    );
}

#[test]
fn excluded_live_rows_are_neither_kept_nor_reintroduced() {
    let chain = vec![
        "leader".to_string(),
        "local-second".to_string(),
        "nvidia/removed".to_string(),
    ];
    let offered = vec!["nvidia/removed".to_string(), "nvidia/other".to_string()];
    let merged = merge_into_chain_with_overrides(
        &chain,
        &offered,
        &[],
        &["nvidia/removed".to_string()],
        flat_rank,
    );

    assert!(!merged.iter().any(|id| id == "nvidia/removed"));
    assert!(merged.iter().any(|id| id == "nvidia/other"));
}

#[test]
fn an_offered_configured_head_remains_protected() {
    let chain: Vec<String> = ["nvidia/chosen", "local-next"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let offered = vec!["nvidia/chosen".to_string(), "nvidia/faster".to_string()];
    let merged = merge_into_chain(&chain, &offered, |id| CandidateRank {
        quality_tier: 4,
        latency_ms: if id == "nvidia/faster" { 1 } else { 1_000 },
    });

    assert_eq!(merged[0], "nvidia/chosen");
    assert_eq!(merged[1], "local-next");
    assert_eq!(merged[2], "nvidia/faster");
}

#[test]
fn an_empty_chain_is_left_alone_rather_than_seeded_remotely() {
    // With no local head to protect, accepting the feed would hand it position 0.
    assert!(merge_into_chain(&[], &["nvidia/fast".to_string()], flat_rank).is_empty());
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
    assert!(SUPPORTED_SCHEMAS.contains(&feed.schema_version));
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

#[test]
fn a_discovered_id_is_derived_stably_and_keeps_its_vendor() {
    use super::store::discovered_id;

    // The vendor is part of the endpoint's identity: the same model is served by
    // more than one provider, and the two are not interchangeable.
    let first = discovered_id("nvidia", "openai/gpt-oss-120b");
    assert!(first.starts_with("nvidia-openai-gpt-oss-120b-"));
    // Derivation must be pure, or a pinned preset would drift between runs.
    assert_eq!(
        discovered_id("nvidia", "meta/llama-3.1-8b-instruct"),
        discovered_id("nvidia", "meta/llama-3.1-8b-instruct")
    );
    // Dots and slashes are not id characters; nothing else survives them.
    assert!(
        !discovered_id("nvidia", "meta/llama-3.1-8b").contains(['/', '.']),
        "an id must be safe to store in a preset"
    );
    assert_ne!(
        discovered_id("nvidia", "meta/a.b"),
        discovered_id("nvidia", "meta/a-b"),
        "normalization collisions need distinct stable suffixes"
    );
}

#[test]
fn a_dedicated_translator_cannot_enter_the_generic_text_catalog() {
    let endpoint = "vendor/translate-specialist";
    let mislabeled_legacy_feed = feed(vec![model(endpoint, Some(100), 1.0, 5)]);
    assert!(
        !crate::model_feed::store::discovered_models_from_feed(&mislabeled_legacy_feed)
            .iter()
            .any(|model| model.full_name == endpoint)
    );

    let mut dedicated_feed_model = model("vendor/dedicated", Some(100), 1.0, 5);
    dedicated_feed_model.modality = Some("translation".to_string());
    let explicit_capability_feed = feed(vec![dedicated_feed_model]);
    assert!(
        crate::model_feed::store::discovered_models_from_feed(&explicit_capability_feed).is_empty()
    );
}
