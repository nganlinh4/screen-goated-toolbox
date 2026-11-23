use super::*;

#[test]
fn real_groq_rate_headers_populate_the_token_budget() {
    use crate::retry_model_chain::{budget, budget_key, record_token_budget};

    // Nonzero reported quota must not be rejected based on guessed request costs.
    let mut headers = ureq::http::HeaderMap::new();
    headers.insert("x-ratelimit-limit-tokens", "8000".parse().unwrap());
    headers.insert("x-ratelimit-remaining-tokens", "1000".parse().unwrap());
    headers.insert("x-ratelimit-reset-tokens", "22.012s".parse().unwrap());
    record_token_budget("groq", "qwen/qwen3.8-27b", &headers);

    let key = budget_key("groq", "qwen/qwen3.8-27b");
    assert_eq!(budget::shortfall(&key, 1), None);
    let config = Config {
        api_key: "test-key".into(),
        ..Default::default()
    };
    for model in crate::model_config::get_all_models()
        .iter()
        .filter(|model| model.provider == "groq" && model.full_name == "qwen/qwen3.8-27b")
    {
        let reason = preflight_skip_reason(&model.id, &model.provider, &config, &HashSet::new());
        assert_eq!(reason, None);
    }

    // A response without the headers must leave admission untouched.
    record_token_budget("groq", "unmetered-model", &ureq::http::HeaderMap::new());
    assert_eq!(
        budget::shortfall(&budget_key("groq", "unmetered-model"), 9_000),
        None
    );
}

#[test]
fn invalid_reset_windows_do_not_record_a_token_budget() {
    use crate::retry_model_chain::{budget, budget_key, record_token_budget};

    for (index, reset) in ["99999999999999999999".to_string(), "9".repeat(400)]
        .into_iter()
        .enumerate()
    {
        let model = format!("invalid-reset-{index}");
        let mut headers = ureq::http::HeaderMap::new();
        headers.insert("x-ratelimit-limit-tokens", "8000".parse().unwrap());
        headers.insert("x-ratelimit-remaining-tokens", "0".parse().unwrap());
        headers.insert("x-ratelimit-reset-tokens", reset.parse().unwrap());
        record_token_budget("test", &model, &headers);
        assert_eq!(budget::shortfall(&budget_key("test", &model), 1), None);
    }
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
