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
    let endpoint = "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning";
    let feed = AvailabilityFeed {
        schema_version: 3,
        control_version: 1,
        availability_gate_version: 1,
        provider: "nvidia".to_string(),
        generated_at: "2026-09-21T00:00:00Z".to_string(),
        models: vec![crate::model_feed::FeedModel {
            id: endpoint.to_string(),
            control: Some(super::super::FeedControl::Plain),
            modality: Some("vision".to_string()),
            p50_ms: Some(100),
            success_rate: 1.0,
            runs: 100,
        }],
    };

    assert!(!endpoint_is_offered(&feed, ModelType::Vision, endpoint));
    assert_eq!(catalog_id_for("nvidia", endpoint, ModelType::Vision), None);
    assert!(discovered_models_from_feed(&feed).is_empty());
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
