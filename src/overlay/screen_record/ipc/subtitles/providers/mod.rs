mod gemini;
mod gemini_segments;
mod gemini_stream;
mod groq;
mod groq_diagnostics;
mod language;
mod parakeet_tdt;
mod qwen;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::APP;
use crate::api::realtime_audio::parakeet_tdt_assets;
use crate::api::realtime_audio::qwen3::{Qwen3ModelVariant, assets, runtime};
use crate::model_config::get_model_by_id;
use crate::unpack_dlls::{self, AiRuntimeStatus};

use super::media::PreparedSubtitleMedia;
use super::types::{CompactSubtitleSegment, SubtitleGenerationMethod, SubtitleMethodCapability};

pub use self::language::{
    ends_sentence, join_word_tokens, normalize_groq_language_hint, normalize_qwen_language_hint,
    normalize_subtitle_text,
};

pub struct SubtitleBackendProgress {
    pub completed_steps: usize,
    pub total_steps: usize,
    pub segments: Vec<CompactSubtitleSegment>,
}

pub struct SubtitleBackendRequest {
    pub media: PreparedSubtitleMedia,
    pub language_hint: Option<String>,
    pub gemini_prompt: Option<String>,
    pub groq_vocabulary: Vec<String>,
    pub cancel_token: Arc<AtomicBool>,
}

pub trait SubtitleBackend {
    fn transcribe_clip(
        &mut self,
        request: SubtitleBackendRequest,
        on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
    ) -> Result<Vec<CompactSubtitleSegment>, String>;
}

pub fn create_backend(
    method: SubtitleGenerationMethod,
) -> Result<Box<dyn SubtitleBackend>, String> {
    match method {
        SubtitleGenerationMethod::GroqWhisperAccurate => {
            Ok(Box::new(groq::GroqSubtitleBackend::new(method)?))
        }
        SubtitleGenerationMethod::GroqWhisperLargeV3Turbo => {
            Ok(Box::new(groq::GroqSubtitleBackend::new(method)?))
        }
        SubtitleGenerationMethod::Gemini3_1FlashLite => {
            Ok(Box::new(gemini::GeminiSubtitleBackend::new(method)?))
        }
        SubtitleGenerationMethod::Gemini3FlashPreview => {
            Ok(Box::new(gemini::GeminiSubtitleBackend::new(method)?))
        }
        SubtitleGenerationMethod::QwenLocal0_6B => Ok(Box::new(qwen::QwenSubtitleBackend::new(
            Qwen3ModelVariant::Small,
        )?)),
        SubtitleGenerationMethod::QwenLocal1_7B => Ok(Box::new(qwen::QwenSubtitleBackend::new(
            Qwen3ModelVariant::Large,
        )?)),
        SubtitleGenerationMethod::ParakeetTdt0_6BV3 => {
            Ok(Box::new(parakeet_tdt::ParakeetTdtSubtitleBackend::new()))
        }
    }
}

pub fn capabilities() -> Vec<SubtitleMethodCapability> {
    vec![
        SubtitleMethodCapability {
            method: SubtitleGenerationMethod::GroqWhisperAccurate,
            available: true,
            reason: None,
        },
        SubtitleMethodCapability {
            method: SubtitleGenerationMethod::GroqWhisperLargeV3Turbo,
            available: true,
            reason: None,
        },
        gemini_subtitle_capability(SubtitleGenerationMethod::Gemini3_1FlashLite),
        gemini_subtitle_capability(SubtitleGenerationMethod::Gemini3FlashPreview),
        qwen_local_capability(
            SubtitleGenerationMethod::QwenLocal0_6B,
            Qwen3ModelVariant::Small,
        ),
        qwen_local_capability(
            SubtitleGenerationMethod::QwenLocal1_7B,
            Qwen3ModelVariant::Large,
        ),
        parakeet_tdt_capability(),
    ]
}

fn qwen_local_capability(
    method: SubtitleGenerationMethod,
    variant: Qwen3ModelVariant,
) -> SubtitleMethodCapability {
    let model_label = match variant {
        Qwen3ModelVariant::Small => "Qwen3-ASR 0.6B",
        Qwen3ModelVariant::Large => "Qwen3-ASR 1.7B",
    };
    let model_downloaded = match variant {
        Qwen3ModelVariant::Small => assets::is_qwen3_model_downloaded(),
        Qwen3ModelVariant::Large => assets::is_qwen3_1_7b_model_downloaded(),
    };
    if !model_downloaded {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(format!(
                "The {model_label} model will be downloaded when this subtitle method is selected."
            )),
        };
    }
    if !runtime::has_discoverable_qwen3_runtime() {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(
                "The Qwen3-ASR CUDA Runtime will be downloaded when this subtitle method is selected."
                    .to_string(),
            ),
        };
    }
    SubtitleMethodCapability {
        method,
        available: true,
        reason: None,
    }
}

fn parakeet_tdt_capability() -> SubtitleMethodCapability {
    let method = SubtitleGenerationMethod::ParakeetTdt0_6BV3;
    let worker_status = crate::component_registry::local_asr::current_status(
        crate::component_registry::local_asr::ComponentKind::Worker,
    );
    if !crate::component_registry::local_asr::status_is_ready(&worker_status) {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(
                "The local speech recognition engine will be downloaded when Parakeet subtitles are selected."
                    .to_string(),
            ),
        };
    }
    if !parakeet_tdt_assets::is_parakeet_tdt_model_downloaded() {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(
                "The Parakeet TDT 0.6B v3 model will be downloaded when this subtitle method is selected."
                    .to_string(),
            ),
        };
    }
    if !matches!(
        unpack_dlls::current_ai_runtime_status(),
        AiRuntimeStatus::Installed
    ) {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(
                "The local AI runtime will be downloaded when Parakeet subtitles are selected."
                    .to_string(),
            ),
        };
    }
    SubtitleMethodCapability {
        method,
        available: true,
        reason: None,
    }
}

fn gemini_subtitle_capability(method: SubtitleGenerationMethod) -> SubtitleMethodCapability {
    let app = match APP.lock() {
        Ok(app) => app,
        Err(_) => {
            return SubtitleMethodCapability {
                method,
                available: false,
                reason: Some("APP lock poisoned".to_string()),
            };
        }
    };
    if !app.config.use_gemini {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some("Enable Gemini in Settings to use Gemini subtitles.".to_string()),
        };
    }
    if app.config.gemini_api_key.trim().is_empty() {
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some("Add a Gemini API key in Settings to use Gemini subtitles.".to_string()),
        };
    }
    let model_id = gemini::gemini_subtitle_model_id(method);
    if get_model_by_id(model_id).is_none() {
        let model_label = match method {
            SubtitleGenerationMethod::Gemini3_1FlashLite => "Gemini 3.1 Flash Lite",
            SubtitleGenerationMethod::Gemini3FlashPreview => "Gemini 3 Flash Preview",
            _ => "Gemini",
        };
        return SubtitleMethodCapability {
            method,
            available: false,
            reason: Some(format!("{model_label} subtitle model config is missing.")),
        };
    }
    SubtitleMethodCapability {
        method,
        available: true,
        reason: None,
    }
}
