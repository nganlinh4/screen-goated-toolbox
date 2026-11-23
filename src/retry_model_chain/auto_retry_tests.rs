use super::{
    RetryChainKind, candidate_rank, is_retry_candidate_compatible, preferred_candidate,
    resolve_next_retry_model,
};
use crate::config::Config;
use crate::model_config::{ModelType, get_all_models_with_custom};
use std::collections::HashSet;

fn auto_retry_pick(config: &Config) -> Option<crate::model_config::ModelConfig> {
    resolve_next_retry_model(
        "",
        &[],
        &HashSet::new(),
        RetryChainKind::TextToText,
        false,
        config,
    )
}

#[test]
fn auto_retry_skips_live_exclusions_and_prefers_the_best_ranked_candidate() {
    let mut config = Config {
        api_key: "test-groq-key".to_string(),
        ..Default::default()
    };
    config.model_priority_chains.text_to_text.clear();
    let first = auto_retry_pick(&config).expect("a compatible fallback exists");
    let offered = crate::model_feed::store::offered_models(&config, ModelType::Text);
    let best = get_all_models_with_custom(&[])
        .into_iter()
        .filter(|model| {
            is_retry_candidate_compatible(model, &ModelType::Text, false, &HashSet::new(), &config)
        })
        .map(|model| candidate_rank(Some(&model), &offered, &model.id).preference_key())
        .min()
        .unwrap();
    assert_eq!(
        candidate_rank(Some(&first), &offered, &first.id).preference_key(),
        best
    );

    config
        .adaptive_model_priority
        .text_to_text_overrides
        .exclude(&first.id);
    assert!(auto_retry_pick(&config).is_none_or(|model| model.id != first.id));
}

#[test]
fn auto_retry_weighs_quality_against_latency_like_the_adaptive_merge() {
    let template = get_all_models_with_custom(&[])
        .into_iter()
        .find(|model| model.model_type == ModelType::Text)
        .expect("a text catalog row");
    let mut strong = template.clone();
    strong.id = "strong-slightly-slower".into();
    strong.intelligence_tier = Some(6);
    strong.typical_latency_ms = Some(1_000);
    let mut weak = template;
    weak.id = "weak-slightly-faster".into();
    weak.intelligence_tier = Some(3);
    weak.typical_latency_ms = Some(900);
    let models = [weak, strong];

    let latency_only = models
        .iter()
        .min_by_key(|model| model.typical_latency_ms)
        .unwrap();
    assert_eq!(latency_only.id, "weak-slightly-faster");
    let picked = preferred_candidate(models.iter(), &models[0].provider, &[]).unwrap();
    assert_eq!(picked.id, "strong-slightly-slower");
}
