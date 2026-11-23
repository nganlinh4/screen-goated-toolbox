use std::sync::LazyLock;

use serde::Deserialize;

#[cfg(not(feature = "recorder-worker"))]
pub const INPUT_LANGUAGE_KEY: &str = "input_language";

#[derive(Deserialize)]
pub struct SpeechLanguage {
    pub value: String,
    #[cfg(not(feature = "recorder-worker"))]
    pub label: String,
}

pub static WHISPER_LANGUAGES: LazyLock<Vec<SpeechLanguage>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../catalog/whisper-languages.json"))
        .expect("Whisper language catalog must parse")
});

pub fn whisper_language_code(value: Option<&str>) -> Option<&'static str> {
    let value = value?.trim();
    WHISPER_LANGUAGES
        .iter()
        .find(|language| language.value.eq_ignore_ascii_case(value))
        .map(|language| language.value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_and_unsupported_languages_omit_the_hint() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../parity-fixtures/preset-system/speech-language.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                whisper_language_code(case["input"].as_str()),
                case["expected"].as_str(),
                "{case}"
            );
        }
        for language in WHISPER_LANGUAGES.iter() {
            assert_eq!(
                whisper_language_code(Some(&language.value)),
                Some(language.value.as_str())
            );
        }
        let unique: std::collections::HashSet<_> = WHISPER_LANGUAGES
            .iter()
            .map(|language| &language.value)
            .collect();
        assert_eq!(unique.len(), WHISPER_LANGUAGES.len());
    }
}
