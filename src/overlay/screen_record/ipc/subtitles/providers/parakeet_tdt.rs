use crate::api::audio::extract_pcm_from_wav;
use crate::api::realtime_audio::parakeet_tdt_assets::get_parakeet_tdt_model_dir;
use crate::overlay::screen_record::ipc::subtitles::types::CompactSubtitleSegment;
use parakeet_rs::{
    ExecutionConfig, ExecutionProvider, ParakeetTDT, TimedToken, TimestampMode, Transcriber,
};
use std::sync::atomic::Ordering;

use super::{
    SubtitleBackend, SubtitleBackendProgress, SubtitleBackendRequest, ends_sentence,
    join_word_tokens, normalize_subtitle_text,
};

const SAMPLE_RATE_HZ: usize = 16_000;
const CHUNK_SEC: f64 = 30.0;
const MAX_SEGMENT_SEC: f64 = 6.5;
const MAX_SEGMENT_CHARS: usize = 96;
const MAX_SEGMENT_WORDS: usize = 16;

pub struct ParakeetTdtSubtitleBackend {
    model: Option<ParakeetTDT>,
}

impl ParakeetTdtSubtitleBackend {
    pub fn new() -> Self {
        Self { model: None }
    }

    fn model(&mut self) -> Result<&mut ParakeetTDT, String> {
        if self.model.is_none() {
            let model_dir = get_parakeet_tdt_model_dir();
            let config = ExecutionConfig::new()
                .with_execution_provider(ExecutionProvider::DirectML)
                .with_intra_threads(4)
                .with_inter_threads(1);
            self.model = Some(
                ParakeetTDT::from_pretrained(&model_dir, Some(config))
                    .map_err(|err| format!("Load Parakeet TDT subtitle model: {err}"))?,
            );
        }
        self.model
            .as_mut()
            .ok_or_else(|| "Parakeet TDT model failed to initialize".to_string())
    }
}

impl SubtitleBackend for ParakeetTdtSubtitleBackend {
    fn transcribe_clip(
        &mut self,
        request: SubtitleBackendRequest,
        on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
    ) -> Result<Vec<CompactSubtitleSegment>, String> {
        if request.media.mime_type != "audio/wav" {
            return Err(format!(
                "Parakeet TDT subtitles require audio/wav input, got {}",
                request.media.mime_type
            ));
        }

        let samples = extract_pcm_from_wav(&request.media.bytes)
            .map_err(|err| format!("Extract WAV PCM for Parakeet TDT subtitles: {err}"))?;
        if samples.is_empty() {
            return Ok(Vec::new());
        }

        let total_samples = samples.len();
        let chunk_samples = (CHUNK_SEC * SAMPLE_RATE_HZ as f64).round() as usize;
        let total_chunks = total_samples.div_ceil(chunk_samples).max(1);
        let mut all_segments = Vec::new();

        crate::log_info!(
            "[SubtitleGen][ParakeetTDT] start samples={} chunks={} chunk_sec={:.1}",
            total_samples,
            total_chunks,
            CHUNK_SEC
        );

        for chunk_index in 0..total_chunks {
            if request.cancel_token.load(Ordering::SeqCst) {
                return Err("Parakeet TDT subtitle generation cancelled".to_string());
            }

            let start_sample = chunk_index * chunk_samples;
            let end_sample = ((chunk_index + 1) * chunk_samples).min(total_samples);
            if start_sample >= end_sample {
                continue;
            }

            let chunk_offset_sec = start_sample as f64 / SAMPLE_RATE_HZ as f64;
            let audio = samples[start_sample..end_sample]
                .iter()
                .map(|sample| *sample as f32 / i16::MAX as f32)
                .collect::<Vec<_>>();
            let transcribed = self
                .model()?
                .transcribe_samples(audio, SAMPLE_RATE_HZ as u32, 1, Some(TimestampMode::Words))
                .map_err(|err| format!("Transcribe Parakeet TDT chunk: {err}"))?;

            all_segments.extend(words_to_segments(&transcribed.tokens, chunk_offset_sec));
            on_progress(SubtitleBackendProgress {
                completed_steps: chunk_index + 1,
                total_steps: total_chunks,
                segments: all_segments.clone(),
            })?;
        }

        Ok(all_segments)
    }
}

fn words_to_segments(words: &[TimedToken], offset_sec: f64) -> Vec<CompactSubtitleSegment> {
    let mut segments = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut start_time: Option<f64> = None;
    let mut end_time = 0.0;

    for word in words {
        let text = normalize_subtitle_text(&word.text);
        if text.is_empty() {
            continue;
        }

        let word_start = offset_sec + word.start.max(0.0) as f64;
        let word_end = offset_sec + word.end.max(word.start + 0.01) as f64;
        if current.is_empty() {
            start_time = Some(word_start);
        }

        let proposed_words = current.len() + 1;
        let proposed_text = if current.is_empty() {
            text.clone()
        } else {
            let next = current
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(text.as_str()))
                .collect::<Vec<_>>();
            join_word_tokens(&next)
        };
        let proposed_start = start_time.unwrap_or(word_start);
        let proposed_duration = word_end - proposed_start;
        let should_flush = !current.is_empty()
            && (proposed_duration > MAX_SEGMENT_SEC
                || proposed_text.chars().count() > MAX_SEGMENT_CHARS
                || proposed_words > MAX_SEGMENT_WORDS);

        if should_flush {
            push_segment(&mut segments, start_time, end_time, &current);
            current.clear();
            start_time = Some(word_start);
        }

        current.push(text);
        end_time = word_end;

        if ends_sentence(current.last().map(String::as_str).unwrap_or_default()) {
            push_segment(&mut segments, start_time, end_time, &current);
            current.clear();
            start_time = None;
        }
    }

    push_segment(&mut segments, start_time, end_time, &current);
    segments
}

fn push_segment(
    segments: &mut Vec<CompactSubtitleSegment>,
    start_time: Option<f64>,
    end_time: f64,
    words: &[String],
) {
    let Some(start_time) = start_time else {
        return;
    };
    let word_refs = words.iter().map(String::as_str).collect::<Vec<_>>();
    let text = join_word_tokens(&word_refs);
    if text.trim().is_empty() || end_time <= start_time {
        return;
    }
    segments.push(CompactSubtitleSegment {
        start_time,
        end_time,
        text,
    });
}
