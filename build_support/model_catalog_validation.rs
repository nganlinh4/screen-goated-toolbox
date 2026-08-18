use std::collections::{HashMap, HashSet};

#[path = "model_catalog_validation/presentation.rs"]
mod presentation;
#[path = "model_catalog_validation/vision.rs"]
mod vision;

pub(super) fn validate(manifest: &serde_json::Value) {
    assert_eq!(
        manifest
            .get("schema_version")
            .and_then(serde_json::Value::as_u64),
        Some(8),
        "catalog schema_version must be 8"
    );
    assert!(
        manifest.get("model_id_migrations").is_none(),
        "permanent model ID migrations are forbidden"
    );

    let models = array(manifest, "models");
    let profiles = object(manifest, "model_profiles");
    let presentation_variants = object(manifest, "presentation_variants");
    presentation::validate_variants(models, presentation_variants);
    let mut ids = HashSet::new();
    let mut used_profiles = HashSet::new();
    let mut localized_names = HashMap::new();
    for item in models {
        let model = item.as_object().expect("model entries must be objects");
        let id = string(model, "id");
        assert!(ids.insert(id), "duplicate model id {id:?}");
        validate_model_id(id);
        let provider = string(model, "provider");
        assert!(
            id.starts_with(provider_id_prefix(provider)),
            "model ID {id:?} does not match provider {provider:?}"
        );
        let full_name = string(model, "full_name");
        let profile_key = format!("{provider}:{full_name}");
        let profile = profiles
            .get(&profile_key)
            .and_then(serde_json::Value::as_object)
            .unwrap_or_else(|| panic!("missing model profile for {profile_key:?}"));
        used_profiles.insert(profile_key.clone());
        validate_presentation(model, profile);
        validate_reasoning_policy(id, provider, profile);
        for language in ["name_vi", "name_ko", "name_en"] {
            let name =
                presentation::localized_name(model, profile, presentation_variants, language);
            let prefix = provider_prefix(string(model, "provider"));
            assert!(
                name.starts_with(&format!("{prefix} ")),
                "{language} for {id:?} must start with {prefix:?}"
            );
            validate_localized_name(language, id, &name);
            if let Some(existing_full_name) =
                localized_names.insert((language, prefix, name.clone()), profile_key.clone())
            {
                assert_eq!(
                    existing_full_name, profile_key,
                    "distinct API models share {language} name within provider group: {name:?}"
                );
            }
        }
    }
    assert_eq!(
        used_profiles.len(),
        profiles.len(),
        "model_profiles contains an unreferenced API model"
    );
    vision::validate_request_profiles(manifest, models, profiles);

    let enabled_ids: HashSet<&str> = models
        .iter()
        .filter_map(serde_json::Value::as_object)
        .filter(|model| model.get("enabled").and_then(serde_json::Value::as_bool) == Some(true))
        .map(|model| string(model, "id"))
        .collect();
    validate_chains(manifest, &enabled_ids);
    validate_endpoints(manifest, models);
}

fn validate_model_id(id: &str) {
    assert!(
        id.bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !id.starts_with('-')
            && !id.ends_with('-')
            && !id.contains("--"),
        "model ID must be lowercase ASCII kebab-case: {id:?}"
    );
    let segments: Vec<&str> = id.split('-').collect();
    assert!(
        [
            "google",
            "groq",
            "openrouter",
            "taalas",
            "qrserver",
            "local",
        ]
        .contains(&segments[0]),
        "model ID must start with a catalog provider: {id:?}"
    );
    assert!(
        ["text", "vision", "audio", "search"].contains(segments.last().unwrap()),
        "model ID must end with a capability: {id:?}"
    );
    for lifecycle in [
        "preview",
        "latest",
        "experimental",
        "stable",
        "deprecated",
        "retired",
    ] {
        assert!(
            !segments.contains(&lifecycle),
            "model ID contains mutable lifecycle word {lifecycle:?}: {id:?}"
        );
    }
}

