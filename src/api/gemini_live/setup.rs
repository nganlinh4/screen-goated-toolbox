//! Typed construction of Gemini Live `setup` envelopes.
//!
//! Endpoint-owned generation policy is applied in [`LiveSetupBuilder::new`].
//! Feature adapters add only the capabilities they actually consume.

use serde_json::{Map, Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaResolution {
    Low,
    High,
}

impl MediaResolution {
    fn api_value(self) -> &'static str {
        match self {
            Self::Low => "MEDIA_RESOLUTION_LOW",
            Self::High => "MEDIA_RESOLUTION_HIGH",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TranscriptionMode {
    #[default]
    None,
    Input,
    Output,
    Both,
}

pub struct LiveSetupBuilder {
    api_model: String,
    max_output_tokens: Option<u32>,
    generation: Value,
    setup: Map<String, Value>,
}

impl LiveSetupBuilder {
    pub fn new(api_model: &str) -> Self {
        let mut generation = json!({ "responseModalities": ["AUDIO"] });
        crate::model_config::apply_live_generation_config(&mut generation, api_model);
        let max_output_tokens = crate::model_config::live_endpoint_profile(api_model)
            .and_then(|profile| profile.max_output_tokens);

        Self {
            api_model: api_model.to_string(),
            max_output_tokens,
            generation,
            setup: Map::new(),
        }
    }

    pub fn media_resolution(mut self, resolution: MediaResolution) -> Self {
        self.generation["mediaResolution"] = resolution.api_value().into();
        self
    }

    pub fn voice(mut self, voice_name: &str) -> Self {
        self.generation["speechConfig"] = json!({
            "voiceConfig": {
                "prebuiltVoiceConfig": { "voiceName": voice_name }
            }
        });
        self
    }

    pub fn thinking_override(mut self, thinking_config: Value) -> Self {
        self.generation["thinkingConfig"] = thinking_config;
        self
    }

    pub fn generation_field(mut self, name: &str, value: Value) -> Self {
        self.generation[name] = value;
        self
    }

    pub fn transcription(mut self, mode: TranscriptionMode) -> Self {
        if matches!(mode, TranscriptionMode::Input | TranscriptionMode::Both) {
            self.setup
                .insert("inputAudioTranscription".to_string(), json!({}));
        }
        if matches!(mode, TranscriptionMode::Output | TranscriptionMode::Both) {
            self.setup
                .insert("outputAudioTranscription".to_string(), json!({}));
        }
        self
    }

    pub fn system_instruction(mut self, instruction: &str) -> Self {
        self.setup.insert(
            "systemInstruction".to_string(),
            json!({ "parts": [{ "text": instruction }] }),
        );
        self
    }

    pub fn context_window_compression(mut self) -> Self {
        self.setup.insert(
            "contextWindowCompression".to_string(),
            json!({ "slidingWindow": {} }),
        );
        self
    }

    pub fn setup_field(mut self, name: &str, value: Value) -> Self {
        self.setup.insert(name.to_string(), value);
        self
    }

    pub fn build(mut self) -> Value {
        if let Some(limit) = self.max_output_tokens {
            // Endpoint policy wins over generic feature-extension fields.
            self.generation["maxOutputTokens"] = limit.into();
        }
        self.setup.insert(
            "model".to_string(),
            json!(format!("models/{}", self.api_model)),
        );
        self.setup
            .insert("generationConfig".to_string(), self.generation);
        json!({ "setup": self.setup })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_config::{GEMINI_LIVE_API_MODEL_2_5, GEMINI_LIVE_API_MODEL_3_1};

    #[test]
    fn endpoint_policy_is_applied_by_construction() {
        let cases = [
            (GEMINI_LIVE_API_MODEL_2_5, 8192, "thinkingBudget"),
            (GEMINI_LIVE_API_MODEL_3_1, 65536, "thinkingLevel"),
        ];

        for (model, limit, thinking_key) in cases {
            let payload = LiveSetupBuilder::new(model).build();
            let generation = &payload["setup"]["generationConfig"];
            assert_eq!(generation["maxOutputTokens"], limit);
            assert!(generation["thinkingConfig"].get(thinking_key).is_some());
        }
    }

    #[test]
    fn feature_fields_stay_explicit() {
        let payload = LiveSetupBuilder::new(GEMINI_LIVE_API_MODEL_3_1)
            .media_resolution(MediaResolution::High)
            .voice("Aoede")
            .transcription(TranscriptionMode::Both)
            .system_instruction("instruction")
            .context_window_compression()
            .build();

        let setup = &payload["setup"];
        assert_eq!(
            setup["generationConfig"]["mediaResolution"],
            "MEDIA_RESOLUTION_HIGH"
        );
        assert_eq!(
            setup["generationConfig"]["speechConfig"]["voiceConfig"]["prebuiltVoiceConfig"]["voiceName"],
            "Aoede"
        );
        assert_eq!(setup["inputAudioTranscription"], json!({}));
        assert_eq!(setup["outputAudioTranscription"], json!({}));
        assert_eq!(
            setup["systemInstruction"]["parts"][0]["text"],
            "instruction"
        );
        assert_eq!(
            setup["contextWindowCompression"],
            json!({ "slidingWindow": {} })
        );
    }

    #[test]
    fn generic_extensions_cannot_replace_endpoint_identity_or_output_policy() {
        let payload = LiveSetupBuilder::new(GEMINI_LIVE_API_MODEL_3_1)
            .generation_field("maxOutputTokens", json!(1))
            .setup_field("model", json!("models/wrong"))
            .build();

        assert_eq!(
            payload["setup"]["model"],
            format!("models/{GEMINI_LIVE_API_MODEL_3_1}")
        );
        assert_eq!(
            payload["setup"]["generationConfig"]["maxOutputTokens"],
            65536
        );
    }
}
