//! Generated quality-retry fallbacks: a model profile may name, by model ID,
//! the model that one suspicious transcript chunk is retried on.

use super::{manifest_array, manifest_object, manifest_string, rust_string};

pub(super) fn push_quality_retry_models(manifest: &serde_json::Value, lines: &mut Vec<String>) {
    lines.push("#[cfg(feature = \"recorder-worker\")]".to_string());
    lines.push(
        "pub fn generated_quality_retry_model_id(provider: &str, api_model: &str) -> Option<&'static str> {"
            .to_string(),
    );
    lines.push("    match (provider, api_model) {".to_string());
    for (endpoint, profile) in manifest_object(manifest, "model_profiles") {
        let Some(retry_id) = profile.get("quality_retry_model_id") else {
            continue;
        };
        let retry_id = retry_id
            .as_str()
            .unwrap_or_else(|| panic!("quality_retry_model_id for {endpoint:?} must be a string"));
        let (provider, api_model) = endpoint.split_once(':').expect("provider endpoint key");
        validate_retry_target(manifest, endpoint, provider, api_model, retry_id);
        lines.push(format!(
            "        ({}, {}) => Some({}),",
            rust_string(provider),
            rust_string(api_model),
            rust_string(retry_id)
        ));
    }
    lines.push("        _ => None,".to_string());
    lines.push("    }".to_string());
    lines.push("}".to_string());
    lines.push(String::new());
}

/// The fallback must be an enabled model of the same provider and a different
/// API model, or the retry would fail or repeat the same request.
fn validate_retry_target(
    manifest: &serde_json::Value,
    endpoint: &str,
    provider: &str,
    api_model: &str,
    retry_id: &str,
) {
    let target = manifest_array(manifest, "models")
        .iter()
        .filter_map(serde_json::Value::as_object)
        .find(|model| manifest_string(model, "id") == retry_id)
        .unwrap_or_else(|| panic!("quality_retry_model_id for {endpoint:?} is unknown"));
    assert_eq!(
        target.get("enabled").and_then(serde_json::Value::as_bool),
        Some(true),
        "quality_retry_model_id for {endpoint:?} must name an enabled model"
    );
    assert_eq!(
        manifest_string(target, "provider"),
        provider,
        "quality_retry_model_id for {endpoint:?} must stay on the same provider"
    );
    assert_ne!(
        manifest_string(target, "full_name"),
        api_model,
        "quality_retry_model_id for {endpoint:?} must name a different API model"
    );
}
