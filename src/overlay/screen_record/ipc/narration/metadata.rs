use super::*;

pub fn handle_get_narration_tts_metadata(
    _args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    crate::api::tts::edge_voices::load_edge_voices_async();
    let defaults = TtsPlaygroundSettings::default();

    let gemini_voices: Vec<serde_json::Value> = GEMINI_VOICES
        .iter()
        .map(|(name, gender)| serde_json::json!({ "name": name, "gender": gender }))
        .collect();

    let gemini_models: Vec<serde_json::Value> = tts_gemini_model_options()
        .iter()
        .map(|(api_model, label)| serde_json::json!({ "apiModel": api_model, "label": label }))
        .collect();

    let gemini_instruction_languages: Vec<serde_json::Value> =
        SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES
            .iter()
            .map(|(code, name)| serde_json::json!({ "languageCode": code, "languageName": name }))
            .collect();

    let default_language_conditions: Vec<serde_json::Value> = defaults
        .gemini_language_conditions
        .iter()
        .map(|condition| {
            serde_json::json!({
                "languageCode": condition.language_code,
                "languageName": condition.language_name,
                "instruction": condition.instruction,
            })
        })
        .collect();

    let default_edge_voice_configs: Vec<serde_json::Value> = defaults
        .edge_settings
        .voice_configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "languageCode": config.language_code,
                "languageName": config.language_name,
                "voiceName": config.voice_name,
            })
        })
        .collect();

    let (edge_voice_state, edge_voice_error, edge_voice_languages, edge_voices_by_language) = {
        let cache = crate::api::tts::edge_voices::EDGE_VOICE_CACHE
            .lock()
            .map_err(|_| "Lock Edge TTS voice cache".to_string())?;
        let state = if cache.loaded {
            "loaded"
        } else if cache.loading {
            "loading"
        } else if cache.error.is_some() {
            "error"
        } else {
            "idle"
        };

        let mut language_names = std::collections::HashMap::<String, String>::new();
        for voice in &cache.voices {
            let lang_code = voice
                .locale
                .split('-')
                .next()
                .unwrap_or(&voice.locale)
                .to_lowercase();
            language_names.entry(lang_code).or_insert_with(|| {
                voice
                    .friendly_name
                    .rfind(" - ")
                    .and_then(|dash_pos| {
                        let lang_region = &voice.friendly_name[dash_pos + 3..];
                        lang_region
                            .find(" (")
                            .map(|paren_pos| lang_region[..paren_pos].to_string())
                            .or_else(|| Some(lang_region.to_string()))
                    })
                    .unwrap_or_else(|| voice.locale.clone())
            });
        }

        let mut languages: Vec<serde_json::Value> = language_names
            .iter()
            .map(|(code, name)| {
                serde_json::json!({
                    "languageCode": code,
                    "languageName": name,
                })
            })
            .collect();
        languages.sort_by(|left, right| {
            let left_name = left["languageName"].as_str().unwrap_or_default();
            let right_name = right["languageName"].as_str().unwrap_or_default();
            left_name.cmp(right_name)
        });

        let voices_by_language = cache
            .by_language
            .iter()
            .map(|(code, voices)| {
                let options: Vec<serde_json::Value> = voices
                    .iter()
                    .map(|voice| {
                        serde_json::json!({
                            "shortName": voice.short_name,
                            "gender": voice.gender,
                            "friendlyName": voice.friendly_name,
                            "locale": voice.locale,
                        })
                    })
                    .collect();
                (code.clone(), serde_json::Value::Array(options))
            })
            .collect::<serde_json::Map<_, _>>();

        (
            state,
            cache.error.clone(),
            languages,
            serde_json::Value::Object(voices_by_language),
        )
    };

    let providers: Vec<serde_json::Value> = narration_tts_providers()
        .map(|provider| {
            serde_json::json!({
                "method": provider.id,
                "label": provider.label,
            })
        })
        .collect();
    let default_method = tts_method_id(&defaults.method);
    let defaults_json = serde_json::json!({
        "method": default_method,
        "geminiModel": defaults.gemini_model,
        "geminiVoice": defaults.gemini_voice,
        "geminiSpeed": defaults.gemini_speed,
        "geminiInstruction": defaults.gemini_instruction,
        "geminiLanguageConditions": default_language_conditions,
        "geminiParallelRequests": default_gemini_parallel_requests(),
        "geminiS2sParallelRequests": default_gemini_s2s_parallel_requests(),
        "googleSpeed": defaults.google_speed,
        "edgeVoice": defaults.edge_voice,
        "edgePitch": defaults.edge_settings.pitch,
        "edgeRate": defaults.edge_settings.rate,
        "edgeVoiceConfigs": default_edge_voice_configs,
    });

    Ok(serde_json::json!({
        "providers": providers,
        "geminiVoices": gemini_voices,
        "geminiModels": gemini_models,
        "geminiInstructionLanguages": gemini_instruction_languages,
        "geminiSpeedOptions": ["Slow", "Normal", "Fast"],
        "googleSpeedOptions": ["Slow", "Normal"],
        "edgeVoiceState": edge_voice_state,
        "edgeVoiceError": edge_voice_error,
        "edgeVoiceLanguages": edge_voice_languages,
        "edgeVoicesByLanguage": edge_voices_by_language,
        "defaults": defaults_json,
    }))
}
