use serde::Serialize;

use crate::config::Config;
use crate::config::tts_catalog::{
    GEMINI_VOICES, KOKORO_VOICE_LANGUAGES, KOKORO_VOICES, MAGPIE_VOICE_LANGUAGES, MAGPIE_VOICES,
    SUPERTONIC_LANGUAGES, SUPERTONIC_VOICES, SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES,
    default_kokoro_voice_for_lang, default_magpie_voice_for_lang,
    default_supertonic_voice_for_lang, kokoro_voice_language_for_condition,
    normalize_supertonic_lang,
};
use crate::gui::locale::LocaleText;

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CatalogsView {
    gemini_models: Vec<serde_json::Value>,
    gemini_voices: Vec<serde_json::Value>,
    gemini_instruction_languages: Vec<serde_json::Value>,
    edge_voices_by_language: serde_json::Value,
    edge_available_languages: Vec<serde_json::Value>,
    magpie_voices_by_language: serde_json::Value,
    magpie_available_languages: Vec<serde_json::Value>,
    kokoro_voices_by_language: serde_json::Value,
    kokoro_available_languages: Vec<serde_json::Value>,
    supertonic_voices_by_language: serde_json::Value,
    supertonic_available_languages: Vec<serde_json::Value>,
    s2s_languages: Vec<serde_json::Value>,
    audio_edit_tasks: Vec<serde_json::Value>,
    audio_edit_subtasks_by_task: serde_json::Value,
    paralinguistic_tags: Vec<&'static str>,
    step_audio_references: Vec<serde_json::Value>,
    vieneu_references: Vec<serde_json::Value>,
}

impl CatalogsView {
    pub(super) fn from_config(config: &Config, _text: &LocaleText) -> Self {
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

        let magpie_voice_options = MAGPIE_VOICES
            .iter()
            .map(|voice| serde_json::json!({ "value": voice.id, "label": voice.label }))
            .collect::<Vec<_>>();
        let magpie_voices_by_language = MAGPIE_VOICE_LANGUAGES
            .iter()
            .map(|(code, _)| {
                (
                    (*code).to_string(),
                    serde_json::Value::Array(magpie_voice_options.clone()),
                )
            })
            .collect::<serde_json::Map<_, _>>();

        let kokoro_voices_by_language = KOKORO_VOICE_LANGUAGES
            .iter()
            .filter_map(|(code, _)| {
                let voice_lang = kokoro_voice_language_for_condition(code)?;
                let voices = KOKORO_VOICES
                    .iter()
                    .filter(|voice| voice.language_code == voice_lang)
                    .map(|voice| {
                        serde_json::json!({
                            "value": voice.id,
                            "label": format!("{} ({})", voice.id, voice.label),
                        })
                    })
                    .collect::<Vec<_>>();
                Some(((*code).to_string(), serde_json::Value::Array(voices)))
            })
            .collect::<serde_json::Map<_, _>>();

        let supertonic_voice_options = SUPERTONIC_VOICES
            .iter()
            .map(|voice| serde_json::json!({ "value": voice.id, "label": voice.label }))
            .collect::<Vec<_>>();
        let supertonic_voices_by_language = SUPERTONIC_LANGUAGES
            .iter()
            .map(|lang| {
                (
                    lang.code.to_string(),
                    serde_json::Value::Array(supertonic_voice_options.clone()),
                )
            })
            .collect::<serde_json::Map<_, _>>();

        let reference_voices = config
            .step_audio_reference_voices
            .iter()
            .map(|reference| {
                serde_json::json!({
                    "id": reference.id,
                    "name": if reference.label.trim().is_empty() {
                        "Reference voice"
                    } else {
                        reference.label.as_str()
                    },
                    "audioPath": reference.audio_path,
                    "transcript": reference.transcript,
                })
            })
            .collect::<Vec<_>>();

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
            magpie_voices_by_language: serde_json::Value::Object(magpie_voices_by_language),
            magpie_available_languages: MAGPIE_VOICE_LANGUAGES
                .iter()
                .map(|(code, name)| {
                    serde_json::json!({
                        "value": code,
                        "label": format!("{} · default {}", name, default_magpie_voice_for_lang(code)),
                    })
                })
                .collect(),
            kokoro_voices_by_language: serde_json::Value::Object(kokoro_voices_by_language),
            kokoro_available_languages: KOKORO_VOICE_LANGUAGES
                .iter()
                .map(|(code, name)| {
                    let voice_lang = kokoro_voice_language_for_condition(code).unwrap_or("en-us");
                    serde_json::json!({
                        "value": code,
                        "label": format!("{} · default {}", name, default_kokoro_voice_for_lang(voice_lang)),
                    })
                })
                .collect(),
            supertonic_voices_by_language: serde_json::Value::Object(supertonic_voices_by_language),
            supertonic_available_languages: SUPERTONIC_LANGUAGES
                .iter()
                .map(|lang| {
                    serde_json::json!({
                        "value": lang.code,
                        "label": format!(
                            "{} · default {}",
                            lang.label,
                            default_supertonic_voice_for_lang(
                                normalize_supertonic_lang(lang.code).as_deref().unwrap_or(lang.code)
                            )
                        ),
                    })
                })
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
            audio_edit_tasks: [
                "emotion",
                "style",
                "speed",
                "denoise",
                "vad",
                "paralinguistic",
            ]
            .into_iter()
            .map(|v| serde_json::json!({ "value": v, "label": v }))
            .collect(),
            audio_edit_subtasks_by_task: audio_edit_subtasks(),
            paralinguistic_tags: vec![
                "[sigh]",
                "[inhale]",
                "[laugh]",
                "[chuckle]",
                "[exhale]",
                "[clears throat]",
                "[snort]",
                "[giggle]",
                "[cough]",
                "[breath]",
                "[uhm]",
                "[Confirmation-en]",
                "[Surprise-oh]",
                "[Surprise-ah]",
                "[Surprise-wa]",
                "[Surprise-yo]",
                "[Dissatisfaction-hnn]",
                "[Question-ei]",
                "[Question-ah]",
                "[Question-en]",
                "[Question-yi]",
                "[Question-oh]",
            ],
            step_audio_references: reference_voices.clone(),
            vieneu_references: reference_voices,
        }
    }
}

fn audio_edit_subtasks() -> serde_json::Value {
    serde_json::json!({
        "emotion": options(&[
            "happy", "angry", "sad", "humour", "confusion", "disgusted", "empathy",
            "embarrass", "fear", "surprised", "excited", "depressed", "coldness",
            "admiration", "remove",
        ]),
        "style": options(&[
            "serious", "arrogant", "child", "older", "girl", "pure", "sister",
            "sweet", "ethereal", "whisper", "gentle", "recite", "generous",
            "act_coy", "warm", "shy", "comfort", "authority", "chat", "radio",
            "soulful", "story", "vivid", "program", "news", "advertising",
            "roar", "murmur", "shout", "deeply", "loudly", "remove",
            "exaggerated",
        ]),
        "speed": options(&["faster", "slower", "more faster", "more slower"]),
        "denoise": [],
        "vad": [],
        "paralinguistic": [],
    })
}

fn options(values: &[&str]) -> Vec<serde_json::Value> {
    values
        .iter()
        .map(|value| serde_json::json!({ "value": value, "label": value }))
        .collect()
}
