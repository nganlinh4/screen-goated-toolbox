use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::APP;
use crate::api::client::UREQ_AGENT;
use crate::model_config::get_model_by_id;
use crate::overlay::screen_record::mf_audio::MfAudioDecoder;

const GROQ_AUDIO_TRANSCRIPT_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const SENTENCE_BREAK_SILENCE_SEC: f64 = 0.45;
const MIN_SUBTITLE_DURATION_SEC: f64 = 0.1;
const MF_100NS_PER_SEC: f64 = 10_000_000.0;

#[derive(Clone, Deserialize)]
pub struct SubtitleGenerationRequest {
    #[serde(rename = "sourceType")]
    source_type: String,
    #[serde(rename = "languageHint")]
    language_hint: Option<String>,
    clips: Vec<SubtitleClipRequest>,
}

#[derive(Clone, Deserialize)]
pub struct SubtitleClipRequest {
    #[serde(rename = "clipId")]
    clip_id: String,
    #[serde(rename = "clipName")]
    clip_name: String,
    #[serde(rename = "sourcePath")]
    source_path: String,
    #[serde(rename = "sourceDuration")]
    source_duration: f64,
    #[serde(rename = "trimSegments")]
    trim_segments: Vec<SubtitleTrimSegment>,
    #[serde(rename = "micAudioOffsetSec")]
    mic_audio_offset_sec: Option<f64>,
}

#[derive(Clone, Deserialize)]
struct SubtitleTrimSegment {
    #[allow(dead_code)]
    id: String,
    #[serde(rename = "startTime")]
    start_time: f64,
    #[serde(rename = "endTime")]
    end_time: f64,
}

