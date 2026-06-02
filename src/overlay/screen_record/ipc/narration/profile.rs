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

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct KokoroVoiceConfigWire {
    language_code: String,
    language_name: String,
    voice_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MagpieVoiceConfigWire {
    language_code: String,
    language_name: String,
    voice_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StepAudioVoiceConfigWire {
    language_code: String,
    language_name: String,
    voice_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SupertonicVoiceConfigWire {
    language_code: String,
    language_name: String,
    voice_id: String,
}

impl From<KokoroVoiceConfigWire> for KokoroVoiceConfig {
    fn from(wire: KokoroVoiceConfigWire) -> Self {
        KokoroVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_id)
    }
}

impl From<EdgeVoiceConfigWire> for EdgeTtsVoiceConfig {
    fn from(wire: EdgeVoiceConfigWire) -> Self {
        EdgeTtsVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_name)
    }
}

impl From<MagpieVoiceConfigWire> for MagpieVoiceConfig {
    fn from(wire: MagpieVoiceConfigWire) -> Self {
        MagpieVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_id)
    }
}

impl From<StepAudioVoiceConfigWire> for StepAudioVoiceConfig {
    fn from(wire: StepAudioVoiceConfigWire) -> Self {
        StepAudioVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_id)
    }
}

impl From<SupertonicVoiceConfigWire> for SupertonicVoiceConfig {
    fn from(wire: SupertonicVoiceConfigWire) -> Self {
        SupertonicVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_id)
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
    #[serde(default)]
    step_audio_voice: String,
    #[serde(default)]
    step_audio_reference_voice_id: String,
    #[serde(default)]
    step_audio_voice_configs: Vec<StepAudioVoiceConfigWire>,
    #[serde(default)]
    step_audio_prompt_text: String,
    #[serde(default)]
    step_audio_use_custom_reference: bool,
    #[serde(default)]
    step_audio_reference_audio_path: String,
    #[serde(default)]
    step_audio_reference_text: String,
    #[serde(default)]
    step_audio_reference_label: String,
    #[serde(default)]
    magpie_voice: String,
    #[serde(default)]
    magpie_voice_configs: Vec<MagpieVoiceConfigWire>,
    #[serde(default)]
    kokoro_voice: String,
    #[serde(default)]
    kokoro_speed: Option<f32>,
    #[serde(default)]
    kokoro_num_threads: Option<i32>,
    #[serde(default)]
    kokoro_voice_configs: Vec<KokoroVoiceConfigWire>,
    #[serde(default)]
    supertonic_speed: Option<f32>,
    #[serde(default)]
    supertonic_num_steps: Option<i32>,
    #[serde(default)]
    supertonic_num_threads: Option<i32>,
    #[serde(default)]
    supertonic_voice_configs: Vec<SupertonicVoiceConfigWire>,
    #[serde(default)]
    vieneu_variant: String,
    #[serde(default)]
    vieneu_emotion: String,
    #[serde(default)]
    vieneu_reference_voice_id: String,
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
        let kokoro_voice = trimmed_or(self.kokoro_voice, defaults.kokoro_settings.voice.clone());
        let magpie_voice = normalize_magpie_voice(&trimmed_or(
            self.magpie_voice,
            defaults.magpie_settings.voice.clone(),
        ));
        let magpie_voice_configs = if self.magpie_voice_configs.is_empty() {
            defaults.magpie_settings.voice_configs
        } else {
            self.magpie_voice_configs
                .into_iter()
                .map(MagpieVoiceConfig::from)
                .collect()
        };
        let kokoro_voice_configs = if self.kokoro_voice_configs.is_empty() {
            defaults.kokoro_settings.voice_configs
        } else {
            self.kokoro_voice_configs
                .into_iter()
                .map(KokoroVoiceConfig::from)
                .collect()
        };
        let supertonic_voice_configs = if self.supertonic_voice_configs.is_empty() {
            defaults.supertonic_settings.voice_configs
        } else {
            self.supertonic_voice_configs
                .into_iter()
                .map(SupertonicVoiceConfig::from)
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
            step_audio_settings: crate::config::StepAudioSettings {
                voice: trimmed_or(
                    self.step_audio_voice,
                    defaults.step_audio_settings.voice.clone(),
                ),
                voice_configs: if self.step_audio_voice_configs.is_empty() {
                    defaults.step_audio_settings.voice_configs
                } else {
                    self.step_audio_voice_configs
                        .into_iter()
                        .map(StepAudioVoiceConfig::from)
                        .collect()
                },
                reference_voice_id: self.step_audio_reference_voice_id,
                use_custom_reference: self.step_audio_use_custom_reference,
                reference_audio_path: self.step_audio_reference_audio_path,
                reference_text: self.step_audio_reference_text,
                reference_label: self.step_audio_reference_label,
                style_prompt: self.step_audio_prompt_text,
            },
            magpie_settings: MagpieSettings {
                voice: magpie_voice,
                voice_configs: magpie_voice_configs,
            },
            kokoro_settings: KokoroSettings {
                voice: kokoro_voice,
                speed: self
                    .kokoro_speed
                    .unwrap_or(defaults.kokoro_settings.speed)
                    .clamp(0.5, 2.0),
                lang: String::new(),
                num_threads: self
                    .kokoro_num_threads
                    .unwrap_or(defaults.kokoro_settings.num_threads)
                    .clamp(1, 8),
                voice_configs: kokoro_voice_configs,
            },
            supertonic_settings: SupertonicSettings {
                speaker_id: defaults.supertonic_settings.speaker_id,
                speed: self
                    .supertonic_speed
                    .unwrap_or(defaults.supertonic_settings.speed)
                    .clamp(0.5, 2.0),
                num_steps: self
                    .supertonic_num_steps
                    .unwrap_or(defaults.supertonic_settings.num_steps)
                    .clamp(1, 20),
                num_threads: self
                    .supertonic_num_threads
                    .unwrap_or(defaults.supertonic_settings.num_threads)
                    .clamp(1, 8),
                lang: String::new(),
                voice_configs: supertonic_voice_configs,
                silence_duration: defaults.supertonic_settings.silence_duration,
                seed: defaults.supertonic_settings.seed,
            },
            vieneu_settings: crate::config::VieneuSettings {
                variant: trimmed_or(self.vieneu_variant, defaults.vieneu_settings.variant),
                emotion: trimmed_or(self.vieneu_emotion, defaults.vieneu_settings.emotion),
                reference_voice_id: self.vieneu_reference_voice_id,
                use_custom_reference: false,
                reference_audio_path: String::new(),
                reference_text: String::new(),
                reference_label: String::new(),
            },
            language_code_override,
        }
    }
}
