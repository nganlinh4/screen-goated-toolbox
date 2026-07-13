use serde::Deserialize;
use std::io::Cursor;
use std::sync::atomic::Ordering;

use crate::APP;
use crate::api::client::{UREQ_AGENT, record_usage_simple};
use crate::model_config::get_model_by_id;
use crate::overlay::screen_record::ipc::subtitles::types::SubtitleGenerationMethod;

use super::groq_diagnostics::GroqTranscriptDiagnostics;
use super::{
    SubtitleBackend, SubtitleBackendProgress, SubtitleBackendRequest, ends_sentence,
    join_word_tokens, normalize_groq_language_hint, normalize_subtitle_text,
};
use crate::overlay::screen_record::ipc::subtitles::audio::{
    MIN_SUBTITLE_DURATION_SEC, build_silence_aware_split_frames,
};
use crate::overlay::screen_record::ipc::subtitles::types::CompactSubtitleSegment;

const GROQ_AUDIO_TRANSCRIPT_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const SENTENCE_BREAK_SILENCE_SEC: f64 = 0.45;
const MAX_GROQ_SPLIT_PARTS: usize = 128;
// Target audio duration per chunk to keep each request comfortably below the
// shared `UREQ_AGENT` 120 s timeout (upload + Whisper processing + response).
const GROQ_TARGET_CHUNK_SEC: f64 = 90.0;
// How far each side of an even-split boundary to scan for a quieter cut point so
// chunks end on natural silence rather than mid-word.
const GROQ_SILENCE_SEARCH_RADIUS_SEC: f64 = 5.0;

pub struct GroqSubtitleBackend {
    api_key: String,
    model_name: String,
}

impl GroqSubtitleBackend {
    pub fn new(method: SubtitleGenerationMethod) -> Result<Self, String> {
        let model_id = match method {
            SubtitleGenerationMethod::GroqWhisperAccurate => "whisper-accurate",
            SubtitleGenerationMethod::GroqWhisperLargeV3Turbo => "whisper-fast",
            _ => return Err("Unsupported Groq subtitle method".to_string()),
        };
        let (api_key, model_name) = {
            let app = APP.lock().map_err(|_| "APP lock poisoned".to_string())?;
            let model = get_model_by_id(model_id)
                .ok_or_else(|| format!("{model_id} model config missing"))?;
            (app.config.api_key.clone(), model.full_name)
        };
        if api_key.trim().is_empty() {
            return Err("NO_API_KEY:groq".to_string());
        }

        Ok(Self {
            api_key,
            model_name,
        })
    }
}

impl SubtitleBackend for GroqSubtitleBackend {
    fn transcribe_clip(
        &mut self,
        request: SubtitleBackendRequest,
        on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
    ) -> Result<Vec<CompactSubtitleSegment>, String> {
        if request.media.mime_type != "audio/wav" {
            return Err(format!(
                "Groq subtitles require audio/wav input, got {}",
                request.media.mime_type
            ));
        }
        transcribe_with_groq_auto_split(
            &self.api_key,
            &self.model_name,
            &request.media.bytes,
            request.language_hint.as_deref(),
            &request.groq_vocabulary,
            &request.cancel_token,
            on_progress,
        )
    }
}

#[derive(Clone)]
struct GroqWavAudio {
    samples: Vec<i16>,
    sample_rate: u32,
    channels: u16,
}

#[derive(Deserialize)]
struct GroqVerboseResponse {
    segments: Option<Vec<GroqSegment>>,
    words: Option<Vec<GroqWord>>,
}

#[derive(Clone, Deserialize)]
struct GroqSegment {
    start: f64,
    end: f64,
    text: String,
    avg_logprob: Option<f64>,
    no_speech_prob: Option<f64>,
    compression_ratio: Option<f64>,
}

#[derive(Clone, Deserialize)]
struct GroqWord {
    start: f64,
    end: f64,
    word: String,
}

enum GroqRequestError {
    TooLarge(String),
    Other(String),
}

impl GroqRequestError {
    fn message(&self) -> &str {
        match self {
            Self::TooLarge(message) | Self::Other(message) => message,
        }
    }
}

