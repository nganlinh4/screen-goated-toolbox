use std::fs;
use std::path::Path;

pub(crate) fn generate(manifest_path: &Path, output_path: &Path) {
    let manifest = fs::read_to_string(manifest_path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {}", manifest_path.display(), err));
    let manifest: serde_json::Value = serde_json::from_str(&manifest)
        .unwrap_or_else(|err| panic!("Failed to parse {}: {}", manifest_path.display(), err));
    validate(&manifest);

    let constants = manifest_object(&manifest, "constants");
    let defaults = manifest_object(&manifest, "defaults");

    let constant_mappings = [
        ("DEFAULT_IMAGE_MODEL_ID", "default_image_model_id"),
        ("DEFAULT_TEXT_MODEL_ID", "default_text_model_id"),
        ("DEFAULT_TEXT_API_MODEL", "default_text_api_model"),
        ("GEMINI_LIVE_API_MODEL_2_5", "gemini_live_api_model_2_5"),
        ("GEMINI_LIVE_API_MODEL_3_1", "gemini_live_api_model_3_1"),
        (
            "GEMINI_LIVE_AUDIO_MODEL_ID_2_5",
            "gemini_live_audio_model_id_2_5",
        ),
        (
            "GEMINI_LIVE_AUDIO_MODEL_ID_3_1",
            "gemini_live_audio_model_id_3_1",
        ),
        (
            "GEMINI_LIVE_TRANSLATE_MODEL_ID",
            "gemini_live_translate_model_id",
        ),
        (
            "GEMINI_LIVE_TRANSLATE_API_MODEL",
            "gemini_live_translate_api_model",
        ),
        ("QWEN3_ASR_0_6B_MODEL_ID", "qwen3_asr_0_6b_model_id"),
        ("QWEN3_ASR_1_7B_MODEL_ID", "qwen3_asr_1_7b_model_id"),
        (
            "REALTIME_TRANSLATION_MODEL_LLM",
            "realtime_translation_model_llm",
        ),
        (
            "REALTIME_TRANSLATION_MODEL_GTX",
            "realtime_translation_model_gtx",
        ),
    ];

    let mut lines = vec![
        "// Generated from catalog/model_catalog.json. Do not edit by hand.".to_string(),
        String::new(),
    ];

    for (const_name, manifest_key) in constant_mappings {
        let value = manifest_string(constants, manifest_key);
        lines.push(format!(
            "pub const {const_name}: &str = {};",
            rust_string(value)
        ));
    }

    lines.push(format!(
        "pub const DEFAULT_GEMINI_LIVE_TTS_MODEL: &str = {};",
        rust_string(manifest_string(defaults, "tts_gemini_live_model"))
    ));
    lines.push(format!(
        "pub const DEFAULT_REALTIME_TRANSCRIPTION_MODEL: &str = {};",
        rust_string(manifest_string(defaults, "realtime_transcription_model"))
    ));
    lines.push(String::new());

    lines.push("pub fn generated_normalize_model_id(model_id: &str) -> &str {".to_string());
    lines.push("    match model_id {".to_string());
    for (old, replacement) in manifest_object(&manifest, "model_id_migrations") {
        lines.push(format!(
            "        {} => {},",
            rust_string(old),
            rust_string(replacement.as_str().unwrap())
        ));
    }
    lines.push("        _ => model_id,".to_string());
    lines.push("    }".to_string());
    lines.push("}".to_string());
    lines.push(String::new());

    lines.push(
        "pub fn generated_live_endpoint_profile(api_model: &str) -> Option<LiveEndpointProfile> {"
            .to_string(),
    );
    lines.push("    match api_model {".to_string());
    for (endpoint, metadata) in manifest_object(&manifest, "endpoints") {
        let thinking = metadata
            .get("live_thinking")
            .and_then(serde_json::Value::as_object)
            .map(|thinking| {
                let kind = manifest_string(thinking, "kind");
                let value = thinking.get("value").unwrap();
                match kind {
                    "budget" => format!(
                        "Some(LiveThinkingConfig::Budget({}))",
                        value.as_u64().unwrap()
                    ),
                    "level" => format!(
                        "Some(LiveThinkingConfig::Level({}))",
                        rust_string(value.as_str().unwrap())
                    ),
                    _ => panic!("unsupported live thinking kind {kind:?}"),
                }
            })
            .unwrap_or_else(|| "None".to_string());
        let limit = metadata
            .get("live_max_output_tokens")
            .and_then(serde_json::Value::as_u64)
            .map(|limit| format!("Some({limit})"))
            .unwrap_or_else(|| "None".to_string());
        let automatic_activity_detection_default = metadata
            .get("live_automatic_activity_detection_default")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let protocol = metadata
            .get("live_protocol")
            .and_then(serde_json::Value::as_str)
            .map(|protocol| format!("Some({})", rust_string(protocol)))
            .unwrap_or_else(|| "None".to_string());
        lines.push(format!(
            "        {} => Some(LiveEndpointProfile {{ lifecycle: {}, thinking: {thinking}, max_output_tokens: {limit}, automatic_activity_detection_default: {automatic_activity_detection_default}, protocol: {protocol} }}),",
            rust_string(endpoint),
            rust_string(manifest_string(metadata.as_object().unwrap(), "lifecycle")),
        ));
    }
    lines.push("        _ => None,".to_string());
    lines.push("    }".to_string());
    lines.push("}".to_string());
    lines.push(String::new());

    let preset_defaults = manifest_object(&manifest, "preset_defaults");
    for (const_name, value) in preset_defaults {
        lines.push(format!(
            "pub const {const_name}: &str = {};",
            rust_string(value.as_str().unwrap())
        ));
    }
    if !preset_defaults.is_empty() {
        lines.push(String::new());
    }

    lines.push("pub const GENERATED_NON_LLM_IDS: &[&str] = &[".to_string());
    for value in manifest_array(&manifest, "non_llm_ids") {
        lines.push(format!("    {},", rust_string(value.as_str().unwrap())));
    }
    lines.push("];".to_string());
    lines.push(String::new());

    lines.push("pub const GENERATED_SEARCH_DISABLED_FULL_NAMES: &[&str] = &[".to_string());
    for value in manifest_array(&manifest, "search_disabled_full_names") {
        lines.push(format!("    {},", rust_string(value.as_str().unwrap())));
    }
    lines.push("];".to_string());
    lines.push(String::new());

    let priority_chains = manifest_object(&manifest, "priority_chains");
    lines.push("pub const DEFAULT_IMAGE_TO_TEXT_PRIORITY_CHAIN_IDS: &[&str] = &[".to_string());
    for value in manifest_array_from_object(priority_chains, "image_to_text") {
        lines.push(format!("    {},", rust_string(value.as_str().unwrap())));
    }
    lines.push("];".to_string());
    lines.push(String::new());
    lines.push("pub const DEFAULT_TEXT_TO_TEXT_PRIORITY_CHAIN_IDS: &[&str] = &[".to_string());
    for value in manifest_array_from_object(priority_chains, "text_to_text") {
        lines.push(format!("    {},", rust_string(value.as_str().unwrap())));
    }
    lines.push("];".to_string());
    lines.push(String::new());

    lines.push("pub const GENERATED_TTS_GEMINI_MODELS: &[(&str, &str)] = &[".to_string());
    for value in manifest_array(&manifest, "tts_gemini_models") {
        let item = value
            .as_object()
            .expect("tts model entries must be objects");
        lines.push(format!(
            "    ({}, {}),",
            rust_string(manifest_string(item, "api_model")),
            rust_string(manifest_string(item, "label"))
        ));
    }
    lines.push("];".to_string());
    lines.push(String::new());

    let realtime_options = manifest_object(&manifest, "realtime_transcription_options");
    lines.push(
        "pub const GENERATED_REALTIME_TRANSCRIPTION_OPTIONS: &[(&str, &str)] = &[".to_string(),
    );
    for value in manifest_array_from_object(realtime_options, "windows") {
        let id = value.as_str().unwrap();
        lines.push(format!(
            "    ({}, {}),",
            rust_string(id),
            rust_string(realtime_transcription_option_label(&manifest, id))
        ));
    }
    lines.push("];".to_string());
    lines.push(String::new());

    lines.push("pub fn generated_models() -> Vec<ModelConfig> {".to_string());
    lines.push("    vec![".to_string());
    for value in manifest_array(&manifest, "models") {
        let model = value.as_object().expect("model entries must be objects");
        if !model
            .get("enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let model_type = manifest_string(model, "model_type");
        lines.extend([
            "        ModelConfig::new(".to_string(),
            format!("            {},", rust_string(manifest_string(model, "id"))),
            format!(
                "            {},",
                rust_string(manifest_string(model, "provider"))
            ),
            format!(
                "            {},",
                rust_string(manifest_string(model, "name_vi"))
            ),
            format!(
                "            {},",
                rust_string(manifest_string(model, "name_ko"))
            ),
            format!(
                "            {},",
                rust_string(manifest_string(model, "name_en"))
            ),
            format!(
                "            {},",
                rust_string(manifest_string(model, "full_name"))
            ),
            format!("            ModelType::{},", model_type),
            "            true,".to_string(),
            format!(
                "            {},",
                rust_string(manifest_string(model, "quota_vi"))
            ),
            format!(
                "            {},",
                rust_string(manifest_string(model, "quota_ko"))
            ),
            format!(
                "            {},",
                rust_string(manifest_string(model, "quota_en"))
            ),
            "        ),".to_string(),
        ]);
    }
    lines.push("    ]".to_string());
    lines.push("}".to_string());
    lines.push(String::new());

    lines.push(
        "pub fn generated_normalize_realtime_transcription_model_id(model_id: &str) -> &'static str {"
            .to_string(),
    );
    lines.push("    match model_id {".to_string());
    for (alias, normalized) in manifest_object(&manifest, "realtime_transcription_aliases") {
        lines.push(format!(
            "        {} => {},",
            rust_string(alias),
            rust_string(normalized.as_str().unwrap())
        ));
    }
    lines.push(format!(
        "        _ => {},",
        rust_string(manifest_string(defaults, "realtime_transcription_model"))
    ));
    lines.push("    }".to_string());
    lines.push("}".to_string());
    lines.push(String::new());

    fs::write(output_path, lines.join("\n"))
        .unwrap_or_else(|err| panic!("Failed to write {}: {}", output_path.display(), err));
}

fn validate(manifest: &serde_json::Value) {
    use std::collections::HashSet;
    let models = manifest_array(manifest, "models");
    let mut ids = HashSet::new();
    for item in models {
        let model = item.as_object().expect("model entries must be objects");
        let id = manifest_string(model, "id");
        assert!(ids.insert(id), "duplicate model id {id:?}");
    }
    for (old, replacement) in manifest_object(manifest, "model_id_migrations") {
        assert!(
            old != replacement.as_str().unwrap(),
            "self-referential migration {old:?}"
        );
        assert!(
            ids.contains(replacement.as_str().unwrap()),
            "migration target is not a model id: {replacement}"
        );
    }
    let endpoints = manifest_object(manifest, "endpoints");
    for (endpoint, value) in endpoints {
        let metadata = value
            .as_object()
            .expect("endpoint lifecycle must be an object");
        let lifecycle = manifest_string(metadata, "lifecycle");
        assert!(
            ["stable", "preview", "experimental", "deprecated", "retired"].contains(&lifecycle),
            "invalid lifecycle for {endpoint}"
        );
        assert!(
            !manifest_string(metadata, "verified_at").is_empty(),
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
        if let Some(thinking) = metadata.get("live_thinking") {
            let thinking = thinking
                .as_object()
                .unwrap_or_else(|| panic!("live_thinking for {endpoint} must be an object"));
            let kind = thinking
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| panic!("live_thinking kind for {endpoint} must be a string"));
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
            let limit = limit.as_u64().unwrap_or_else(|| {
                panic!("live_max_output_tokens for {endpoint} must be a positive u32")
            });
            assert!(
                (1..=u32::MAX as u64).contains(&limit),
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
    let forbidden = |endpoint: &str| {
        endpoints
            .get(endpoint)
            .and_then(|v| v.get("lifecycle"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|stage| matches!(stage, "deprecated" | "retired"))
    };
    let constants = manifest_object(manifest, "constants");
    for key in ["gemini_live_api_model_2_5", "gemini_live_api_model_3_1"] {
        let endpoint = manifest_string(constants, key);
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
            let endpoint = manifest_string(model, "full_name");
            assert!(
                !forbidden(endpoint),
                "enabled model uses deprecated/retired endpoint {endpoint:?}"
            );
        }
    }
    let defaults = manifest_object(manifest, "defaults");
    assert!(
        !forbidden(manifest_string(defaults, "tts_gemini_live_model")),
        "default TTS endpoint is deprecated/retired"
    );
    for item in manifest_array(manifest, "tts_gemini_models") {
        let endpoint = manifest_string(item.as_object().unwrap(), "api_model");
        assert!(
            !forbidden(endpoint),
            "TTS option is deprecated/retired: {endpoint}"
        );
    }
}

fn manifest_object<'a>(
    manifest: &'a serde_json::Value,
    key: &str,
) -> &'a serde_json::Map<String, serde_json::Value> {
    manifest
        .get(key)
        .and_then(serde_json::Value::as_object)
        .unwrap_or_else(|| panic!("manifest key {key:?} must be an object"))
}

fn manifest_array<'a>(manifest: &'a serde_json::Value, key: &str) -> &'a Vec<serde_json::Value> {
    manifest
        .get(key)
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("manifest key {key:?} must be an array"))
}

fn manifest_array_from_object<'a>(
    manifest: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> &'a Vec<serde_json::Value> {
    manifest
        .get(key)
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("manifest object key {key:?} must be an array"))
}

fn realtime_transcription_option_label<'a>(
    manifest: &'a serde_json::Value,
    id: &'a str,
) -> &'a str {
    let _ = manifest;
    match id {
        "gemini-live-audio" => "Gemini Live",
        "gemini-live-audio-3.1" => "Gemini S2S",
        "gemini-3.5-translate" => "Gemini Translate",
        "parakeet" => "Parakeet",
        "qwen3-asr-0.6b" => "Qwen3-ASR 0.6B",
        "qwen3-asr-1.7b" => "Qwen3-ASR 1.7B",
        "zipformer" => "Zipformer",
        "moonshine-tiny-streaming" => "Moonshine Tiny",
        "moonshine-small-streaming" => "Moonshine Small",
        "moonshine-medium-streaming" => "Moonshine Medium",
        _ => id,
    }
}

fn manifest_string<'a>(
    manifest: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> &'a str {
    manifest
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("manifest object key {key:?} must be a string"))
}

fn rust_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
