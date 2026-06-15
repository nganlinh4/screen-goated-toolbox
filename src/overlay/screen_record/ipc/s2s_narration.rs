use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use crate::api::audio::encode_wav;
use crate::api::realtime_audio::s2s::{
    S2sBatchSegment, default_batch_settings_for_target, run_gemini_live_s2s_batch_with_callbacks,
};

use super::job_registry::{self, JobHandle, JobState};
use super::media_server;
use super::subtitles::audio::compact_to_source_time;
use super::subtitles::media::prepare_clip_media;
use super::subtitles::types::{SubtitleClipRequest, SubtitleGenerationMethod};

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct S2sNarrationRequest {
    source_type: String,
    target_language: String,
    #[serde(default)]
    gemini_model: String,
    #[serde(default)]
    gemini_voice: String,
    #[serde(default)]
    gemini_speed: String,
    #[serde(default = "default_s2s_parallel_requests")]
    parallel_requests: usize,
    #[serde(default = "default_s2s_group_text_budget")]
    group_text_budget: usize,
    clips: Vec<SubtitleClipRequest>,
}

fn default_s2s_parallel_requests() -> usize {
    3
}

fn default_s2s_group_text_budget() -> usize {
    25
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct S2sNarrationSegmentResult {
    id: String,
    clip_id: String,
    source_text: String,
    target_text: String,
    start_time: f64,
    end_time: f64,
    path: String,
    duration: f64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct S2sNarrationClipResult {
    clip_id: String,
    is_partial: bool,
    segments: Vec<S2sNarrationSegmentResult>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct S2sNarrationJobSnapshot {
    state: String,
    message: String,
    progress: f64,
    total_clips: usize,
    completed_clips: usize,
    active_clip_id: Option<String>,
    vad_segment_done: usize,
    vad_segment_total: usize,
    vad_no_speech: bool,
    results_revision: usize,
    results: Vec<S2sNarrationClipResult>,
    #[serde(skip_serializing)]
    result_events: Vec<S2sNarrationResultEvent>,
    error: Option<String>,
}

#[derive(Clone)]
struct S2sNarrationResultEvent {
    revision: usize,
    result: S2sNarrationClipResult,
}

impl Default for S2sNarrationJobSnapshot {
    fn default() -> Self {
        Self {
            state: "queued".to_string(),
            message: "Queued Gemini S2S narration".to_string(),
            progress: 0.0,
            total_clips: 0,
            completed_clips: 0,
            active_clip_id: None,
            vad_segment_done: 0,
            vad_segment_total: 0,
            vad_no_speech: false,
            results_revision: 0,
            results: Vec::new(),
            result_events: Vec::new(),
            error: None,
        }
    }
}

impl JobState for S2sNarrationJobSnapshot {
    fn state(&self) -> &str {
        &self.state
    }
}

static S2S_NARRATION_JOBS: OnceLock<Mutex<HashMap<String, JobHandle<S2sNarrationJobSnapshot>>>> =
    OnceLock::new();

fn jobs() -> &'static Mutex<HashMap<String, JobHandle<S2sNarrationJobSnapshot>>> {
    job_registry::registry(&S2S_NARRATION_JOBS)
}

pub fn handle_start_s2s_narration(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: S2sNarrationRequest = serde_json::from_value(args.clone())
        .map_err(|error| format!("Invalid S2S narration request: {error}"))?;
    if request.clips.is_empty() {
        return Err("No audio source is available for Gemini S2S narration".to_string());
    }
    let mut jobs = jobs()
        .lock()
        .map_err(|_| "S2S narration jobs lock poisoned".to_string())?;
    if let Some(active) = job_registry::find_active(&jobs) {
        return Err(format!(
            "Gemini S2S narration already running (job={active})"
        ));
    }
    let job_id = job_registry::uuid("s2s-narration");
    let snapshot = Arc::new(Mutex::new(S2sNarrationJobSnapshot {
        total_clips: request.clips.len(),
        ..S2sNarrationJobSnapshot::default()
    }));
    let cancelled = Arc::new(AtomicBool::new(false));
    jobs.insert(
        job_id.clone(),
        JobHandle {
            snapshot: snapshot.clone(),
            cancelled: cancelled.clone(),
        },
    );
    drop(jobs);

    let thread_job_id = job_id.clone();
    std::thread::spawn(move || run_job(&thread_job_id, request, snapshot, cancelled));
    Ok(serde_json::json!({ "jobId": job_id }))
}

pub fn handle_get_s2s_narration_status(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let job_id = args["jobId"].as_str().ok_or("Missing jobId")?;
    let known_results_revision = args["knownResultsRevision"].as_u64().unwrap_or(0) as usize;
    let jobs = jobs()
        .lock()
        .map_err(|_| "S2S narration jobs lock poisoned".to_string())?;
    let handle = jobs
        .get(job_id)
        .ok_or_else(|| format!("Unknown S2S narration job: {job_id}"))?;
    let mut snapshot = handle
        .snapshot
        .lock()
        .map_err(|_| "S2S narration snapshot lock poisoned".to_string())?
        .clone();
    let latest_results_revision = snapshot.results_revision;
    if latest_results_revision <= known_results_revision {
        snapshot.results.clear();
    } else if !matches!(snapshot.state.as_str(), "completed" | "cancelled" | "error") {
        if let Some(event) = snapshot
            .result_events
            .iter()
            .find(|event| event.revision > known_results_revision)
        {
            snapshot.results_revision = event.revision;
            snapshot.results = vec![event.result.clone()];
        } else {
            snapshot.results_revision = latest_results_revision;
            snapshot.results.clear();
        }
    } else {
        snapshot.results = snapshot
            .result_events
            .iter()
            .filter(|event| event.revision > known_results_revision)
            .map(|event| event.result.clone())
            .collect();
    }
    serde_json::to_value(snapshot).map_err(|error| format!("Serialize S2S status: {error}"))
}

pub fn handle_cancel_s2s_narration(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let job_id = args["jobId"].as_str().ok_or("Missing jobId")?;
    let jobs = jobs()
        .lock()
        .map_err(|_| "S2S narration jobs lock poisoned".to_string())?;
    let handle = jobs
        .get(job_id)
        .ok_or_else(|| format!("Unknown S2S narration job: {job_id}"))?;
    handle.cancelled.store(true, Ordering::SeqCst);
    if let Ok(mut snapshot) = handle.snapshot.lock() {
        snapshot.state = "cancelled".to_string();
        snapshot.message = "Cancelled".to_string();
        snapshot.active_clip_id = None;
    }
    Ok(serde_json::Value::Null)
}

fn run_job(
    job_id: &str,
    request: S2sNarrationRequest,
    snapshot: Arc<Mutex<S2sNarrationJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
) {
    let started = Instant::now();
    let result = run_job_inner(job_id, &request, &snapshot, &cancelled);
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
            locked.message = "Gemini S2S narration complete".to_string();
            locked.active_clip_id = None;
            crate::log_info!(
                "[S2SNarration][job={}] completed in {:.3}s clips={}",
                job_id,
                started.elapsed().as_secs_f64(),
                request.clips.len()
            );
        }
        Err(error) => {
            crate::log_info!(
                "[S2SNarration][job={}] failed after {:.3}s: {}",
                job_id,
                started.elapsed().as_secs_f64(),
                error
            );
            locked.state = "error".to_string();
            locked.message = error.clone();
            locked.error = Some(error);
            locked.active_clip_id = None;
        }
    }
}