fn transcribe_with_groq_auto_split(
    api_key: &str,
    model_name: &str,
    audio_data: &[u8],
    language_hint: Option<&str>,
    vocabulary: &[String],
    cancel_token: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
) -> Result<Vec<CompactSubtitleSegment>, String> {
    let wav = decode_wav_audio(audio_data)?;
    if wav.samples.is_empty() {
        return Ok(Vec::new());
    }

    let duration_sec = wav.duration_sec();
    let mut split_parts =
        ((duration_sec / GROQ_TARGET_CHUNK_SEC).ceil() as usize).clamp(1, MAX_GROQ_SPLIT_PARTS);
    loop {
        if cancel_token.load(Ordering::SeqCst) {
            return Err("Groq subtitle generation cancelled".to_string());
        }
        crate::log_info!(
            "[SubtitleGen][Groq] transcribe attempt split_parts={} bytes={} duration_sec={:.2}",
            split_parts,
            audio_data.len(),
            wav.duration_sec()
        );

        match transcribe_groq_split_attempt(
            GroqSplitAttempt {
                api_key,
                model_name,
                wav: &wav,
                language_hint,
                vocabulary,
                cancel_token,
            },
            split_parts,
            on_progress,
        ) {
            Ok(segments) => return Ok(segments),
            Err(GroqRequestError::TooLarge(message)) => {
                if split_parts >= MAX_GROQ_SPLIT_PARTS {
                    return Err(format!(
                        "Groq subtitle request still failed after splitting into {split_parts} parts: {message}"
                    ));
                }
                split_parts += 1;
                crate::log_info!(
                    "[SubtitleGen][Groq] retry needed; splitting into {} parts ({})",
                    split_parts,
                    message
                );
            }
            Err(GroqRequestError::Other(message)) => return Err(message),
        }
    }
}

struct GroqSplitAttempt<'a> {
    api_key: &'a str,
    model_name: &'a str,
    wav: &'a GroqWavAudio,
    language_hint: Option<&'a str>,
    vocabulary: &'a [String],
    cancel_token: &'a std::sync::Arc<std::sync::atomic::AtomicBool>,
}

fn transcribe_groq_split_attempt(
    request: GroqSplitAttempt<'_>,
    split_parts: usize,
    on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
) -> Result<Vec<CompactSubtitleSegment>, GroqRequestError> {
    let GroqSplitAttempt {
        api_key,
        model_name,
        wav,
        language_hint,
        vocabulary,
        cancel_token,
    } = request;
    let chunk_ranges = build_silence_aware_split_frames(
        &wav.samples,
        wav.channels as usize,
        wav.sample_rate,
        split_parts,
        GROQ_SILENCE_SEARCH_RADIUS_SEC,
    );
    let total_parts = chunk_ranges.len();
    let mut all_segments = Vec::new();
    for (part_index, (start_frame, end_frame)) in chunk_ranges.into_iter().enumerate() {
        if cancel_token.load(Ordering::SeqCst) {
            return Err(GroqRequestError::Other(
                "Groq subtitle generation cancelled".to_string(),
            ));
        }

        let chunk = wav.chunk_from_frames(start_frame, end_frame);
        if chunk.samples.is_empty() {
            continue;
        }
        let chunk_wav = encode_wav(&chunk.samples, wav.sample_rate, wav.channels)
            .map_err(GroqRequestError::Other)?;
        crate::log_info!(
            "[SubtitleGen][Groq] part-start {}/{} offset={:.2}s duration={:.2}s bytes={}",
            part_index + 1,
            total_parts,
            chunk.offset_sec,
            chunk.duration_sec,
            chunk_wav.len()
        );
        let mut response = transcribe_with_groq_verbose(
            api_key,
            model_name,
            chunk_wav.clone(),
            language_hint,
            vocabulary,
        )?;
        let diagnostics = transcript_diagnostics(&response);
        diagnostics.log(model_name, part_index + 1, total_parts);

        // Retry a suspicious Turbo chunk once with the accurate model. Keep the
        // original successful transcript if the diagnostic retry itself fails.
        if model_name == "whisper-large-v3-turbo" && diagnostics.should_retry() {
            if cancel_token.load(Ordering::SeqCst) {
                return Err(GroqRequestError::Other(
                    "Groq subtitle generation cancelled".to_string(),
                ));
            }
            crate::log_info!(
                "[SubtitleGen][Groq] part-retry {}/{} reason=quality model=whisper-large-v3",
                part_index + 1,
                total_parts
            );
            match transcribe_with_groq_verbose(
                api_key,
                "whisper-large-v3",
                chunk_wav,
                language_hint,
                vocabulary,
            ) {
                Ok(accurate) => {
                    transcript_diagnostics(&accurate).log(
                        "whisper-large-v3",
                        part_index + 1,
                        total_parts,
                    );
                    response = accurate;
                }
                Err(error) => crate::log_info!(
                    "[SubtitleGen][Groq] diagnostic retry failed; keeping original: {}",
                    error.message()
                ),
            }
        }
        let mut segments = build_sentence_blocks(&response);
        for segment in &mut segments {
            segment.start_time += chunk.offset_sec;
            segment.end_time += chunk.offset_sec;
        }
        crate::log_info!(
            "[SubtitleGen][Groq] part-complete {}/{} added_segments={}",
            part_index + 1,
            total_parts,
            segments.len()
        );
        all_segments.extend(segments);

        // Stream the accumulated transcript to the timeline as soon as each
        // part finishes, instead of waiting for the whole job.
        on_progress(SubtitleBackendProgress {
            completed_steps: part_index + 1,
            total_steps: total_parts,
            segments: all_segments.clone(),
        })
        .map_err(GroqRequestError::Other)?;
    }
    Ok(all_segments)
}