#[derive(Clone, Serialize, Default)]
pub struct SubtitleJobSnapshot {
    state: String,
    message: String,
    progress: f64,
    #[serde(rename = "activeClipId")]
    active_clip_id: Option<String>,
    #[serde(rename = "totalClips")]
    total_clips: usize,
    #[serde(rename = "completedClips")]
    completed_clips: usize,
    results: Vec<SubtitleClipResult>,
    skipped: Vec<SubtitleSkippedClip>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct SubtitleClipResult {
    #[serde(rename = "clipId")]
    clip_id: String,
    #[serde(rename = "isPartial")]
    is_partial: bool,
    segments: Vec<SubtitleSegmentResult>,
}

#[derive(Clone, Serialize)]
pub struct SubtitleSegmentResult {
    #[serde(rename = "startTime")]
    start_time: f64,
    #[serde(rename = "endTime")]
    end_time: f64,
    text: String,
}

#[derive(Clone, Serialize)]
pub struct SubtitleSkippedClip {
    #[serde(rename = "clipId")]
    clip_id: String,
    reason: String,
}

#[derive(Clone)]
struct SubtitleJobHandle {
    snapshot: Arc<Mutex<SubtitleJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
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
}

#[derive(Clone, Deserialize)]
struct GroqWord {
    start: f64,
    end: f64,
    word: String,
}

static SUBTITLE_JOBS: OnceLock<Mutex<HashMap<String, SubtitleJobHandle>>> = OnceLock::new();

fn subtitle_jobs() -> &'static Mutex<HashMap<String, SubtitleJobHandle>> {
    SUBTITLE_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn handle_start_subtitle_generation(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: SubtitleGenerationRequest = serde_json::from_value(args.clone())
        .map_err(|e| format!("Invalid subtitle request: {e}"))?;
    let job_id = uuid();
    let snapshot = Arc::new(Mutex::new(SubtitleJobSnapshot {
        state: "queued".to_string(),
        message: "Queued".to_string(),
        total_clips: request.clips.len(),
        ..SubtitleJobSnapshot::default()
    }));
    let cancelled = Arc::new(AtomicBool::new(false));
    subtitle_jobs()
        .lock()
        .map_err(|_| "Subtitle jobs lock poisoned".to_string())?
        .insert(
            job_id.clone(),
            SubtitleJobHandle {
                snapshot: snapshot.clone(),
                cancelled: cancelled.clone(),
            },
        );

    std::thread::spawn(move || run_subtitle_generation(request, snapshot, cancelled));

    Ok(serde_json::json!({ "jobId": job_id }))
}

pub fn handle_get_subtitle_generation_status(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let job_id = args["jobId"].as_str().ok_or("Missing jobId")?;
    let jobs = subtitle_jobs()
        .lock()
        .map_err(|_| "Subtitle jobs lock poisoned".to_string())?;
    let handle = jobs
        .get(job_id)
        .ok_or_else(|| format!("Unknown subtitle job: {job_id}"))?;
    let snapshot = handle
        .snapshot
        .lock()
        .map_err(|_| "Subtitle snapshot lock poisoned".to_string())?
        .clone();
    Ok(serde_json::to_value(snapshot).map_err(|e| format!("Serialize subtitle status: {e}"))?)
}

pub fn handle_cancel_subtitle_generation(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let job_id = args["jobId"].as_str().ok_or("Missing jobId")?;
    let jobs = subtitle_jobs()
        .lock()
        .map_err(|_| "Subtitle jobs lock poisoned".to_string())?;
    let handle = jobs
        .get(job_id)
        .ok_or_else(|| format!("Unknown subtitle job: {job_id}"))?;
    handle.cancelled.store(true, Ordering::SeqCst);
    if let Ok(mut snapshot) = handle.snapshot.lock() {
        snapshot.state = "cancelled".to_string();
        snapshot.message = "Cancelled".to_string();
        snapshot.active_clip_id = None;
    }
    Ok(serde_json::Value::Null)
}

fn run_subtitle_generation(
    request: SubtitleGenerationRequest,
    snapshot: Arc<Mutex<SubtitleJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
) {
    let result = run_subtitle_generation_inner(&request, &snapshot, &cancelled);
    let mut locked = match snapshot.lock() {
        Ok(locked) => locked,
        Err(_) => return,
    };
    if cancelled.load(Ordering::SeqCst) {
        locked.state = "cancelled".to_string();
        locked.message = "Cancelled".to_string();
        locked.active_clip_id = None;
        return;
    }
    match result {
        Ok(()) => {
            locked.state = "completed".to_string();
            locked.progress = 1.0;
            locked.active_clip_id = None;
            locked.message = if locked.skipped.is_empty() {
                "Subtitle generation complete".to_string()
            } else {
                format!(
                    "Subtitle generation complete with {} skipped clip(s)",
                    locked.skipped.len()
                )
            };
        }
        Err(error) => {
            locked.state = "error".to_string();
            locked.message = error.clone();
            locked.error = Some(error);
            locked.active_clip_id = None;
        }
    }
}

fn run_subtitle_generation_inner(
    request: &SubtitleGenerationRequest,
    snapshot: &Arc<Mutex<SubtitleJobSnapshot>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), String> {
    let (api_key, model_name) = {
        let app = APP.lock().map_err(|_| "APP lock poisoned".to_string())?;
        let model = get_model_by_id("whisper-accurate")
            .ok_or_else(|| "whisper-accurate model config missing".to_string())?;
        (app.config.api_key.clone(), model.full_name)
    };
    if api_key.trim().is_empty() {
        return Err("NO_API_KEY:groq".to_string());
    }

    if let Ok(mut locked) = snapshot.lock() {
        locked.state = "running".to_string();
        locked.message = "Generating subtitles…".to_string();
    }

    for (index, clip) in request.clips.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(());
        }

        if clip.source_path.trim().is_empty() || !std::path::Path::new(&clip.source_path).exists() {
            push_skipped(
                snapshot,
                &clip.clip_id,
                format!("Missing {} source", request.source_type),
            )?;
            continue;
        }

        update_progress(
            snapshot,
            format!("Transcribing {}", clip.clip_name),
            index,
            request.clips.len(),
        )?;
        upsert_clip_result(snapshot, &clip.clip_id, Vec::new(), true)?;

        let wav_data = build_trimmed_wav(
            &clip.source_path,
            &clip.trim_segments,
            clip.mic_audio_offset_sec.unwrap_or(0.0),
            request.source_type == "mic",
        )?;
        let response = transcribe_with_groq_verbose(
            &api_key,
            &model_name,
            wav_data,
            request.language_hint.as_deref(),
        )?;
        let compact_segments = build_sentence_blocks(&response);
        let mapped_segments = compact_segments
            .into_iter()
            .map(|segment| SubtitleSegmentResult {
                start_time: compact_to_source_time(
                    segment.start_time,
                    &clip.trim_segments,
                    clip.source_duration,
                ),
                end_time: compact_to_source_time(
                    segment.end_time,
                    &clip.trim_segments,
                    clip.source_duration,
                )
                .max(
                    compact_to_source_time(
                        segment.start_time,
                        &clip.trim_segments,
                        clip.source_duration,
                    ) + MIN_SUBTITLE_DURATION_SEC,
                ),
                text: segment.text,
            })
            .collect::<Vec<_>>();

