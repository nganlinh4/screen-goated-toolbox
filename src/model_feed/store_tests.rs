use super::*;

#[test]
fn an_unknown_healthy_model_routes_through_its_discovered_identity() {
    assert_eq!(
        catalog_id_for("nvidia", "nvidia/not-in-this-build", ModelType::Text),
        Some(discovered_id("nvidia", "nvidia/not-in-this-build"))
    );
}

#[test]
fn discovered_names_are_short_provider_marked_and_deterministic() {
    assert_eq!(
        crate::model_config::compact_provider_endpoint_name(
            "nvidia",
            "nvidia/nemotron-mini-4b-instruct"
        ),
        "N nm4i"
    );
}

#[test]
fn a_known_model_resolves_to_its_catalog_row() {
    // Wired in the same change that added the feed, so this also guards the
    // pairing between the published name and the catalog row.
    assert_eq!(
        catalog_id_for(
            "nvidia",
            "nvidia/nemotron-3.5-lightning-30b-a3b",
            ModelType::Text,
        )
        .as_deref(),
        Some("nvidia-nemotron-3-5-lightning-text")
    );
}

#[test]
fn a_reviewed_withdrawal_cannot_return_through_the_signed_feed() {
    for (endpoint, modality, model_type, control) in [
        (
            "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning",
            "vision",
            ModelType::Vision,
            super::super::FeedControl::Plain,
        ),
        (
            "stepfun-ai/step-5-preview",
            "text",
            ModelType::Text,
            super::super::FeedControl::EffortNone,
        ),
    ] {
        let feed = AvailabilityFeed {
            schema_version: 3,
            control_version: 1,
            availability_gate_version: 1,
            provider: "nvidia".to_string(),
            generated_at: "2026-10-18T00:00:00Z".to_string(),
            models: vec![crate::model_feed::FeedModel {
                id: endpoint.to_string(),
                control: Some(control),
                modality: Some(modality.to_string()),
                p50_ms: Some(100),
                success_rate: 1.0,
                runs: 100,
            }],
        };

        assert!(!endpoint_is_offered(&feed, model_type, endpoint));
        assert_eq!(catalog_id_for("nvidia", endpoint, model_type), None);
        assert!(discovered_models_from_feed(&feed).is_empty());
    }
}

#[test]
fn a_feed_modality_cannot_reuse_another_modalitys_catalog_id() {
    let endpoint = "openai/gpt-oss-120b";
    assert_eq!(
        catalog_id_for("nvidia", endpoint, ModelType::Text).as_deref(),
        Some("nvidia-gpt-oss-120b-text")
    );
    assert_eq!(
        catalog_id_for("nvidia", endpoint, ModelType::Vision),
        Some(discovered_id("nvidia", endpoint))
    );

    let feed = AvailabilityFeed {
        schema_version: 3,
        control_version: 1,
        availability_gate_version: 1,
        provider: "nvidia".to_string(),
        generated_at: "2026-08-28T00:00:00Z".to_string(),
        models: vec![crate::model_feed::FeedModel {
            id: endpoint.to_string(),
            control: Some(super::super::FeedControl::Plain),
            modality: Some("vision".to_string()),
            p50_ms: Some(1_063),
            success_rate: 0.33,
            runs: 24,
        }],
    };
    let discovered = discovered_models_from_feed(&feed);
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].model_type, ModelType::Vision);
    assert_eq!(discovered[0].name_vi, "N go1");
    assert_eq!(discovered[0].typical_latency_ms, Some(1_063));
}

#[test]
fn a_provider_the_user_has_not_enabled_offers_nothing() {
    let config = crate::config::Config {
        use_nvidia: false,
        ..Default::default()
    };
    assert!(offered_models(&config, ModelType::Text).is_empty());
}

