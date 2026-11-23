use std::collections::HashSet;
use std::time::Duration;

use super::{
    CredentialPool, Credentials, Pacer, dotenv_credential_slots, dotenv_value, select_models,
};
use crate::model_config::ModelType;
use crate::model_config::get_all_models;

fn empty_credentials() -> Credentials {
    Credentials {
        groq: CredentialPool::empty(),
        gemini: CredentialPool::empty(),
        openrouter: CredentialPool::empty(),
        nvidia: CredentialPool::empty(),
    }
}

#[test]
fn selection_excludes_dedicated_non_llm_services_from_general_suites() {
    let credentials = empty_credentials();
    let text_filter = HashSet::from(["google-gtx-translate-text".to_string()]);
    let text = select_models(ModelType::Text, Some(&text_filter), None, &credentials);
    assert!(text.is_empty());

    let vision_filter = HashSet::from(["qrserver-qr-scanner-vision".to_string()]);
    assert!(select_models(ModelType::Vision, Some(&vision_filter), None, &credentials).is_empty());
}

#[test]
fn groq_rotates_across_indexed_credentials_like_the_other_pools() {
    // Groq was the only provider read as a single key, so a second slot in
    // .env changed nothing. It now cycles like Gemini and OpenRouter.
    let pool = CredentialPool::from_values(vec!["a".to_string(), "b".to_string()]);
    assert_eq!(pool.next(), "a");
    assert_eq!(pool.next(), "b");
    assert_eq!(pool.next(), "a");

    // Only a Groq model consumes the rotated key.
    assert_eq!(Credentials::groq_key_for("groq", "rotated"), "rotated");
    assert_eq!(Credentials::groq_key_for("google", "rotated"), "");
}

#[test]
fn pacer_respects_provider_free_tier_rates() {
    let openrouter = get_all_models()
        .iter()
        .find(|model| model.provider == "openrouter")
        .unwrap();
    let default = Pacer {
        min_interval: Duration::from_millis(2_500),
        groq_credential_count: 1,
        last_call: Default::default(),
    };
    assert_eq!(
        default.interval_for(openrouter),
        Duration::from_millis(3_100)
    );
    let slower_override = Pacer {
        min_interval: Duration::from_millis(5_000),
        groq_credential_count: 1,
        last_call: Default::default(),
    };
    assert_eq!(
        slower_override.interval_for(openrouter),
        Duration::from_millis(5_000)
    );
}

#[test]
fn groq_vision_pacing_uses_independent_credential_capacity() {
    let vision = get_all_models()
        .iter()
        .find(|model| model.provider == "groq" && model.model_type == ModelType::Vision)
        .unwrap();
    let one_key = Pacer {
        min_interval: Duration::from_millis(2_500),
        groq_credential_count: 1,
        last_call: Default::default(),
    };
    assert_eq!(one_key.interval_for(vision), Duration::from_secs(31));
    let two_keys = Pacer {
        min_interval: Duration::from_millis(2_500),
        groq_credential_count: 2,
        last_call: Default::default(),
    };
    assert_eq!(two_keys.interval_for(vision), Duration::from_millis(15_500));
}

#[test]
fn pacer_shares_only_provider_wide_quota_scopes() {
    let pacer = Pacer {
        min_interval: Duration::ZERO,
        groq_credential_count: 1,
        last_call: Default::default(),
    };
    let mut openrouter = get_all_models()
        .iter()
        .filter(|model| model.provider == "openrouter");
    let openrouter_text = openrouter.next().unwrap();
    let openrouter_vision = openrouter.next().unwrap();
    assert_eq!(pacer.scope(openrouter_text), pacer.scope(openrouter_vision));

    let mut google = get_all_models()
        .iter()
        .filter(|model| model.provider == "google");
    let google_fast = google.next().unwrap();
    let google_quality = google.next().unwrap();
    assert_ne!(pacer.scope(google_fast), pacer.scope(google_quality));
}

#[test]
fn credential_pool_rotates_in_stable_round_robin_order() {
    let pool = CredentialPool::from_values(vec![
        "first".to_string(),
        "second".to_string(),
        "third".to_string(),
    ]);
    assert_eq!(pool.next(), "first");
    assert_eq!(pool.next(), "second");
    assert_eq!(pool.next(), "third");
    assert_eq!(pool.next(), "first");
}

#[test]
fn dotenv_parser_accepts_benchmark_slots_without_exposing_other_values() {
    let contents = "# local credentials\nGEMINI_API_KEY=primary\nexport GEMINI_API_KEY_2=\"second\"\nGEMINI_API_KEY_20=twentieth\nGEMINI_API_KEY_02=ignored\nOPENROUTER_API_KEY=other\n";
    assert_eq!(
        dotenv_value(contents, "GEMINI_API_KEY"),
        Some("primary".to_string())
    );
    assert_eq!(
        dotenv_value(contents, "GEMINI_API_KEY_2"),
        Some("second".to_string())
    );
    assert_eq!(dotenv_value(contents, "GEMINI_API_KEY_3"), None);
    assert_eq!(
        dotenv_value(contents, "GEMINI_API_KEY_20"),
        Some("twentieth".to_string())
    );
    assert_eq!(
        dotenv_credential_slots(contents, "GEMINI_API_KEY"),
        [2, 20].into_iter().collect()
    );
}