fn validate_presentation(
    model: &serde_json::Map<String, serde_json::Value>,
    profile: &serde_json::Map<String, serde_json::Value>,
) {
    let id = string(model, "id");
    for field in [
        "name_vi",
        "name_ko",
        "name_en",
        "quota_vi",
        "quota_ko",
        "quota_en",
        "supports_search",
        "search_tool_enabled_by_default",
        "intelligence_tier",
        "reasoning_policy",
    ] {
        assert!(
            model.get(field).is_none(),
            "model row {id:?} duplicates profile-owned field {field:?}"
        );
    }
    assert!(
        profile
            .get("intelligence_tier")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|value| (1..=6).contains(&value)),
        "intelligence_tier for {id:?} must be 1..=6"
    );
    assert!(
        model
            .get("typical_latency_ms")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|value| (1..=i32::MAX as u64).contains(&value)),
        "typical_latency_ms for {id:?} must be a positive cross-platform i32"
    );
    assert!(
        !string(model, "performance_source").trim().is_empty(),
        "performance_source for {id:?} must not be empty"
    );
    assert!(
        profile
            .get("supports_search")
            .and_then(serde_json::Value::as_bool)
            .is_some(),
        "supports_search for {id:?} must be boolean"
    );
    let search_tool_enabled_by_default = profile
        .get("search_tool_enabled_by_default")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| panic!("search_tool_enabled_by_default for {id:?} must be boolean"));
    assert!(
        !search_tool_enabled_by_default
            || profile
                .get("supports_search")
                .and_then(serde_json::Value::as_bool)
                == Some(true),
        "search_tool_enabled_by_default for {id:?} requires supports_search"
    );
    validate_quota_triplet(id, profile);
}

fn validate_reasoning_policy(
    id: &str,
    provider: &str,
    profile: &serde_json::Map<String, serde_json::Value>,
) {
    let policy = string(profile, "reasoning_policy");
    assert!(
        [
            "not-applicable",
            "gemini-disabled",
            "gemini-minimal",
            "openai-none",
            "openai-low",
            "provider-managed",
            "live-profile",
        ]
        .contains(&policy),
        "unsupported reasoning_policy for {id:?}: {policy:?}"
    );
    let compatible = match provider {
        "google" => matches!(policy, "gemini-disabled" | "gemini-minimal"),
        "gemini-live" => policy == "live-profile",
        "groq" | "openrouter" => matches!(
            policy,
            "not-applicable" | "openai-none" | "openai-low" | "provider-managed"
        ),
        _ => policy == "not-applicable",
    };
    assert!(
        compatible,
        "reasoning_policy {policy:?} is incompatible with provider {provider:?} for {id:?}"
    );
}

fn validate_localized_name(language: &str, id: &str, name: &str) {
    let lower = name.to_lowercase();
    let forbidden: &[&str] = match language {
        "name_vi" => &[
            " ảnh",
            " chữ",
            "ocr",
            "định vị",
            "giới hạn",
            "sắp dừng",
            "dài dòng",
            "thử nghiệm",
            "suy luận",
            "dịch máy",
            "live lỗi",
        ],
        "name_ko" => &[
            "비전",
            "텍스트",
            "ocr",
            "위치 찾기",
            "제한",
            "곧 종료",
            "장황",
            "실험",
            "추론",
            "기계 번역",
            "오류",
        ],
        "name_en" => &[
            " vision",
            " text",
            "ocr",
            "grounding",
            "limited",
            "retiring",
            "verbose",
            "experimental",
            "reasoning",
            "machine translation",
            "live errors",
        ],
        _ => unreachable!(),
    };
    assert!(
        !forbidden.iter().any(|term| lower.contains(term)),
        "{language} for {id:?} contains forbidden category/lifecycle wording: {name:?}"
    );
}

fn validate_quota_triplet(id: &str, profile: &serde_json::Map<String, serde_json::Value>) {
    let vi = string(profile, "quota_vi");
    let ko = string(profile, "quota_ko");
    let en = string(profile, "quota_en");
    if (vi, ko, en) == ("Không giới hạn", "무제한", "Unlimited") {
        return;
    }
    let vi_count = quota_count(vi, " lượt/ngày");
    let ko_count = quota_count(ko, "회/일");
    let en_count = quota_count(en, " requests/day");
    assert!(
        vi_count.is_some() && vi_count == ko_count && ko_count == en_count,
        "quota labels for {id:?} must be the same positive daily request count or Unlimited"
    );
}

