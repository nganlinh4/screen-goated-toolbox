use super::*;

/// Wire shape for a Gemini language-instruction condition. Uses camelCase to
/// match what the frontend serializes; converts into `TtsLanguageCondition`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanguageConditionWire {
    language_code: String,
    language_name: String,
    #[serde(default)]
    instruction: String,
}

impl From<LanguageConditionWire> for TtsLanguageCondition {
    fn from(wire: LanguageConditionWire) -> Self {
        TtsLanguageCondition::new(&wire.language_code, &wire.language_name, &wire.instruction)
    }
}

/// Wire shape for an Edge TTS per-language voice config. Mirrors
/// `EdgeTtsVoiceConfig` with camelCase keys for the WebView frontend.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdgeVoiceConfigWire {
    language_code: String,
    language_name: String,
    voice_name: String,
}

impl From<EdgeVoiceConfigWire> for EdgeTtsVoiceConfig {
    fn from(wire: EdgeVoiceConfigWire) -> Self {
        EdgeTtsVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_name)
    }
}

/// Wire shape for the per-request TTS profile. Mirrors the user's narration
/// settings on the frontend; gets converted into `TtsRequestProfile` here so
/// callers don't have to touch `app.config.tts_playground`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TtsProfileWire {
    method: TtsMethod,
    #[serde(default)]
    gemini_model: String,
    #[serde(default)]
    gemini_voice: String,
    #[serde(default)]
    gemini_speed: String,
    #[serde(default)]
    gemini_instruction: String,
    #[serde(default)]
    gemini_language_conditions: Vec<LanguageConditionWire>,
    #[serde(default = "default_gemini_parallel_requests")]
    gemini_parallel_requests: usize,
    #[serde(default)]
    google_speed: String,
    #[serde(default)]
    edge_voice: String,
    #[serde(default)]
    edge_pitch: i32,
    #[serde(default)]
    edge_rate: i32,
    #[serde(default)]
    edge_voice_configs: Vec<EdgeVoiceConfigWire>,
}

pub(super) fn default_gemini_parallel_requests() -> usize {
    2
}

pub(super) fn default_gemini_s2s_parallel_requests() -> usize {
    3
}

impl TtsProfileWire {
    pub(super) fn into_request_profile(
        self,
        language_code_override: Option<String>,
    ) -> TtsRequestProfile {
        // Pull catalog defaults whenever the frontend left a field blank,
        // so a fresh narration tab "just works" without forcing the user to
        // choose every value before the first run.
        let defaults = crate::config::TtsPlaygroundSettings::default();
        let trimmed_or = |value: String, fallback: String| -> String {
            if value.trim().is_empty() {
                fallback
            } else {
                value
            }
        };
        let edge_voice_configs: Vec<EdgeTtsVoiceConfig> = if self.edge_voice_configs.is_empty() {
            defaults.edge_settings.voice_configs
        } else {
            self.edge_voice_configs
                .into_iter()
                .map(EdgeTtsVoiceConfig::from)
                .collect()
        };

        TtsRequestProfile {
            method: self.method,
            gemini_model: trimmed_or(self.gemini_model, defaults.gemini_model),
            gemini_voice: trimmed_or(self.gemini_voice, defaults.gemini_voice),
            gemini_speed: trimmed_or(self.gemini_speed, defaults.gemini_speed),
            gemini_instruction: self.gemini_instruction,
            gemini_language_conditions: if self.gemini_language_conditions.is_empty() {
                defaults.gemini_language_conditions
            } else {
                self.gemini_language_conditions
                    .into_iter()
                    .map(TtsLanguageCondition::from)
                    .collect()
            },
            gemini_parallel_requests: self.gemini_parallel_requests.clamp(1, 4),
            google_speed: trimmed_or(self.google_speed, defaults.google_speed),
            edge_voice: trimmed_or(self.edge_voice, defaults.edge_voice),
            edge_settings: EdgeTtsSettings {
                pitch: self.edge_pitch,
                rate: self.edge_rate,
                volume: 0,
                voice_configs: edge_voice_configs,
            },
            language_code_override,
        }
    }
}
