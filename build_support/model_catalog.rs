use std::fs;
use std::path::Path;

#[path = "model_catalog_validation.rs"]
mod validation;

pub(crate) fn generate(manifest_path: &Path, output_path: &Path) {
    let manifest = fs::read_to_string(manifest_path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {}", manifest_path.display(), err));
    let manifest: serde_json::Value = serde_json::from_str(&manifest)
        .unwrap_or_else(|err| panic!("Failed to parse {}: {}", manifest_path.display(), err));
    validation::validate(&manifest);

    let constants = manifest_object(&manifest, "constants");
    let defaults = manifest_object(&manifest, "defaults");

    let constant_mappings = [
        ("DEFAULT_IMAGE_MODEL_ID", "default_image_model_id"),
        ("DEFAULT_TEXT_MODEL_ID", "default_text_model_id"),
        ("DEFAULT_TEXT_API_MODEL", "default_text_api_model"),
        ("GEMINI_EMBEDDING_API_MODEL", "gemini_embedding_api_model"),
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

    let feature_model_chains = manifest_object(&manifest, "feature_model_chains");
    lines.push("pub const HELP_ASSISTANT_MODEL_CHAIN_IDS: &[&str] = &[".to_string());
    for value in manifest_array_from_object(feature_model_chains, "help_assistant") {
        lines.push(format!("    {},", rust_string(value.as_str().unwrap())));
    }
    lines.push("];".to_string());
    lines.push(String::new());
    lines.push("pub const COMPUTER_CONTROL_GROUNDING_MODEL_CHAIN_IDS: &[&str] = &[".to_string());
    for value in manifest_array_from_object(feature_model_chains, "computer_control_grounding") {
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
            format!("            {},", manifest_u64(model, "quality_tier")),
            format!("            {},", manifest_u64(model, "typical_latency_ms")),
            format!(
                "            {},",
                rust_string(manifest_string(model, "performance_source"))
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
        "google-gemini-2-5-live-transcribe-audio" => "Gemini Live",
        "google-gemini-3-1-live-transcribe-audio" => "Gemini S2S",
        "google-gemini-3-5-live-translate-audio" => "Gemini Translate",
        "parakeet" => "Parakeet",
        "local-qwen-3-asr-600m-audio" => "Qwen3-ASR 0.6B",
        "local-qwen-3-asr-1-7b-audio" => "Qwen3-ASR 1.7B",
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

fn manifest_u64(manifest: &serde_json::Map<String, serde_json::Value>, key: &str) -> u64 {
    manifest
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_else(|| panic!("manifest object key {key:?} must be an unsigned integer"))
}

fn rust_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
