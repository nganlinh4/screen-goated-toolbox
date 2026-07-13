use anyhow::Result;

use crate::APP;

use super::utils::tts_instruction_for_target;

#[derive(Clone, Debug)]
pub struct S2sBatchSettings {
    pub model: String,
    pub voice: String,
    pub speed: String,
    pub target_language: String,
    pub custom_instruction: String,
    pub parallel_requests: usize,
    pub vad_group_budget: usize,
}

#[derive(Clone)]
pub(super) struct S2sSettings {
    pub(super) api_key: String,
    pub(super) model: String,
    pub(super) mode: S2sMode,
    pub(super) voice: String,
    pub(super) speed: String,
    pub(super) custom_instruction: String,
    pub(super) target_language: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum S2sMode {
    LegacyInterpreter,
    LiveTranslate,
}

impl S2sMode {
    pub(super) fn log_tag(self) -> &'static str {
        match self {
            Self::LegacyInterpreter => "RealtimeS2S",
            Self::LiveTranslate => "RealtimeLiveTranslate",
        }
    }
}

pub(super) fn load_settings() -> Result<S2sSettings> {
    let app = APP.lock().unwrap();
    let api_key = app.config.gemini_api_key.trim().to_string();
    if api_key.is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:google"));
    }
    let model = app.config.tts_gemini_live_model.trim();
    let transcription_model = crate::model_config::normalize_realtime_transcription_model_id(
        &app.config.realtime_transcription_model,
    );
    let voice = app.config.tts_voice.trim();
    let speed = app.config.tts_speed.trim();
    let target_language = app.config.realtime_target_language.clone();
    let custom_instruction =
        tts_instruction_for_target(&target_language, &app.config.tts_language_conditions);
    let mode = if crate::model_config::is_gemini_live_translate_model_id(&transcription_model) {
        S2sMode::LiveTranslate
    } else {
        S2sMode::LegacyInterpreter
    };
    Ok(S2sSettings {
        api_key,
        model: if mode == S2sMode::LiveTranslate {
            crate::model_config::GEMINI_LIVE_TRANSLATE_API_MODEL.to_string()
        } else if model.is_empty() {
            crate::model_config::GEMINI_LIVE_API_MODEL_3_1.to_string()
        } else {
            crate::model_config::normalize_tts_gemini_model(model).to_string()
        },
        mode,
        voice: if voice.is_empty() {
            "Aoede".to_string()
        } else {
            voice.to_string()
        },
        speed: if speed.is_empty() {
            "Normal".to_string()
        } else {
            speed.to_string()
        },
        custom_instruction,
        target_language,
    })
}