fn quota_count(label: &str, suffix: &str) -> Option<u64> {
    label
        .strip_suffix(suffix)?
        .parse::<u64>()
        .ok()
        .filter(|count| *count > 0)
}

fn validate_chains(manifest: &serde_json::Value, enabled_ids: &HashSet<&str>) {
    let priority = object(manifest, "priority_chains");
    for key in ["image_to_text", "text_to_text"] {
        validate_chain(array_from(priority, key), key, enabled_ids, None);
    }

    let features = object(manifest, "feature_model_chains");
    for key in ["help_assistant", "computer_control_grounding"] {
        validate_chain(array_from(features, key), key, enabled_ids, Some(2));
    }

    let constants = object(manifest, "constants");
    assert_eq!(
        string(constants, "default_image_model_id"),
        array_from(priority, "image_to_text")[0].as_str().unwrap(),
        "default image model must lead image_to_text"
    );
    assert_eq!(
        string(constants, "default_text_model_id"),
        array_from(priority, "text_to_text")[0].as_str().unwrap(),
        "default text model must lead text_to_text"
    );
}

fn validate_chain(
    chain: &[serde_json::Value],
    key: &str,
    enabled_ids: &HashSet<&str>,
    required_len: Option<usize>,
) {
    if let Some(required_len) = required_len {
        assert_eq!(
            chain.len(),
            required_len,
            "{key} must define primary and fallback models"
        );
    }
    assert!(!chain.is_empty(), "{key} must not be empty");
    let mut seen = HashSet::new();
    for value in chain {
        let id = value
            .as_str()
            .unwrap_or_else(|| panic!("{key} model IDs must be strings"));
        assert!(
            enabled_ids.contains(id),
            "{key} references disabled or unknown model {id:?}"
        );
        assert!(seen.insert(id), "{key} contains duplicate model {id:?}");
    }
}

