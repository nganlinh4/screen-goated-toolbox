use crate::api::realtime_audio::qwen3::Qwen3ModelVariant;
use crate::api::realtime_audio::qwen3::{assets, reference};
use crate::overlay::screen_record::ipc::subtitles::wav_chunks::split_subtitle_wav_into_chunks;

use super::{
    SubtitleBackend, SubtitleBackendProgress, normalize_qwen_language_hint, normalize_subtitle_text,
};
use crate::overlay::screen_record::ipc::subtitles::audio::MIN_SUBTITLE_DURATION_SEC;
use crate::overlay::screen_record::ipc::subtitles::types::CompactSubtitleSegment;

pub struct QwenSubtitleBackend {
    server: reference::QwenReferenceServer,
}

impl QwenSubtitleBackend {
    pub fn new(variant: Qwen3ModelVariant) -> Result<Self, String> {
        let (model_dir, is_downloaded, model_label) = match variant {
            Qwen3ModelVariant::Small => (
                assets::get_qwen3_model_dir(),
                assets::is_qwen3_model_downloaded(),
                "Qwen3-ASR 0.6B",
            ),
            Qwen3ModelVariant::Large => (
                assets::get_qwen3_1_7b_model_dir(),
                assets::is_qwen3_1_7b_model_downloaded(),
                "Qwen3-ASR 1.7B",
            ),
        };
        if !is_downloaded {
            return Err(format!(
                "Qwen Local subtitles require the {model_label} model from Downloaded Tools."
            ));
        }

        let server = reference::QwenReferenceServer::start(&model_dir)
            .map_err(|err| format!("Failed to start Qwen Local subtitle backend: {err}"))?;
        Ok(Self { server })
    }
}

impl SubtitleBackend for QwenSubtitleBackend {
    fn transcribe_clip(
        &mut self,
        audio_data: Vec<u8>,
        language_hint: Option<&str>,
        on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
    ) -> Result<Vec<CompactSubtitleSegment>, String> {
        let chunks = split_subtitle_wav_into_chunks(&audio_data)?;
        if chunks.is_empty() {
            return Ok(Vec::new());
        }
        let total_steps = chunks.len();
        let mut combined_segments = Vec::new();
        let normalized_language_hint = normalize_qwen_language_hint(language_hint);

        for (index, chunk) in chunks.into_iter().enumerate() {
            let response = self
                .server
                .transcribe_audio_verbose(chunk.wav_data, normalized_language_hint.as_deref())
                .map_err(|err| format!("Qwen Local subtitle request failed: {err}"))?;
            let mut chunk_segments = qwen_segments_from_response(response);
            for segment in &mut chunk_segments {
                segment.start_time += chunk.start_time_sec;
                segment.end_time += chunk.start_time_sec;
            }
            combined_segments.extend(chunk_segments);
            on_progress(SubtitleBackendProgress {
                completed_steps: index + 1,
                total_steps,
                segments: combined_segments.clone(),
            })?;
        }

        Ok(combined_segments)
    }
}

fn qwen_segments_from_response(
    response: reference::VerboseTranscriptionResponse,
) -> Vec<CompactSubtitleSegment> {
    let _detected_language = response.language.clone();
    let mut segments = response
        .segments
        .into_iter()
        .filter_map(|segment| {
            let text = normalize_subtitle_text(&segment.text);
            if text.is_empty() {
                return None;
            }
            Some(CompactSubtitleSegment {
                start_time: segment.start.max(0.0),
                end_time: segment.end.max(segment.start + MIN_SUBTITLE_DURATION_SEC),
                text,
            })
        })
        .collect::<Vec<_>>();

    if segments.is_empty() {
        let text = normalize_subtitle_text(&response.text);
        if !text.is_empty() && response.duration > 0.0 {
            segments.push(CompactSubtitleSegment {
                start_time: 0.0,
                end_time: response.duration.max(MIN_SUBTITLE_DURATION_SEC),
                text,
            });
        }
    }

    segments
}
