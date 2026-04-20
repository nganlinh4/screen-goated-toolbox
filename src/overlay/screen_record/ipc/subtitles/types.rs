use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum SubtitleGenerationMethod {
    #[default]
    GroqWhisperAccurate,
    #[serde(rename = "qwen-local-0-6b", alias = "qwen-local")]
    QwenLocal0_6B,
    #[serde(rename = "qwen-local-1-7b")]
    QwenLocal1_7B,
}

#[derive(Clone, Deserialize)]
pub struct SubtitleGenerationRequest {
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "languageHint")]
    pub language_hint: Option<String>,
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
    pub results: Vec<SubtitleClipResult>,
    pub skipped: Vec<SubtitleSkippedClip>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct SubtitleClipResult {
    #[serde(rename = "clipId")]
    pub clip_id: String,
    #[serde(rename = "isPartial")]
    pub is_partial: bool,
    pub segments: Vec<SubtitleSegmentResult>,
}

#[derive(Clone, Serialize)]
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

#[derive(Clone)]
pub struct CompactSubtitleSegment {
    pub start_time: f64,
    pub end_time: f64,
    pub text: String,
}
