use crate::config::{Config, TtsMethod, TtsPlaygroundMode};

const RETIRED_METHODS: [&str; 7] = [
    "FishAudioS2Pro",
    "StepAudioEditX",
    "MagpieMultilingual",
    "Kokoro",
    "VieneuTts",
    "VoxtralTts",
    "Supertonic",
];

#[test]
fn retired_local_tts_methods_load_as_the_default_provider() {
    for retired in RETIRED_METHODS {
        let saved = serde_json::json!({
            "tts_method": retired,
            "tts_voice": "Puck",
            "kokoro_settings": { "voice": "af_heart" },
            "supertonic_settings": { "speed": 1.2, "voice_configs": [] },
            "step_audio_reference_voices": [{ "id": "ref-1", "audio_path": "clip.wav" }],
            "tts_playground": {
                "method": retired,
                "mode": "AudioEdit",
                "supertonic_settings": { "speed": 1.2 },
                "step_audio_edit_settings": { "source_audio_path": "source.wav", "edit_type": "emotion" }
            }
        });
        let config: Config = serde_json::from_value(saved).expect(retired);
        assert_eq!(config.tts_method, TtsMethod::GeminiLive, "{retired}");
        assert_eq!(config.tts_voice, "Puck", "{retired}");
        assert_eq!(config.tts_playground.method, TtsMethod::GeminiLive);
        assert_eq!(config.tts_playground.mode, TtsPlaygroundMode::TtsClone);
        assert_eq!(
            config.tts_playground.source_audio.source_audio_path,
            "source.wav"
        );
        let saved_again = serde_json::to_value(&config).unwrap();
        assert_eq!(saved_again["tts_method"], "GeminiLive");
        assert!(saved_again.get("kokoro_settings").is_none());
        assert!(saved_again.get("supertonic_settings").is_none());
        assert!(
            saved_again["tts_playground"]
                .get("supertonic_settings")
                .is_none()
        );
    }
}

#[test]
fn reference_library_mode_loads_as_text_to_speech() {
    let mode: TtsPlaygroundMode = serde_json::from_str("\"ReferenceLibrary\"").unwrap();
    assert_eq!(mode, TtsPlaygroundMode::TtsClone);
}