fn transcribe_with_groq_verbose(
    api_key: &str,
    model_name: &str,
    audio_data: Vec<u8>,
    language_hint: Option<&str>,
    vocabulary: &[String],
) -> Result<GroqVerboseResponse, GroqRequestError> {
    let boundary = format!("----SGTSubtitle{}", chrono::Utc::now().timestamp_millis());
    let mut body = Vec::new();
    add_multipart_field(&mut body, &boundary, "model", model_name.as_bytes());
    add_multipart_field(&mut body, &boundary, "response_format", b"verbose_json");
    add_multipart_field(
        &mut body,
        &boundary,
        "timestamp_granularities[]",
        b"segment",
    );
    add_multipart_field(&mut body, &boundary, "timestamp_granularities[]", b"word");
    if let Some(language) = normalize_groq_language_hint(language_hint) {
        add_multipart_field(&mut body, &boundary, "language", language.as_bytes());
    }
    if let Some(prompt) = build_groq_vocabulary_prompt(vocabulary) {
        add_multipart_field(&mut body, &boundary, "prompt", prompt.as_bytes());
    }
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"subtitle-source.wav\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(&audio_data);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

    let response = UREQ_AGENT
        .post(GROQ_AUDIO_TRANSCRIPT_URL)
        .header("Authorization", &format!("Bearer {}", api_key))
        .header(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send(&body)
        .map_err(map_groq_request_error)?;

    record_usage_simple(response.headers(), model_name);

    let json: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|e| GroqRequestError::Other(format!("Parse Groq subtitle response: {e}")))?;
    serde_json::from_value(json)
        .map_err(|e| GroqRequestError::Other(format!("Decode Groq verbose response: {e}")))
}

fn map_groq_request_error(error: ureq::Error) -> GroqRequestError {
    let message = error.to_string();
    let lowered = message.to_ascii_lowercase();
    if message.contains("413") || lowered.contains("request entity too large") {
        GroqRequestError::TooLarge(format!("Groq subtitle request failed: {message}"))
    } else if lowered.contains("os error 10053")
        || lowered.contains("os error 10054")
        || lowered.contains("connectionaborted")
        || lowered.contains("connection abort")
        || lowered.contains("connection reset")
        || lowered.contains("timed out")
        || lowered.contains("timeout")
    {
        // Treat connection drops and timeouts as a "split smaller and retry"
        // signal: Groq's response just took longer than the shared HTTP
        // client's global timeout, so smaller chunks fit the budget.
        GroqRequestError::TooLarge(format!(
            "Groq subtitle request aborted (likely timed out): {message}"
        ))
    } else {
        GroqRequestError::Other(format!("Groq subtitle request failed: {message}"))
    }
}

fn build_groq_vocabulary_prompt(vocabulary: &[String]) -> Option<String> {
    let terms: Vec<String> = vocabulary
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .take(80)
        .map(ToOwned::to_owned)
        .collect();
    if terms.is_empty() {
        return None;
    }
    Some(format!(
        "Use these spellings and domain terms when heard: {}.",
        terms.join(", ")
    ))
}

fn add_multipart_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &[u8]) {
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
    );
    body.extend_from_slice(value);
    body.extend_from_slice(b"\r\n");
}

fn transcript_diagnostics(response: &GroqVerboseResponse) -> GroqTranscriptDiagnostics {
    let segments = response.segments.as_deref().unwrap_or_default();
    GroqTranscriptDiagnostics::from_metrics(
        segments.len(),
        segments.iter().map(|segment| {
            (
                segment.avg_logprob,
                segment.no_speech_prob,
                segment.compression_ratio,
            )
        }),
    )
}