        upsert_clip_result(snapshot, &clip.clip_id, mapped_segments, false)?;

        let mut locked = snapshot
            .lock()
            .map_err(|_| "Subtitle snapshot lock poisoned".to_string())?;
        locked.completed_clips += 1;
        locked.progress = locked.completed_clips as f64 / locked.total_clips.max(1) as f64;
        locked.active_clip_id = None;
    }

    Ok(())
}

fn build_trimmed_wav(
    source_path: &str,
    trim_segments: &[SubtitleTrimSegment],
    source_offset_sec: f64,
    apply_offset: bool,
) -> Result<Vec<u8>, String> {
    let decoder = MfAudioDecoder::new_with_output_format(source_path, Some(16_000), Some(1))?;
    let sample_rate = decoder.sample_rate() as f64;
    let channels = decoder.channels().max(1) as usize;
    let mut pcm_samples: Vec<i16> = Vec::new();

    for trim_segment in trim_segments {
        let adjusted_start =
            (trim_segment.start_time + if apply_offset { source_offset_sec } else { 0.0 }).max(0.0);
        let adjusted_end = (trim_segment.end_time
            + if apply_offset { source_offset_sec } else { 0.0 })
        .max(adjusted_start);
        decoder.seek((adjusted_start * MF_100NS_PER_SEC) as i64)?;

        while let Some((bytes, timestamp_100ns)) = decoder.read_samples()? {
            let timestamp_sec = timestamp_100ns as f64 / MF_100NS_PER_SEC;
            let total_float_samples = bytes.len() / 4;
            if total_float_samples == 0 {
                continue;
            }
            let frame_count = total_float_samples / channels;
            let chunk_duration_sec = frame_count as f64 / sample_rate;
            let chunk_end_sec = timestamp_sec + chunk_duration_sec;
            if chunk_end_sec <= adjusted_start {
                continue;
            }
            if timestamp_sec >= adjusted_end {
                break;
            }

            let overlap_start = adjusted_start.max(timestamp_sec);
            let overlap_end = adjusted_end.min(chunk_end_sec);
            if overlap_end <= overlap_start {
                continue;
            }

            let start_frame = ((overlap_start - timestamp_sec) * sample_rate)
                .floor()
                .max(0.0) as usize;
            let end_frame = ((overlap_end - timestamp_sec) * sample_rate)
                .ceil()
                .max(start_frame as f64) as usize;
            let floats = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect::<Vec<_>>();

            for frame_index in start_frame..end_frame.min(frame_count) {
                let sample = floats[frame_index * channels];
                let clamped = sample.clamp(-1.0, 1.0);
                pcm_samples.push((clamped * i16::MAX as f32) as i16);
            }
        }
    }

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    let mut writer =
        hound::WavWriter::new(&mut cursor, spec).map_err(|e| format!("Create WAV writer: {e}"))?;
    for sample in pcm_samples {
        writer
            .write_sample(sample)
            .map_err(|e| format!("Write WAV sample: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("Finalize WAV: {e}"))?;
    Ok(cursor.into_inner())
}

fn transcribe_with_groq_verbose(
    api_key: &str,
    model_name: &str,
    audio_data: Vec<u8>,
    language_hint: Option<&str>,
) -> Result<GroqVerboseResponse, String> {
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
    if let Some(language) = language_hint
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "auto")
    {
        add_multipart_field(&mut body, &boundary, "language", language.as_bytes());
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
        .map_err(|e| format!("Groq subtitle request failed: {e}"))?;

    let json: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|e| format!("Parse Groq subtitle response: {e}"))?;
    serde_json::from_value(json).map_err(|e| format!("Decode Groq verbose response: {e}"))
}

fn add_multipart_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &[u8]) {
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
    );
    body.extend_from_slice(value);
    body.extend_from_slice(b"\r\n");
}

fn build_sentence_blocks(response: &GroqVerboseResponse) -> Vec<SubtitleSegmentResult> {
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
                Some(SubtitleSegmentResult {
                    start_time: segment.start,
                    end_time: segment.end.max(segment.start + MIN_SUBTITLE_DURATION_SEC),
                    text,
                })
            }
        })
        .collect()
}

