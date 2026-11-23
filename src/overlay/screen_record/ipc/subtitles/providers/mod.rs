mod gemini;
mod gemini_segments;
mod gemini_stream;
mod groq;
mod groq_diagnostics;
mod language;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::APP;
use crate::model_config::get_model_by_id;

use super::media::PreparedSubtitleMedia;
use super::types::{CompactSubtitleSegment, SubtitleGenerationMethod, SubtitleMethodCapability};

pub use self::language::{
    ends_sentence, join_word_tokens, normalize_groq_language_hint, normalize_subtitle_text,
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
    ]
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
