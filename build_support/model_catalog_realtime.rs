//! Generated Live Translate realtime-transcription selectors: the per-platform
//! option lists, S2S selectors, saved-id aliases, and per-selector capabilities.

use super::{
    manifest_array, manifest_array_from_object, manifest_object, manifest_string, rust_string,
};

pub(super) fn push_realtime_transcription(manifest: &serde_json::Value, lines: &mut Vec<String>) {
    lines.push("#[cfg(not(feature = \"recorder-worker\"))]".to_string());
    lines.push("pub const GENERATED_REALTIME_S2S_MODEL_IDS: &[&str] = &[".to_string());
    for id in manifest_array(manifest, "realtime_s2s_model_ids") {
        lines.push(format!("    {},", rust_string(id.as_str().unwrap())));
    }
    lines.push("];".to_string());

    let realtime_options = manifest_object(manifest, "realtime_transcription_options");
    lines.push(
        "pub const GENERATED_REALTIME_TRANSCRIPTION_OPTIONS: &[(&str, &str)] = &[".to_string(),
    );
    for value in manifest_array_from_object(realtime_options, "windows") {
        let id = value.as_str().unwrap();
        lines.push(format!(
            "    ({}, {}),",
            rust_string(id),
            rust_string(option_label(manifest, id))
        ));
    }
    lines.push("];".to_string());
    lines.push(String::new());

    // Selectors whose engine transcribes only an explicitly chosen source
    // language, so the UI must offer the transcription-language choice.
    lines.push("#[cfg(not(feature = \"recorder-worker\"))]".to_string());
    lines.push("pub const GENERATED_REALTIME_SOURCE_LANGUAGE_MODEL_IDS: &[&str] = &[".to_string());
    for value in manifest_array(manifest, "realtime_transcription_requires_source_language") {
        let id = value
            .as_str()
            .expect("realtime_transcription_requires_source_language entries must be strings");
        assert!(
            realtime_options.values().any(|platform| platform
                .as_array()
                .is_some_and(|ids| ids.iter().any(|option| option.as_str() == Some(id)))),
            "realtime_transcription_requires_source_language names unknown selector {id:?}"
        );
        lines.push(format!("    {},", rust_string(id)));
    }
    lines.push("];".to_string());
    lines.push(String::new());
}

pub(super) fn push_realtime_transcription_aliases(
    manifest: &serde_json::Value,
    lines: &mut Vec<String>,
) {
    let defaults = manifest_object(manifest, "defaults");
    lines.push(
        "pub fn generated_normalize_realtime_transcription_model_id(model_id: &str) -> &'static str {"
            .to_string(),
    );
    lines.push("    match model_id {".to_string());
    for (alias, normalized) in manifest_object(manifest, "realtime_transcription_aliases") {
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
}

fn option_label<'a>(manifest: &'a serde_json::Value, id: &str) -> &'a str {
    manifest["realtime_transcription_labels"][id]
        .as_str()
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| panic!("missing realtime transcription label for {id}"))
}