struct GroqWavChunk {
    samples: Vec<i16>,
    offset_sec: f64,
    duration_sec: f64,
}

impl GroqWavAudio {
    fn duration_sec(&self) -> f64 {
        let frames = self.samples.len() / self.channels.max(1) as usize;
        frames as f64 / self.sample_rate.max(1) as f64
    }

    fn chunk_from_frames(&self, start_frame: usize, end_frame: usize) -> GroqWavChunk {
        let channels = self.channels.max(1) as usize;
        let total_frames = self.samples.len() / channels;
        let end_frame = end_frame.min(total_frames);
        let start_frame = start_frame.min(end_frame);
        let start_sample = start_frame * channels;
        let end_sample = end_frame * channels;
        GroqWavChunk {
            samples: self.samples[start_sample..end_sample].to_vec(),
            offset_sec: start_frame as f64 / self.sample_rate.max(1) as f64,
            duration_sec: (end_frame.saturating_sub(start_frame)) as f64
                / self.sample_rate.max(1) as f64,
        }
    }
}

fn decode_wav_audio(audio_data: &[u8]) -> Result<GroqWavAudio, String> {
    let cursor = Cursor::new(audio_data);
    let mut reader =
        hound::WavReader::new(cursor).map_err(|err| format!("Decode Groq WAV audio: {err}"))?;
    let spec = reader.spec();
    let samples = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("Read Groq WAV PCM samples: {err}"))?,
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| {
                sample.map(|value| (value.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("Read Groq WAV float samples: {err}"))?,
    };
    Ok(GroqWavAudio {
        samples,
        sample_rate: spec.sample_rate,
        channels: spec.channels.max(1),
    })
}

fn encode_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels: channels.max(1),
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut cursor, spec)
        .map_err(|err| format!("Create Groq split WAV writer: {err}"))?;
    for sample in samples {
        writer
            .write_sample(*sample)
            .map_err(|err| format!("Write Groq split WAV sample: {err}"))?;
    }
    writer
        .finalize()
        .map_err(|err| format!("Finalize Groq split WAV: {err}"))?;
    Ok(cursor.into_inner())
}

fn build_sentence_blocks(response: &GroqVerboseResponse) -> Vec<CompactSubtitleSegment> {
    if let Some(words) = response.words.as_ref().filter(|words| !words.is_empty()) {
        return build_sentence_blocks_from_words(words);
    }

    response
        .segments
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|segment| {
            let text = normalize_subtitle_text(&segment.text);
            if text.is_empty() {
                None
            } else {
                Some(CompactSubtitleSegment {
                    start_time: segment.start,
                    end_time: segment.end.max(segment.start + MIN_SUBTITLE_DURATION_SEC),
                    text,
                })
            }
        })
        .collect()
}

fn build_sentence_blocks_from_words(words: &[GroqWord]) -> Vec<CompactSubtitleSegment> {
    let mut blocks = Vec::new();
    let mut current_words: Vec<&GroqWord> = Vec::new();

    for word in words {
        if current_words.is_empty() {
            current_words.push(word);
            continue;
        }

        let previous = current_words[current_words.len() - 1];
        let gap = word.start - previous.end;
        let previous_text = normalize_subtitle_text(&join_word_tokens(
            &current_words
                .iter()
                .map(|entry| entry.word.as_str())
                .collect::<Vec<_>>(),
        ));

        if gap >= SENTENCE_BREAK_SILENCE_SEC || ends_sentence(&previous_text) {
            if let Some(segment) = finalize_word_block(&current_words) {
                blocks.push(segment);
            }
            current_words.clear();
        }

        current_words.push(word);
    }

    if let Some(segment) = finalize_word_block(&current_words) {
        blocks.push(segment);
    }

    blocks
}

fn finalize_word_block(words: &[&GroqWord]) -> Option<CompactSubtitleSegment> {
    let text = normalize_subtitle_text(&join_word_tokens(
        &words
            .iter()
            .map(|word| word.word.as_str())
            .collect::<Vec<_>>(),
    ));
    if text.is_empty() {
        return None;
    }
    let start_time = words.first()?.start;
    let end_time = words
        .last()?
        .end
        .max(start_time + MIN_SUBTITLE_DURATION_SEC);
    Some(CompactSubtitleSegment {
        start_time,
        end_time,
        text,
    })
}
