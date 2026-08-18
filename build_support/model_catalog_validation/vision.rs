use std::collections::HashSet;

use super::{object, string};

pub(super) fn validate_request_profiles(
    manifest: &serde_json::Value,
    models: &[serde_json::Value],
    model_profiles: &serde_json::Map<String, serde_json::Value>,
) {
    let request_profiles = object(manifest, "vision_request_profiles");
    let expected: HashSet<String> = models
        .iter()
        .filter_map(serde_json::Value::as_object)
        .filter(|model| {
            model.get("enabled").and_then(serde_json::Value::as_bool) == Some(true)
                && string(model, "model_type") == "Vision"
                && matches!(string(model, "provider"), "google" | "groq" | "openrouter")
        })
        .map(|model| {
            format!(
                "{}:{}",
                string(model, "provider"),
                string(model, "full_name")
            )
        })
        .collect();
    let actual: HashSet<String> = request_profiles.keys().cloned().collect();
    assert_eq!(
        actual, expected,
        "vision_request_profiles must cover every enabled ordinary LLM vision endpoint exactly"
    );

    for (profile_key, value) in request_profiles {
        assert!(
            model_profiles.contains_key(profile_key),
            "vision request profile has no model profile: {profile_key:?}"
        );
        let profile = value
            .as_object()
            .expect("vision request profiles must be objects");
        let fields: HashSet<&str> = profile.keys().map(String::as_str).collect();
        assert_eq!(
            fields,
            HashSet::from([
                "input_order",
                "media_resolution",
                "sampling",
                "max_output_tokens",
                "structured_output",
            ]),
            "vision request profile fields drifted for {profile_key:?}"
        );
        assert!(
            ["text-first", "image-first"].contains(&string(profile, "input_order")),
            "unsupported input_order for {profile_key:?}"
        );
        assert!(
            string(profile, "media_resolution") == "provider-default",
            "unsupported media_resolution for {profile_key:?}"
        );
        let sampling = string(profile, "sampling");
        assert!(
            ["provider-default", "qwen3-groq-non-thinking"].contains(&sampling),
            "unsupported sampling policy for {profile_key:?}"
        );
        let max_output_tokens = match profile.get("max_output_tokens") {
            Some(serde_json::Value::Null) => None,
            Some(value) => value.as_u64(),
            None => panic!("missing max_output_tokens for {profile_key:?}"),
        };
        assert!(
            max_output_tokens.is_none_or(|value| (1..=u32::MAX as u64).contains(&value)),
            "invalid max_output_tokens for {profile_key:?}"
        );
        assert!(
            [
                "unsupported",
                "prompt-only",
                "json-object",
                "strict-json-schema",
            ]
            .contains(&string(profile, "structured_output")),
            "unsupported structured_output policy for {profile_key:?}"
        );
        if sampling == "qwen3-groq-non-thinking" {
            assert!(
                profile_key.starts_with("groq:")
                    && model_profiles[profile_key]["reasoning_policy"] == "openai-none"
                    && max_output_tokens.is_some(),
                "qwen3-groq-non-thinking requires Groq reasoning policy openai-none and an output limit"
            );
        }
    }
}