fn validate_endpoints(manifest: &serde_json::Value, models: &[serde_json::Value]) {
    let endpoints = object(manifest, "endpoints");
    for (endpoint, value) in endpoints {
        let metadata = value
            .as_object()
            .expect("endpoint lifecycle must be an object");
        let lifecycle = string(metadata, "lifecycle");
        assert!(
            ["stable", "preview", "experimental", "deprecated", "retired"].contains(&lifecycle),
            "invalid lifecycle for {endpoint}"
        );
        assert!(
            !string(metadata, "verified_at").is_empty(),
            "missing verified_at for {endpoint}"
        );
        if let Some(replacement) = metadata
            .get("replacement")
            .and_then(serde_json::Value::as_str)
        {
            assert!(
                endpoints.contains_key(replacement),
                "unknown replacement {replacement:?}"
            );
        }
        validate_live_profile(endpoint, metadata);
    }

    let forbidden = |endpoint: &str| {
        endpoints
            .get(endpoint)
            .and_then(|value| value.get("lifecycle"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|stage| matches!(stage, "deprecated" | "retired"))
    };
    let constants = object(manifest, "constants");
    for key in ["gemini_live_api_model_2_5", "gemini_live_api_model_3_1"] {
        let endpoint = string(constants, key);
        let profile = endpoints
            .get(endpoint)
            .and_then(serde_json::Value::as_object)
            .unwrap_or_else(|| panic!("{key} must reference a catalog endpoint"));
        assert_eq!(
            profile
                .get("live_protocol")
                .and_then(serde_json::Value::as_str),
            Some("native-audio"),
            "{key} endpoint must use the native-audio protocol"
        );
    }
    for model in models.iter().filter_map(serde_json::Value::as_object) {
        if model.get("enabled").and_then(serde_json::Value::as_bool) == Some(true) {
            let endpoint = string(model, "full_name");
            assert!(
                !forbidden(endpoint),
                "enabled model uses deprecated/retired endpoint {endpoint:?}"
            );
        }
    }
    let defaults = object(manifest, "defaults");
    assert!(
        !forbidden(string(defaults, "tts_gemini_live_model")),
        "default TTS endpoint is deprecated/retired"
    );
    for item in array(manifest, "tts_gemini_models") {
        let endpoint = string(item.as_object().unwrap(), "api_model");
        assert!(
            !forbidden(endpoint),
            "TTS option is deprecated/retired: {endpoint}"
        );
    }
}

fn validate_live_profile(endpoint: &str, metadata: &serde_json::Map<String, serde_json::Value>) {
    if let Some(thinking) = metadata.get("live_thinking") {
        let thinking = thinking
            .as_object()
            .unwrap_or_else(|| panic!("live_thinking for {endpoint} must be an object"));
        let kind = string(thinking, "kind");
        let value = thinking
            .get("value")
            .unwrap_or_else(|| panic!("live_thinking value is required for {endpoint}"));
        match kind {
            "budget" => assert!(
                value
                    .as_u64()
                    .is_some_and(|budget| budget <= i32::MAX as u64),
                "live_thinking budget for {endpoint} must be a non-negative 32-bit integer"
            ),
            "level" => assert!(
                value.as_str().is_some_and(|level| !level.trim().is_empty()),
                "live_thinking level for {endpoint} must be a non-empty string"
            ),
            _ => panic!("unsupported live_thinking kind {kind:?} for {endpoint}"),
        }
    }
    if let Some(limit) = metadata.get("live_max_output_tokens") {
        assert!(
            limit
                .as_u64()
                .is_some_and(|limit| (1..=u32::MAX as u64).contains(&limit)),
            "live_max_output_tokens for {endpoint} must be a positive u32"
        );
    }
    if let Some(value) = metadata.get("live_automatic_activity_detection_default") {
        assert!(
            value.is_boolean(),
            "live_automatic_activity_detection_default for {endpoint} must be boolean"
        );
    }
    if let Some(value) = metadata.get("live_protocol") {
        assert!(
            value.as_str().is_some_and(|value| !value.trim().is_empty()),
            "live_protocol for {endpoint} must be a non-empty string"
        );
    }
    if metadata
        .get("live_protocol")
        .and_then(serde_json::Value::as_str)
        == Some("native-audio")
    {
        assert!(
            metadata.get("live_thinking").is_some(),
            "native-audio endpoint {endpoint} must define live_thinking"
        );
        assert!(
            metadata.get("live_max_output_tokens").is_some(),
            "native-audio endpoint {endpoint} must define live_max_output_tokens"
        );
    }
}

fn provider_prefix(provider: &str) -> &str {
    match provider {
        "google" | "google-gtx" | "gemini-live" => "GG",
        "groq" => "G",
        "openrouter" => "O",
        "taalas" => "T",
        "parakeet" | "qwen3" => "L",
        "qrserver" => "QR",
        _ => panic!("provider {provider:?} has no localized-name prefix"),
    }
}

fn provider_id_prefix(provider: &str) -> &str {
    match provider {
        "google" | "google-gtx" | "gemini-live" => "google-",
        "groq" => "groq-",
        "openrouter" => "openrouter-",
        "taalas" => "taalas-",
        "parakeet" | "qwen3" => "local-",
        "qrserver" => "qrserver-",
        _ => panic!("provider {provider:?} has no internal-ID prefix"),
    }
}

fn object<'a>(
    value: &'a serde_json::Value,
    key: &str,
) -> &'a serde_json::Map<String, serde_json::Value> {
    value
        .get(key)
        .and_then(serde_json::Value::as_object)
        .unwrap_or_else(|| panic!("manifest key {key:?} must be an object"))
}

fn array<'a>(value: &'a serde_json::Value, key: &str) -> &'a Vec<serde_json::Value> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("manifest key {key:?} must be an array"))
}

fn array_from<'a>(
    value: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> &'a Vec<serde_json::Value> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("manifest object key {key:?} must be an array"))
}

fn string<'a>(value: &'a serde_json::Map<String, serde_json::Value>, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("manifest object key {key:?} must be a string"))
}
