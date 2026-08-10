use super::migrate_config;
use crate::config::Config;

#[test]
fn retired_tts_value_deserializes_then_migrates_without_losing_legacy_settings() {
    let retired: crate::config::TtsMethod = serde_json::from_str("\"VoxtralTts\"").unwrap();
    assert_eq!(retired, crate::config::TtsMethod::VoxtralTts);

    let mut config = Config {
        tts_method: retired.clone(),
        ..Default::default()
    };
    config.tts_playground.method = retired;
    config.voxtral_settings.voice = "saved-legacy-voice".to_string();
    migrate_config(&mut config);

    assert_eq!(config.tts_method, crate::config::TtsMethod::VieneuTts);
    assert_eq!(
        config.tts_playground.method,
        crate::config::TtsMethod::VieneuTts
    );
    assert_eq!(config.voxtral_settings.voice, "saved-legacy-voice");
}
