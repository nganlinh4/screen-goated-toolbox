//! Shared default-preset vector: `parity-fixtures/preset-system/default-presets.json`.
//!
//! The fixture is generated from `get_default_presets()` and the localized
//! preset names, then replayed by Android `DefaultPresetsParityTest`. Run this
//! test with `SGT_WRITE_PARITY_FIXTURES=1` to regenerate it after changing a
//! built-in preset. Windows-only metadata (hotkeys, window geometry) is left out.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::get_default_presets;
use crate::config::preset::{Preset, ProcessingBlock};
use crate::gui::locale::get_localized_preset_name;

const FIXTURE_PATH: &str = "parity-fixtures/preset-system/default-presets.json";

fn kind(preset: &Preset) -> String {
    match (
        preset.preset_type.as_str(),
        preset.text_input_mode.as_str(),
        preset.audio_source.as_str(),
    ) {
        ("text", "type", _) => "text_input".into(),
        ("text", _, _) => "text_select".into(),
        ("audio", _, "device") => "device_audio".into(),
        ("audio", _, _) => "mic".into(),
        (other, _, _) => other.into(),
    }
}

/// The editor's effective variables: a legacy `selected_language` fills a
/// missing `language1`, as the Windows node-graph editor does.
fn language_vars(block: &ProcessingBlock) -> BTreeMap<String, String> {
    let mut vars: BTreeMap<_, _> = block.language_vars.clone().into_iter().collect();
    if !block.selected_language.is_empty() {
        vars.entry("language1".to_string())
            .or_insert_with(|| block.selected_language.clone());
    }
    vars
}

fn block(block: &ProcessingBlock) -> Value {
    // An input adapter passes its input through: execution reads only its
    // output behavior, never a model, prompt, language or streaming flag.
    if block.block_type == "input_adapter" {
        return json!({
            "type": block.block_type,
            "renderMode": block.render_mode,
            "showOverlay": block.show_overlay,
            "autoCopy": block.auto_copy,
            "autoSpeak": block.auto_speak,
        });
    }
    json!({
        "type": block.block_type,
        "model": block.model,
        "prompt": block.prompt,
        "languageVars": language_vars(block),
        "streamingEnabled": block.streaming_enabled,
        "searchEnabled": block.search_enabled,
        "renderMode": block.render_mode,
        "showOverlay": block.show_overlay,
        "autoCopy": block.auto_copy,
        "autoSpeak": block.auto_speak,
    })
}

fn preset(preset: &Preset) -> Value {
    json!({
        "id": preset.id,
        "kind": kind(preset),
        "names": {
            "en": get_localized_preset_name(&preset.id, "en"),
            "vi": get_localized_preset_name(&preset.id, "vi"),
            "ko": get_localized_preset_name(&preset.id, "ko"),
        },
        "promptMode": preset.prompt_mode,
        "autoPaste": preset.auto_paste,
        "autoPasteNewline": preset.auto_paste_newline,
        "hideRecordingUi": preset.hide_recording_ui,
        "autoStopRecording": preset.auto_stop_recording,
        "continuousInput": preset.continuous_input,
        "isMaster": preset.is_master,
        "isFavorite": preset.is_favorite,
        "isUpcoming": preset.is_upcoming,
        "showControllerUi": preset.show_controller_ui,
        "graphExplicit": preset.graph_explicit,
        "blockConnections": preset.block_connections,
        "blocks": preset.blocks.iter().map(block).collect::<Vec<_>>(),
    })
}

fn vector() -> Value {
    json!({
        "description": "Generated from Windows get_default_presets(); regenerate with SGT_WRITE_PARITY_FIXTURES=1 and the default_presets_match_the_shared_vector test.",
        "presets": get_default_presets().iter().map(preset).collect::<Vec<_>>(),
    })
}

#[test]
fn default_presets_match_the_shared_vector() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_PATH);
    let current = vector();
    if std::env::var_os("SGT_WRITE_PARITY_FIXTURES").is_some() {
        let text = serde_json::to_string_pretty(&current).unwrap() + "\n";
        std::fs::write(&path, text).unwrap();
    }
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        saved, current,
        "{FIXTURE_PATH} is stale; regenerate it with SGT_WRITE_PARITY_FIXTURES=1"
    );
}
