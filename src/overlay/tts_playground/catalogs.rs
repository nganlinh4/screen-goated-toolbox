use serde::Serialize;

use crate::config::tts_catalog::{GEMINI_VOICES, SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES};

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CatalogsView {
    gemini_models: Vec<serde_json::Value>,
    gemini_voices: Vec<serde_json::Value>,
    gemini_instruction_languages: Vec<serde_json::Value>,
    edge_voices_by_language: serde_json::Value,
    edge_available_languages: Vec<serde_json::Value>,
    s2s_languages: Vec<serde_json::Value>,
}

impl CatalogsView {
    pub(super) fn build() -> Self {
        crate::api::tts::edge_voices::load_edge_voices_async();
        let edge_available_languages = crate::api::tts::edge_voices::get_available_languages();
        let edge_voices_by_language = edge_available_languages
            .iter()
            .map(|(code, _)| {
                let voices = crate::api::tts::edge_voices::get_voices_for_language(code)
                    .into_iter()
                    .map(|voice| {
                        serde_json::json!({
                            "value": voice.short_name,
                            "label": format!("{} ({})", voice.short_name, voice.gender),
                        })
                    })
                    .collect::<Vec<_>>();
                (code.clone(), serde_json::Value::Array(voices))
            })
            .collect::<serde_json::Map<_, _>>();

        Self {
            gemini_models: crate::model_config::tts_gemini_model_options()
                .iter()
                .map(|(value, label)| serde_json::json!({ "value": value, "label": label }))
                .collect(),
            gemini_voices: GEMINI_VOICES
                .iter()
                .map(|(voice, gender)| {
                    serde_json::json!({
                        "value": voice,
                        "label": voice,
                        "gender": gender.to_ascii_lowercase(),
                    })
                })
                .collect(),
            gemini_instruction_languages: SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES
                .iter()
                .map(|(code, name)| serde_json::json!({ "value": code, "label": name }))
                .collect(),
            edge_voices_by_language: serde_json::Value::Object(edge_voices_by_language),
            edge_available_languages: edge_available_languages
                .iter()
                .map(|(code, name)| serde_json::json!({ "value": code, "label": name }))
                .collect(),
            s2s_languages: [
                ("en", "English"),
                ("vi", "Vietnamese"),
                ("ko", "Korean"),
                ("ja", "Japanese"),
                ("zh", "Chinese"),
                ("es", "Spanish"),
                ("fr", "French"),
                ("de", "German"),
            ]
            .into_iter()
            .map(|(value, label)| serde_json::json!({ "value": value, "label": label }))
            .collect(),
        }
    }
}