fn run_job_inner(
    job_id: &str,
    request: &S2sNarrationRequest,
    snapshot: &Arc<Mutex<S2sNarrationJobSnapshot>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), String> {
    if let Ok(mut locked) = snapshot.lock() {
        locked.state = "running".to_string();
        locked.message = "Generating Gemini S2S narration".to_string();
    }
    let settings = default_batch_settings_for_target(
        &request.target_language,
        &request.gemini_model,
        &request.gemini_voice,
        &request.gemini_speed,
    )
    .map_err(|error: anyhow::Error| error.to_string())?;
    let mut settings = settings;
    settings.parallel_requests = request.parallel_requests.clamp(1, 6);
    settings.vad_group_budget = request.group_text_budget.clamp(5, 120);
    crate::log_info!(
        "[S2SNarration][job={}] start clips={} source={} target={} model={} voice={} speed={} parallel={} group_budget={}",
        job_id,
        request.clips.len(),
        request.source_type,
        request.target_language,
        settings.model,
        settings.voice,
        settings.speed,
        settings.parallel_requests,
        settings.vad_group_budget
    );

    for (clip_index, clip) in request.clips.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            crate::log_info!(
                "[S2SNarration][job={}] cancelled before clip {}/{}",
                job_id,
                clip_index + 1,
                request.clips.len()
            );
            return Ok(());
        }
        if let Ok(mut locked) = snapshot.lock() {
            locked.active_clip_id = Some(clip.clip_id.clone());
            locked.vad_segment_done = 0;
            locked.vad_segment_total = 0;
            locked.vad_no_speech = false;
            locked.message = format!(
                "Generating Gemini S2S narration {}/{}",
                clip_index + 1,
                request.clips.len()
            );
        }
        let prepared = prepare_clip_media(
            SubtitleGenerationMethod::GroqWhisperAccurate,
            &request.source_type,
            clip,
        )?;
        let samples = super::wav_decode::decode_wav_mono_i16(&prepared.bytes, "S2S")?;
        crate::log_info!(
            "[S2SNarration][job={}] clip {}/{} id={} prepared_samples={} duration_sec={:.3}",
            job_id,
            clip_index + 1,
            request.clips.len(),
            clip.clip_id,
            samples.len(),
            samples.len() as f64 / 16_000.0
        );
        let mut progress = |done: usize, total: usize| {
            if let Ok(mut locked) = snapshot.lock() {
                locked.vad_segment_done = done.min(total);
                locked.vad_segment_total = total;
                locked.vad_no_speech = total == 0;
                locked.message = if total == 0 {
                    format!(
                        "Gemini S2S VAD found no speech in clip {}/{}",
                        clip_index + 1,
                        request.clips.len()
                    )
                } else {
                    format!(
                        "Generating Gemini S2S narration {}/{} · VAD segment {}/{}",
                        clip_index + 1,
                        request.clips.len(),
                        done.min(total),
                        total
                    )
                };
            }
        };
        let mut on_segment = |segment: S2sBatchSegment| -> anyhow::Result<()> {
            let wav = encode_wav(&segment.audio_pcm_24k, 24_000, 1);
            let path = media_server::write_managed_narration_wav(
                job_id,
                clip_index * 1000 + segment.id as usize,
                &wav,
            )
            .map_err(anyhow::Error::msg)?;
            let start_time = compact_to_source_time(
                segment.source_start_sec,
                &clip.trim_segments,
                clip.source_duration,
            );
            let end_time = compact_to_source_time(
                segment.source_end_sec,
                &clip.trim_segments,
                clip.source_duration,
            );
            let result = S2sNarrationSegmentResult {
                id: format!("{}-s2s-{}", clip.clip_id, segment.id),
                clip_id: clip.clip_id.clone(),
                source_text: segment.source_text,
                target_text: segment.target_text,
                start_time,
                end_time: end_time.max(start_time + 0.05),
                path,
                duration: segment.audio_pcm_24k.len() as f64 / 24_000.0,
            };
            push_result(
                snapshot,
                S2sNarrationClipResult {
                    clip_id: clip.clip_id.clone(),
                    is_partial: true,
                    segments: vec![result],
                },
            )
            .map_err(anyhow::Error::msg)
        };
        let _batch_segments = run_gemini_live_s2s_batch_with_callbacks(
            samples,
            settings.clone(),
            cancelled.clone(),
            Some(&mut progress),
            Some(&mut on_segment),
        )
        .map_err(|error: anyhow::Error| error.to_string())?;
        crate::log_info!(
            "[S2SNarration][job={}] clip {}/{} id={} finished",
            job_id,
            clip_index + 1,
            request.clips.len(),
            clip.clip_id
        );
        if let Ok(mut locked) = snapshot.lock() {
            locked.completed_clips = clip_index + 1;
            locked.progress = (clip_index + 1) as f64 / request.clips.len().max(1) as f64;
        }
    }
    Ok(())
}

fn push_result(
    snapshot: &Arc<Mutex<S2sNarrationJobSnapshot>>,
    result: S2sNarrationClipResult,
) -> Result<(), String> {
    let mut locked = snapshot
        .lock()
        .map_err(|_| "S2S narration snapshot lock poisoned".to_string())?;
    locked.results_revision += 1;
    let revision = locked.results_revision;
    locked.results = vec![result.clone()];
    locked
        .result_events
        .push(S2sNarrationResultEvent { revision, result });
    Ok(())
}