fn build_sentence_blocks_from_words(words: &[GroqWord]) -> Vec<SubtitleSegmentResult> {
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

fn finalize_word_block(words: &[&GroqWord]) -> Option<SubtitleSegmentResult> {
    let text = normalize_subtitle_text(&join_word_tokens(
        &words.iter().map(|word| word.word.as_str()).collect::<Vec<_>>(),
    ));
    if text.is_empty() {
        return None;
    }
    let start_time = words.first()?.start;
    let end_time = words
        .last()?
        .end
        .max(start_time + MIN_SUBTITLE_DURATION_SEC);
    Some(SubtitleSegmentResult {
        start_time,
        end_time,
        text,
    })
}

fn join_word_tokens(tokens: &[&str]) -> String {
    let mut result = String::new();
    let mut previous: Option<&str> = None;
    for token in tokens {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        if result.is_empty() {
            result.push_str(trimmed);
            previous = Some(trimmed);
            continue;
        }
        if should_attach_without_space(trimmed, previous) {
            result.push_str(trimmed);
        } else {
            result.push(' ');
            result.push_str(trimmed);
        }
        previous = Some(trimmed);
    }
    result
}

fn should_attach_without_space(token: &str, previous: Option<&str>) -> bool {
    let leading = token.chars().next();
    if matches!(
        leading,
        Some('.') | Some(',') | Some('!') | Some('?') | Some(':') | Some(';') | Some('…')
    ) {
        return true;
    }
    if matches!(token, "'" | "’" | "\"" | ")" | "]" | "}" | "%") {
        return true;
    }
    if previous.is_some_and(|prev| matches!(prev, "(" | "[" | "{" | "\"" | "'")) {
        return true;
    }
    false
}

fn ends_sentence(text: &str) -> bool {
    matches!(
        text.chars().last(),
        Some('.') | Some('!') | Some('?') | Some('…')
    )
}

fn normalize_subtitle_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn compact_to_source_time(
    compact_time: f64,
    trim_segments: &[SubtitleTrimSegment],
    source_duration: f64,
) -> f64 {
    let mut remaining = compact_time.max(0.0);
    for segment in trim_segments {
        let len = (segment.end_time - segment.start_time).max(0.0);
        if remaining <= len {
            return (segment.start_time + remaining).clamp(0.0, source_duration);
        }
        remaining -= len;
    }
    trim_segments
        .last()
        .map(|segment| segment.end_time)
        .unwrap_or(source_duration)
        .clamp(0.0, source_duration)
}

fn update_progress(
    snapshot: &Arc<Mutex<SubtitleJobSnapshot>>,
    message: String,
    clip_index: usize,
    total_clips: usize,
) -> Result<(), String> {
    let mut locked = snapshot
        .lock()
        .map_err(|_| "Subtitle snapshot lock poisoned".to_string())?;
    locked.message = message;
    locked.progress = clip_index as f64 / total_clips.max(1) as f64;
    Ok(())
}

fn upsert_clip_result(
    snapshot: &Arc<Mutex<SubtitleJobSnapshot>>,
    clip_id: &str,
    segments: Vec<SubtitleSegmentResult>,
    is_partial: bool,
) -> Result<(), String> {
    let mut locked = snapshot
        .lock()
        .map_err(|_| "Subtitle snapshot lock poisoned".to_string())?;
    locked.active_clip_id = Some(clip_id.to_string());
    if let Some(existing) = locked.results.iter_mut().find(|result| result.clip_id == clip_id) {
        existing.segments = segments;
        existing.is_partial = is_partial;
    } else {
        locked.results.push(SubtitleClipResult {
            clip_id: clip_id.to_string(),
            is_partial,
            segments,
        });
    }
    Ok(())
}

fn push_skipped(
    snapshot: &Arc<Mutex<SubtitleJobSnapshot>>,
    clip_id: &str,
    reason: String,
) -> Result<(), String> {
    let mut locked = snapshot
        .lock()
        .map_err(|_| "Subtitle snapshot lock poisoned".to_string())?;
    locked.skipped.push(SubtitleSkippedClip {
        clip_id: clip_id.to_string(),
        reason,
    });
    Ok(())
}

fn uuid() -> String {
    format!(
        "subtitle-{}-{}",
        chrono::Utc::now().timestamp_millis(),
        std::process::id()
    )
}
