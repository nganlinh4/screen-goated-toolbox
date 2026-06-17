mod gt_test;
mod output_vad;
mod socket_io;
mod stream;
mod text_delta;
mod word_distribute;

/// CLI test entry: stream `input_wav` (16 kHz mono PCM) through the live-translate
/// narration pipeline and write `<input_wav>.narration.wav`.
pub(crate) fn run_gt_narration_test_cli(input_wav: &str, target_language: &str) -> Result<(), String> {
    gt_test::run_cli(input_wav, target_language)
}

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use super::job_registry::{self, JobHandle, JobState};
use super::subtitles::types::SubtitleClipRequest;

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiTranslateNarrationRequest {
    source_type: String,
    target_language: String,
    #[serde(default = "default_group_budget")]
    group_text_budget: usize,
    clips: Vec<SubtitleClipRequest>,
}

fn default_group_budget() -> usize {
    25
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SegmentResult {
    id: String,
    clip_id: String,
    source_text: String,
    target_text: String,
    start_time: f64,
    end_time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    narration_start_time: Option<f64>,
    path: String,
    duration: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_in_point: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_out_point: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    narration_group_take_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    narration_group_source_start_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alignment_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alignment_confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tts_profile_method: Option<String>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipResult {
    clip_id: String,
    is_partial: bool,
    segments: Vec<SegmentResult>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct JobSnapshot {
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
    results: Vec<ClipResult>,
    #[serde(skip_serializing)]
    result_events: Vec<ResultEvent>,
    error: Option<String>,
}

#[derive(Clone)]
struct ResultEvent {
    revision: usize,
    result: ClipResult,
}

impl Default for JobSnapshot {
    fn default() -> Self {
        Self {
            state: "queued".to_string(),
            message: "Queued Gemini Translate narration".to_string(),
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

impl JobState for JobSnapshot {
    fn state(&self) -> &str {
        &self.state
    }
}

static JOBS: OnceLock<Mutex<HashMap<String, JobHandle<JobSnapshot>>>> = OnceLock::new();

fn jobs() -> &'static Mutex<HashMap<String, JobHandle<JobSnapshot>>> {
    job_registry::registry(&JOBS)
}

pub fn handle_start_gemini_translate_narration(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: GeminiTranslateNarrationRequest = serde_json::from_value(args.clone())
        .map_err(|error| format!("Invalid Gemini Translate narration request: {error}"))?;
    if request.clips.is_empty() {
        return Err("No audio source is available for Gemini Translate narration".to_string());
    }
    let mut jobs = jobs()
        .lock()
        .map_err(|_| "Gemini Translate narration jobs lock poisoned".to_string())?;
    if let Some(active) = job_registry::find_active(&jobs) {
        return Err(format!(
            "Gemini Translate narration already running (job={active})"
        ));
    }
    let job_id = job_registry::uuid("gemini-translate-narration");
    let snapshot = Arc::new(Mutex::new(JobSnapshot {
        total_clips: request.clips.len(),
        ..JobSnapshot::default()
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

pub fn handle_get_gemini_translate_narration_status(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let job_id = args["jobId"].as_str().ok_or("Missing jobId")?;
    let known_results_revision = args["knownResultsRevision"].as_u64().unwrap_or(0) as usize;
    let jobs = jobs()
        .lock()
        .map_err(|_| "Gemini Translate narration jobs lock poisoned".to_string())?;
    let handle = jobs
        .get(job_id)
        .ok_or_else(|| format!("Unknown Gemini Translate narration job: {job_id}"))?;
    let mut snapshot = handle
        .snapshot
        .lock()
        .map_err(|_| "Gemini Translate narration snapshot lock poisoned".to_string())?
        .clone();
    let latest = snapshot.results_revision;
    if latest <= known_results_revision {
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
    serde_json::to_value(snapshot)
        .map_err(|error| format!("Serialize Gemini Translate status: {error}"))
}

pub fn handle_cancel_gemini_translate_narration(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let job_id = args["jobId"].as_str().ok_or("Missing jobId")?;
    let jobs = jobs()
        .lock()
        .map_err(|_| "Gemini Translate narration jobs lock poisoned".to_string())?;
    let handle = jobs
        .get(job_id)
        .ok_or_else(|| format!("Unknown Gemini Translate narration job: {job_id}"))?;
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
    request: GeminiTranslateNarrationRequest,
    snapshot: Arc<Mutex<JobSnapshot>>,
    cancelled: Arc<AtomicBool>,
) {
    let started = Instant::now();
    let result = stream::run_job_inner(job_id, &request, &snapshot, &cancelled);
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
            locked.message = "Gemini Translate narration complete".to_string();
            locked.active_clip_id = None;
            crate::log_info!(
                "[GeminiTranslateNarration][job={}] completed in {:.3}s clips={}",
                job_id,
                started.elapsed().as_secs_f64(),
                request.clips.len()
            );
        }
        Err(error) => {
            locked.state = "error".to_string();
            locked.message = error.clone();
            locked.error = Some(error.clone());
            locked.active_clip_id = None;
            crate::log_info!(
                "[GeminiTranslateNarration][job={}] failed after {:.3}s: {}",
                job_id,
                started.elapsed().as_secs_f64(),
                error
            );
        }
    }
}