#[test]
fn signed_offer_removes_stale_nvidia_rows_without_touching_other_providers() {
    let feed = AvailabilityFeed {
        schema_version: 3,
        control_version: 1,
        availability_gate_version: 1,
        provider: "nvidia".to_string(),
        generated_at: "2026-08-28T00:00:00Z".to_string(),
        models: vec![crate::model_feed::FeedModel {
            id: "nvidia/nemotron-3.5-lightning-30b-a3b".to_string(),
            control: Some(super::super::FeedControl::Plain),
            modality: Some("text".to_string()),
            p50_ms: Some(300),
            success_rate: 0.5,
            runs: 3,
        }],
    };
    let configured = vec![
        "groq-qwen-3-8-27b-text".to_string(),
        "nvidia-nemotron-3-5-lightning-text".to_string(),
        "nvidia-nemotron-3-super-120b-text".to_string(),
    ];

    assert_eq!(
        reconcile_configured_chain_from_feed(&feed, ModelType::Text, &configured, &[]),
        [
            "groq-qwen-3-8-27b-text",
            "nvidia-nemotron-3-5-lightning-text"
        ]
    );

    let mut selectors = crate::model_config::get_all_models().to_vec();
    project_provider_inventory(&mut selectors, &feed);
    assert!(selectors.iter().any(|model| model.provider == "groq"));
    assert!(
        selectors
            .iter()
            .any(|model| model.id == "nvidia-nemotron-3-5-lightning-text")
    );
    assert!(
        selectors
            .iter()
            .all(|model| model.id != "nvidia-nemotron-3-super-120b-text")
    );
}

#[test]
fn the_feed_in_use_is_reported_with_its_publication_time() {
    let payload = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/model-feed/published-feed.json"
    ));
    let signature = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/model-feed/published-feed.json.sig"
    ));
    let feed = parse_verified(payload, signature).unwrap();
    let line = feed_in_use_line(&feed);
    assert!(line.contains("generated 2026-08-20T07:33:26Z"), "{line}");
    assert!(line.starts_with("[Model feed] using nvidia feed"), "{line}");
}

fn feed_generated_at(generated_at: &str) -> AvailabilityFeed {
    AvailabilityFeed {
        schema_version: 3,
        control_version: 1,
        availability_gate_version: 1,
        provider: "nvidia".to_string(),
        generated_at: generated_at.to_string(),
        models: Vec::new(),
    }
}

fn instant(text: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&chrono::Utc)
}

#[test]
fn a_feed_stops_steering_routing_once_publication_has_stopped() {
    let feed = feed_generated_at("2026-10-18T00:00:00Z");
    assert!(feed.is_current_at(instant("2026-10-18T02:00:00Z")));
    assert!(feed.is_current_at(instant("2026-10-19T00:00:00Z")));
    assert!(!feed.is_current_at(instant("2026-10-19T00:00:01Z")));
    assert!(!feed_generated_at("not a time").is_current_at(instant("2026-10-18T00:00:00Z")));
}

#[test]
fn an_unparseable_publication_time_rejects_the_whole_feed() {
    let error = super::super::validate(&feed_generated_at("x")).expect_err("time must parse");
    assert!(error.to_string().contains("RFC 3339"), "{error}");
    assert!(super::super::validate(&feed_generated_at("2026-10-18T00:00:00Z")).is_ok());
}

#[test]
fn refresh_refuses_an_expired_or_replayed_publication() {
    let now = instant("2026-10-18T03:00:00Z");
    let cached = feed_generated_at("2026-10-18T02:00:00Z");
    let replay = feed_generated_at("2026-10-18T00:00:00Z");
    let newer = feed_generated_at("2026-10-18T02:17:00Z");
    let expired = feed_generated_at("2026-10-16T00:00:00Z");

    assert!(accept_refresh(&replay, Some(&cached), now).is_err());
    assert!(accept_refresh(&expired, None, now).is_err());
    assert!(accept_refresh(&newer, Some(&cached), now).is_ok());
    assert!(accept_refresh(&cached, Some(&cached), now).is_ok());
    assert!(accept_refresh(&replay, None, now).is_ok());
}

#[test]
fn discovery_is_projected_once_per_loaded_feed() {
    let memo = DiscoveredMemo(std::sync::Mutex::new(None));
    let mut published = feed_generated_at("2026-08-28T00:00:00Z");
    published.models.push(crate::model_feed::FeedModel {
        id: "nvidia/not-in-this-build".to_string(),
        control: Some(super::super::FeedControl::Plain),
        modality: Some("text".to_string()),
        p50_ms: Some(900),
        success_rate: 1.0,
        runs: 24,
    });
    let feed = Arc::new(published.clone());
    let first = memo.for_feed(&feed);
    assert_eq!(first.len(), 1);
    assert!(Arc::ptr_eq(&first, &memo.for_feed(&feed)));

    let refreshed = Arc::new(published);
    assert!(!Arc::ptr_eq(&first, &memo.for_feed(&refreshed)));
}
