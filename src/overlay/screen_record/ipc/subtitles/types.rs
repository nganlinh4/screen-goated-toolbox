use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum SubtitleGenerationMethod {
    #[default]
    GroqWhisperAccurate,
    GroqWhisperLargeV3Turbo,
    #[serde(rename = "gemini-3-1-flash-lite")]
    Gemini3_1FlashLite,
    #[serde(rename = "gemini-3-flash-preview")]
    Gemini3FlashPreview,
    #[serde(rename = "qwen-local-0-6b", alias = "qwen-local")]
    QwenLocal0_6B,
    #[serde(rename = "qwen-local-1-7b")]
    QwenLocal1_7B,
    #[serde(rename = "parakeet-tdt-0-6b-v3")]
    ParakeetTdt0_6BV3,
}

#[derive(Clone, Deserialize)]
pub struct SubtitleGenerationRequest {
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "languageHint")]
    pub language_hint: Option<String>,
    #[serde(rename = "geminiPrompt")]
    pub gemini_prompt: Option<String>,
    #[serde(rename = "groqVocabulary", default)]
    pub groq_vocabulary: Vec<String>,
    #[serde(rename = "subtitleMethod", default)]
    pub subtitle_method: SubtitleGenerationMethod,
    pub clips: Vec<SubtitleClipRequest>,
}

#[derive(Clone, Deserialize)]
pub struct SubtitleClipRequest {
    #[serde(rename = "clipId")]
    pub clip_id: String,
    #[serde(rename = "clipName")]
    pub clip_name: String,
    #[serde(rename = "sourcePath")]
    pub source_path: String,
    #[serde(rename = "sourceDuration")]
    pub source_duration: f64,
    #[serde(rename = "trimSegments")]
    pub trim_segments: Vec<SubtitleTrimSegment>,
    #[serde(rename = "micAudioOffsetSec")]
    pub mic_audio_offset_sec: Option<f64>,
}

#[derive(Clone, Deserialize)]
pub struct SubtitleTrimSegment {
    #[serde(rename = "startTime")]
    pub start_time: f64,
    #[serde(rename = "endTime")]
    pub end_time: f64,
}

#[derive(Clone, Serialize, Default)]
pub struct SubtitleJobSnapshot {
    pub state: String,
    pub message: String,
    #[serde(rename = "messageKey")]
    pub message_key: Option<String>,
    #[serde(rename = "messageParams")]
    pub message_params: HashMap<String, String>,
    pub progress: f64,
    #[serde(rename = "activeClipId")]
    pub active_clip_id: Option<String>,
    #[serde(rename = "totalClips")]
    pub total_clips: usize,
    #[serde(rename = "completedClips")]
    pub completed_clips: usize,
    #[serde(rename = "resultsRevision")]
    pub results_revision: usize,
    pub results: Vec<SubtitleClipResult>,
    #[serde(skip_serializing, skip_deserializing)]
    pub result_events: Vec<SubtitleClipResultEvent>,
    pub skipped: Vec<SubtitleSkippedClip>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct SubtitleClipResultEvent {
    pub revision: usize,
    pub result: SubtitleClipResult,
}

#[derive(Clone, Serialize, PartialEq)]
pub struct SubtitleClipResult {
    #[serde(rename = "clipId")]
    pub clip_id: String,
    #[serde(rename = "isPartial")]
    pub is_partial: bool,
    pub segments: Vec<SubtitleSegmentResult>,
}

#[derive(Clone, Serialize, PartialEq)]
pub struct SubtitleSegmentResult {
    #[serde(rename = "startTime")]
    pub start_time: f64,
    #[serde(rename = "endTime")]
    pub end_time: f64,
    pub text: String,
}

#[derive(Clone, Serialize)]
pub struct SubtitleSkippedClip {
    #[serde(rename = "clipId")]
    pub clip_id: String,
    pub reason: String,
}

#[derive(Clone, Serialize)]
pub struct SubtitleGenerationCapabilities {
    pub methods: Vec<SubtitleMethodCapability>,
}

#[derive(Clone, Serialize)]
pub struct SubtitleMethodCapability {
    pub method: SubtitleGenerationMethod,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CompactSubtitleSegment {
    pub start_time: f64,
    pub end_time: f64,
    pub text: String,
}

#[derive(Clone, Deserialize)]
pub struct SubtitleTranslationRequest {
    #[serde(rename = "targetLanguage")]
    pub target_language: String,
    #[serde(rename = "modelId")]
    pub model_id: Option<String>,
    #[serde(rename = "chunkMode")]
    pub chunk_mode: Option<String>,
    #[serde(rename = "chunkCount")]
    pub chunk_count: Option<usize>,
    #[serde(rename = "smartFallback", default)]
    pub smart_fallback: bool,
    pub instructions: Option<String>,
    pub items: Vec<SubtitleTranslationItemRequest>,
}

#[derive(Clone, Deserialize)]
pub struct SubtitleTranslationItemRequest {
    pub id: String,
    #[serde(rename = "clipId")]
    pub clip_id: Option<String>,
    #[serde(rename = "sourceGroupId")]
    pub source_group_id: Option<String>,
    #[serde(rename = "sourceName")]
    pub source_name: Option<String>,
    pub text: String,
}

#[derive(Clone, Serialize, Default)]
pub struct SubtitleTranslationJobSnapshot {
    pub state: String,
    pub message: String,
    #[serde(rename = "messageKey")]
    pub message_key: Option<String>,
    #[serde(rename = "messageParams")]
    pub message_params: HashMap<String, String>,
    pub progress: f64,
    #[serde(rename = "currentModelId")]
    pub current_model_id: Option<String>,
    #[serde(rename = "currentModelLabel")]
    pub current_model_label: Option<String>,
    #[serde(rename = "currentChunkCount")]
    pub current_chunk_count: usize,
    #[serde(rename = "currentChunkIndex")]
    pub current_chunk_index: usize,
    #[serde(rename = "totalChunks")]
    pub total_chunks: usize,
    #[serde(rename = "targetLanguage")]
    pub target_language: Option<String>,
    pub results: Vec<SubtitleTranslationResultItem>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SubtitleTranslationResultItem {
    pub id: String,
    #[serde(rename = "clipId")]
    pub clip_id: Option<String>,
    #[serde(rename = "translatedText")]
    pub translated_text: String,
}

#[derive(Clone, Serialize)]
pub struct SubtitleTranslationCapabilities {
    pub available: bool,
    pub reason: Option<String>,
    pub models: Vec<SubtitleTranslationModelCapability>,
}

#[derive(Clone, Serialize)]
pub struct SubtitleTranslationModelCapability {
    #[serde(rename = "modelId")]
    pub model_id: String,
    #[serde(rename = "modelLabel")]
    pub model_label: String,
    #[serde(rename = "modelName")]
    pub model_name: String,
    pub provider: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: Option<u8>,
    #[serde(rename = "typicalLatencyMs")]
    pub typical_latency_ms: Option<u32>,
    #[serde(rename = "performanceSource")]
    pub performance_source: Option<String>,
}
