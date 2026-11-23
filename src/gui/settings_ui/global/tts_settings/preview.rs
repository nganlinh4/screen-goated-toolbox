use crate::gui::locale::LocaleText;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static LAST_TTS_PREVIEW_IDX: AtomicUsize = AtomicUsize::new(9999);
static PREVIEW_OWNER: Mutex<Option<u64>> = Mutex::new(None);

fn replace_preview(
    owner: &Mutex<Option<u64>>,
    manager: &crate::api::tts::TtsManager,
    text: &str,
    profile: Option<crate::api::tts::types::TtsRequestProfile>,
) -> u64 {
    let mut current = owner.lock().unwrap();
    if let Some(id) = current.take() {
        manager.stop_if_active(id);
    }
    let id = match profile {
        Some(profile) => manager.speak_with_profile(text, 0, profile),
        None => manager.speak(text, 0),
    };
    *current = Some(id);
    id
}

/// Speak a randomized preview line for the given speaker, avoiding repeating the
/// previously used line back-to-back. Falls back to a fixed English line when no
/// localized preview texts are configured.
pub(super) fn speak_settings_preview(text: &LocaleText, speaker_name: &str) {
    let preview_text = if !text.tts_settings.tts_preview_texts.is_empty() {
        let s = RandomState::new();
        let mut hasher = s.build_hasher();
        hasher.write_usize(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as usize,
        );
        let rand_val = hasher.finish();
        let len = text.tts_settings.tts_preview_texts.len();
        let mut idx = (rand_val as usize) % len;

        let last = LAST_TTS_PREVIEW_IDX.load(Ordering::Relaxed);
        if idx == last {
            idx = (idx + 1) % len;
        }
        LAST_TTS_PREVIEW_IDX.store(idx, Ordering::Relaxed);

        text.tts_settings.tts_preview_texts[idx].replace("{}", speaker_name)
    } else {
        format!("Hello, I am {}. This is a voice preview.", speaker_name)
    };
    replace_preview(
        &PREVIEW_OWNER,
        &crate::api::tts::TTS_MANAGER,
        &preview_text,
        None,
    );
}

fn sample(language: &str) -> (&'static str, bool) {
    static SAMPLES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let data = SAMPLES.get_or_init(|| {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/tts-runtime/voice-preview.json"
        )))
        .expect("valid voice preview samples")
    });
    let code = language.trim().to_ascii_lowercase();
    let code = match code.as_str() {
        "cmn" | "zho" => "zh".to_string(),
        value if value.len() == 3 => isolang::Language::from_639_3(value)
            .and_then(|lang| lang.to_639_1())
            .unwrap_or(value)
            .to_string(),
        value => value.split(['-', '_']).next().unwrap_or(value).to_string(),
    };
    match data["samples"][&code].as_str() {
        Some(text) => (text, false),
        None => (data["fallback"].as_str().expect("preview fallback"), true),
    }
}

pub(super) fn row_preview_label(text: &LocaleText, language: &str) -> &'static str {
    if sample(language).1 {
        text.tts_settings.tts_preview_english_sample
    } else {
        text.tts_settings.tts_preview_label
    }
}

fn edge_profile(
    settings: &crate::config::EdgeTtsSettings,
    row: &crate::config::EdgeTtsVoiceConfig,
) -> crate::api::tts::types::TtsRequestProfile {
    let mut profile = crate::api::tts::types::TtsRequestProfile::from(
        &crate::config::TtsPlaygroundSettings::default(),
    );
    let code = row.language_code.trim().to_ascii_lowercase();
    let language = isolang::Language::from_639_1(&code)
        .map(|lang| lang.to_639_3())
        .unwrap_or("eng");
    let routing_code = isolang::Language::from_639_3(language)
        .and_then(|lang| lang.to_639_1())
        .unwrap_or("en");
    let mut selected = row.clone();
    selected.language_code = routing_code.to_string();
    profile.method = crate::config::TtsMethod::EdgeTTS;
    profile.edge_settings = settings.clone();
    profile.edge_settings.voice_configs = vec![selected];
    profile.language_code_override = Some(language.to_string());
    profile
}

pub(super) fn preview_edge_row(
    settings: &crate::config::EdgeTtsSettings,
    row: &crate::config::EdgeTtsVoiceConfig,
) {
    replace_preview(
        &PREVIEW_OWNER,
        &crate::api::tts::TTS_MANAGER,
        sample(&row.language_code).0,
        Some(edge_profile(settings, row)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_profiles_follow_shared_voice_and_sample_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/tts-runtime/voice-preview.json"
        )))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let language = case["language"].as_str().unwrap();
            let voice = case["voice"].as_str().unwrap();
            let sample_code = case["expectedSampleLanguage"].as_str().unwrap();
            let expected = if sample_code == "fallback" {
                &fixture["fallback"]
            } else {
                &fixture["samples"][sample_code]
            };
            assert_eq!(
                sample(language).0,
                expected.as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(sample(language).1, sample_code == "fallback");
            assert_eq!(case["provider"], "edge", "{}", case["name"]);
            let row = crate::config::EdgeTtsVoiceConfig::new(language, "row", voice);
            let mut settings = crate::config::EdgeTtsSettings::default();
            settings.voice_configs.insert(
                0,
                crate::config::EdgeTtsVoiceConfig::new(language, "old", "older"),
            );
            let profile = edge_profile(&settings, &row);
            let selected = crate::api::tts::utils::resolve_edge_voice_for_language(
                &profile,
                profile.language_code_override.as_deref(),
            );
            assert_eq!(selected.0, voice, "{}", case["name"]);
            assert_eq!(settings.voice_configs[0].voice_name, "older");
        }
    }

    #[test]
    fn replacing_settings_preview_preserves_unrelated_speech() {
        let manager = crate::api::tts::TtsManager::new();
        let owner = Mutex::new(None);
        let unrelated = manager.speak("unrelated", 0);
        let first = replace_preview(&owner, &manager, "first preview", None);
        let row = crate::config::EdgeTtsVoiceConfig::new("ja", "Japanese", "ja-JP-NanamiNeural");
        let profile = edge_profile(&crate::config::EdgeTtsSettings::default(), &row);
        let second = replace_preview(&owner, &manager, "second preview", Some(profile));
        assert!(manager.is_speaking(unrelated));
        assert!(!manager.is_speaking(first));
        assert!(manager.is_speaking(second));
    }
}
