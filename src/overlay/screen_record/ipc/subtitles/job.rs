use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use super::audio::{MIN_SUBTITLE_DURATION_SEC, build_trimmed_wav, compact_to_source_time};
use super::providers;
use super::types::{
    SubtitleClipResult, SubtitleGenerationCapabilities, SubtitleGenerationRequest,
    SubtitleJobSnapshot, SubtitleSegmentResult, SubtitleSkippedClip,
};

#[derive(Clone)]
struct SubtitleJobHandle {
    snapshot: Arc<Mutex<SubtitleJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
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
        message_key: Some("subtitleStatusQueued".to_string()),
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
    serde_json::to_value(snapshot).map_err(|e| format!("Serialize subtitle status: {e}"))
}

pub fn handle_get_subtitle_generation_capabilities(
    _args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let payload = SubtitleGenerationCapabilities {
        methods: providers::capabilities(),
    };
    serde_json::to_value(payload).map_err(|e| format!("Serialize subtitle capabilities: {e}"))
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
        snapshot.message_key = Some("subtitleStatusCancelled".to_string());
        snapshot.message_params.clear();
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
        locked.message_key = Some("subtitleStatusCancelled".to_string());
        locked.message_params.clear();
        locked.active_clip_id = None;
        return;
    }
    match result {
        Ok(()) => {
            locked.state = "completed".to_string();
            locked.progress = 1.0;
            locked.active_clip_id = None;
            if locked.skipped.is_empty() {
                locked.message = "Subtitle generation complete".to_string();
                locked.message_key = Some("subtitleStatusComplete".to_string());
                locked.message_params.clear();
            } else {
                locked.message = format!(
                    "Subtitle generation complete with {} skipped clip(s)",
                    locked.skipped.len()
                );
                locked.message_key = Some("subtitleStatusCompleteWithSkipped".to_string());
                locked.message_params = HashMap::from([(
                    "skipped".to_string(),
                    locked.skipped.len().to_string(),
                )]);
            }
        }
        Err(error) => {
            locked.state = "error".to_string();
            locked.message = error.clone();
            locked.error = Some(error);
            locked.message_key = None;
            locked.message_params.clear();
            locked.active_clip_id = None;
        }
    }
}

fn run_subtitle_generation_inner(
    request: &SubtitleGenerationRequest,
    snapshot: &Arc<Mutex<SubtitleJobSnapshot>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), String> {
    let mut backend = providers::create_backend(request.subtitle_method)?;

    if let Ok(mut locked) = snapshot.lock() {
        locked.state = "running".to_string();
        locked.message = "Generating subtitles…".to_string();
        locked.message_key = Some("subtitleGenerating".to_string());
        locked.message_params.clear();
    }

    for (index, clip) in request.clips.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(());
        }

        if clip.source_path.trim().is_empty() || !Path::new(&clip.source_path).exists() {
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
        let mut publish_progress =
            |progress: providers::SubtitleBackendProgress| -> Result<(), String> {
                publish_clip_progress(
                    snapshot,
                    clip,
                    index,
                    request.clips.len(),
                    progress.completed_steps,
                    progress.total_steps,
                    progress.segments,
                )
            };
        let compact_segments = backend.transcribe_clip(
            wav_data,
            request.language_hint.as_deref(),
            &mut publish_progress,
        )?;
        let mapped_segments = map_segments_to_source_time(
            compact_segments,
            &clip.trim_segments,
            clip.source_duration,
        );

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

fn publish_clip_progress(
    snapshot: &Arc<Mutex<SubtitleJobSnapshot>>,
    clip: &super::types::SubtitleClipRequest,
    clip_index: usize,
    total_clips: usize,
    completed_steps: usize,
    total_steps: usize,
    compact_segments: Vec<super::types::CompactSubtitleSegment>,
) -> Result<(), String> {
    let mapped_segments =
        map_segments_to_source_time(compact_segments, &clip.trim_segments, clip.source_duration);
    let is_partial = completed_steps < total_steps;
    let progress = if total_steps == 0 {
        clip_index as f64 / total_clips.max(1) as f64
    } else {
        (clip_index as f64 + completed_steps as f64 / total_steps as f64)
            / total_clips.max(1) as f64
    };
    let message = if total_steps > 1 {
        format!(
            "Transcribing {} ({}/{})",
            clip.clip_name, completed_steps, total_steps
        )
    } else {
        format!("Transcribing {}", clip.clip_name)
    };

    let mut locked = snapshot
        .lock()
        .map_err(|_| "Subtitle snapshot lock poisoned".to_string())?;
    locked.message = message;
    locked.message_key = Some(if total_steps > 1 {
        "subtitleStatusTranscribingClipChunked".to_string()
    } else {
        "subtitleStatusTranscribingClip".to_string()
    });
    locked.message_params = HashMap::from([
        ("clipName".to_string(), clip.clip_name.clone()),
        ("completed".to_string(), completed_steps.to_string()),
        ("total".to_string(), total_steps.to_string()),
    ]);
    locked.progress = progress;
    locked.active_clip_id = Some(clip.clip_id.clone());
    if let Some(existing) = locked
        .results
        .iter_mut()
        .find(|result| result.clip_id == clip.clip_id)
    {
        existing.segments = mapped_segments;
        existing.is_partial = is_partial;
    } else {
        locked.results.push(SubtitleClipResult {
            clip_id: clip.clip_id.clone(),
            is_partial,
            segments: mapped_segments,
        });
    }
    Ok(())
}

fn map_segments_to_source_time(
    compact_segments: Vec<super::types::CompactSubtitleSegment>,
    trim_segments: &[super::types::SubtitleTrimSegment],
    source_duration: f64,
) -> Vec<SubtitleSegmentResult> {
    compact_segments
        .into_iter()
        .map(|segment| {
            let start_time =
                compact_to_source_time(segment.start_time, trim_segments, source_duration);
            let end_time = compact_to_source_time(segment.end_time, trim_segments, source_duration)
                .max(start_time + MIN_SUBTITLE_DURATION_SEC);
            SubtitleSegmentResult {
                start_time,
                end_time,
                text: segment.text,
            }
        })
        .collect()
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
    if let Some(existing) = locked
        .results
        .iter_mut()
        .find(|result| result.clip_id == clip_id)
    {
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
