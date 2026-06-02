use std::collections::HashMap;
use std::io::Write;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use unicode_normalization::UnicodeNormalization;

use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::TtsCollectedAudio;
use crate::api::tts::types::TtsRequestProfile;
use crate::config::tts_catalog::{
    GEMINI_VOICES, KOKORO_VOICES, MAGPIE_VOICE_LANGUAGES, MAGPIE_VOICES, SUPERTONIC_LANGUAGES,
    SUPERTONIC_VOICES, SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES, narration_tts_providers,
    normalize_kokoro_lang, normalize_magpie_voice, tts_method_id,
};
use crate::config::{
    EdgeTtsSettings, EdgeTtsVoiceConfig, KokoroSettings, KokoroVoiceConfig, MagpieSettings,
    MagpieVoiceConfig, StepAudioVoiceConfig, SupertonicSettings, SupertonicVoiceConfig,
    TtsLanguageCondition, TtsMethod, TtsPlaygroundSettings,
};
use crate::model_config::tts_gemini_model_options;

use super::media_server;
use super::subtitles::audio::snap_split_frames_to_silence;

const NARRATION_TTS_MAX_ATTEMPTS: usize = 4;
const NARRATION_TTS_RETRY_BASE_DELAY_MS: u64 = 350;
const NARRATION_GROUP_DEFAULT_TEXT_BUDGET: usize = 25;
const NARRATION_GROUP_MIN_TEXT_BUDGET: usize = 5;
const NARRATION_GROUP_MAX_TEXT_BUDGET: usize = 120;
const NARRATION_GROUP_MAX_ITEMS: usize = 10;
const NARRATION_GROUP_MAX_CHARS: usize = 650;
const NARRATION_GROUP_GAP_BREAK_SEC: f64 = 1.2;
const NARRATION_GROUP_DEFAULT_VAD_RADIUS_SEC: f64 = 0.35;
const NARRATION_ALIGNER_ENV: &str = "SGT_NARRATION_ALIGNER_CMD";

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleNarrationItemRequest {
    id: String,
    text: String,
    start_time: f64,
    end_time: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NarrationLanguageDetectionItem {
    text: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NarrationLanguageDetectionRequest {
    #[serde(default)]
    items: Vec<NarrationLanguageDetectionItem>,
}

mod profile;
use profile::{
    TtsProfileWire, default_gemini_parallel_requests, default_gemini_s2s_parallel_requests,
};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleNarrationRequest {
    items: Vec<SubtitleNarrationItemRequest>,
    profile: TtsProfileWire,
    #[serde(default)]
    source_language_code: Option<String>,
    #[serde(default)]
    grouping: SubtitleNarrationGroupingRequest,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleNarrationGroupingRequest {
    #[serde(default = "default_narration_group_text_budget")]
    text_budget_units: usize,
    #[serde(default = "default_narration_group_vad_radius_sec")]
    vad_search_radius_sec: f64,
}

impl Default for SubtitleNarrationGroupingRequest {
    fn default() -> Self {
        Self {
            text_budget_units: default_narration_group_text_budget(),
            vad_search_radius_sec: default_narration_group_vad_radius_sec(),
        }
    }
}

fn default_narration_group_text_budget() -> usize {
    NARRATION_GROUP_DEFAULT_TEXT_BUDGET
}

fn default_narration_group_vad_radius_sec() -> f64 {
    NARRATION_GROUP_DEFAULT_VAD_RADIUS_SEC
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleNarrationResult {
    subtitle_id: String,
    text: String,
    path: String,
    duration: f64,
    source_in_point: f64,
    source_out_point: f64,
    group_id: String,
    narration_group_take_id: String,
    narration_group_prompt_text: String,
    narration_group_source_start_time: f64,
    alignment_mode: String,
    alignment_confidence: f64,
    start_time: f64,
    end_time: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleNarrationError {
    subtitle_id: String,
    message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleNarrationJobSnapshot {
    state: String,
    message: String,
    progress: f64,
    total_items: usize,
    completed_items: usize,
    active_subtitle_id: Option<String>,
    results_revision: usize,
    results: Vec<SubtitleNarrationResult>,
    #[serde(skip_serializing, skip_deserializing)]
    result_events: Vec<SubtitleNarrationResultEvent>,
    errors: Vec<SubtitleNarrationError>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct SubtitleNarrationResultEvent {
    revision: usize,
    result: SubtitleNarrationResult,
}

impl Default for SubtitleNarrationJobSnapshot {
    fn default() -> Self {
        Self {
            state: "queued".to_string(),
            message: "Queued subtitle narration".to_string(),
            progress: 0.0,
            total_items: 0,
            completed_items: 0,
            active_subtitle_id: None,
            results_revision: 0,
            results: Vec::new(),
            result_events: Vec::new(),
            errors: Vec::new(),
            error: None,
        }
    }
}

#[derive(Clone)]
struct SubtitleNarrationJobHandle {
    snapshot: Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
}

static SUBTITLE_NARRATION_JOBS: OnceLock<Mutex<HashMap<String, SubtitleNarrationJobHandle>>> =
    OnceLock::new();

fn subtitle_narration_jobs() -> &'static Mutex<HashMap<String, SubtitleNarrationJobHandle>> {
    SUBTITLE_NARRATION_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn find_active_narration_job_id(
    jobs: &HashMap<String, SubtitleNarrationJobHandle>,
) -> Option<String> {
    jobs.iter().find_map(|(job_id, handle)| {
        let snapshot = handle.snapshot.lock().ok()?;
        matches!(snapshot.state.as_str(), "queued" | "running").then(|| job_id.clone())
    })
}

pub fn handle_start_subtitle_narration(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: SubtitleNarrationRequest = serde_json::from_value(args.clone())
        .map_err(|error| format!("Invalid subtitle narration request: {error}"))?;
    if request.items.is_empty() {
        return Err("No subtitle text is available for narration".to_string());
    }

    let source_language_code = request
        .source_language_code
        .as_deref()
        .and_then(normalize_language_to_639_3);
    let (detected_language_code, narration_language_sample) = if source_language_code.is_some() {
        (None, build_narration_language_sample(&request.items))
    } else {
        detect_narration_job_language(&request.items)
    };
    let narration_language_code = source_language_code
        .clone()
        .or(detected_language_code.clone());
    let profile = request
        .profile
        .clone()
        .into_request_profile(narration_language_code.clone());
    let (edge_resolved_voice, edge_language_code, edge_config_voice, edge_voice_source) =
        crate::api::tts::utils::resolve_edge_voice_for_language(
            &profile,
            narration_language_code.as_deref(),
        );
    eprintln!(
        "[Narration][LanguageTrace] start items={} method={:?} source_6393='{}' detected_6393='{}' chosen_6393='{}' edge_lang='{}' edge_voice_source='{}' edge_config_voice='{}' edge_resolved_voice='{}' stored_edge_fallback='{}' sample=\"{}\"",
        request.items.len(),
        profile.method,
        source_language_code.as_deref().unwrap_or(""),
        detected_language_code.as_deref().unwrap_or(""),
        narration_language_code.as_deref().unwrap_or(""),
        edge_language_code,
        edge_voice_source,
        edge_config_voice,
        edge_resolved_voice,
        profile.edge_voice,
        narration_language_sample
            .chars()
            .take(180)
            .collect::<String>()
    );
    eprintln!(
        "[Narration] start request items={} method={:?} language_override='{}' gemini_model='{}' gemini_voice='{}' gemini_speed='{}' google_speed='{}' edge_voice='{}' edge_pitch={} edge_rate={} language_conditions={}",
        request.items.len(),
        profile.method,
        narration_language_code.as_deref().unwrap_or(""),
        profile.gemini_model,
        profile.gemini_voice,
        profile.gemini_speed,
        profile.google_speed,
        profile.edge_voice,
        profile.edge_settings.pitch,
        profile.edge_settings.rate,
        profile.gemini_language_conditions.len()
    );

    let mut jobs = subtitle_narration_jobs()
        .lock()
        .map_err(|_| "Subtitle narration jobs lock poisoned".to_string())?;
    if let Some(active_job_id) = find_active_narration_job_id(&jobs) {
        return Err(format!(
            "Subtitle narration already running (job={active_job_id})"
        ));
    }

    let job_id = uuid();
    let snapshot = Arc::new(Mutex::new(SubtitleNarrationJobSnapshot {
        state: "queued".to_string(),
        message: "Queued subtitle narration".to_string(),
        total_items: request.items.len(),
        ..SubtitleNarrationJobSnapshot::default()
    }));
    let cancelled = Arc::new(AtomicBool::new(false));
    jobs.insert(
        job_id.clone(),
        SubtitleNarrationJobHandle {
            snapshot: snapshot.clone(),
            cancelled: cancelled.clone(),
        },
    );
    drop(jobs);

    let thread_job_id = job_id.clone();
    std::thread::spawn(move || {
        run_subtitle_narration(&thread_job_id, request, profile, snapshot, cancelled)
    });

    Ok(serde_json::json!({ "jobId": job_id }))
}

fn normalize_language_to_639_3(language: &str) -> Option<String> {
    let language = language.trim();
    if language.is_empty() || language.eq_ignore_ascii_case("auto") {
        return None;
    }
    if language.len() == 3 && isolang::Language::from_639_3(language).is_some() {
        return Some(language.to_ascii_lowercase());
    }
    if language.len() == 2
        && let Some(lang) = isolang::Language::from_639_1(language)
    {
        return Some(lang.to_639_3().to_string());
    }
    isolang::Language::from_name(language).map(|lang| lang.to_639_3().to_string())
}

fn build_narration_language_sample(items: &[SubtitleNarrationItemRequest]) -> String {
    items
        .iter()
        .filter_map(|item| {
            let text = item.text.trim();
            (!text.is_empty()).then_some(text)
        })
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_narration_language_sample_from_texts<'a>(
    texts: impl IntoIterator<Item = &'a str>,
) -> String {
    texts
        .into_iter()
        .filter_map(|text| {
            let text = text.trim();
            (!text.is_empty()).then_some(text)
        })
        .take(8)
        .collect::<Vec<_>>()
        .join(" ")
}

fn detect_narration_job_language(
    items: &[SubtitleNarrationItemRequest],
) -> (Option<String>, String) {
    let sample = build_narration_language_sample(items);
    if sample.trim().is_empty() {
        return (None, String::new());
    }
    (crate::lang_detect::detect_language(&sample), sample)
}

pub fn handle_detect_narration_language(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: NarrationLanguageDetectionRequest = serde_json::from_value(args.clone())
        .map_err(|error| format!("Invalid narration language detection request: {error}"))?;
    let sample = build_narration_language_sample_from_texts(
        request.items.iter().map(|item| item.text.as_str()),
    );
    let language_code = if sample.trim().is_empty() {
        None
    } else {
        crate::lang_detect::detect_language(&sample)
    };
    Ok(serde_json::json!({
        "languageCode": language_code,
        "sample": sample,
    }))
}

pub fn handle_get_subtitle_narration_status(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let job_id = args["jobId"].as_str().ok_or("Missing jobId")?;
    let known_results_revision = args["knownResultsRevision"].as_u64().unwrap_or(0) as usize;
    let known_error_count = args["knownErrorCount"].as_u64().unwrap_or(0) as usize;
    let jobs = subtitle_narration_jobs()
        .lock()
        .map_err(|_| "Subtitle narration jobs lock poisoned".to_string())?;
    let handle = jobs
        .get(job_id)
        .ok_or_else(|| format!("Unknown subtitle narration job: {job_id}"))?;
    let snapshot = handle
        .snapshot
        .lock()
        .map_err(|_| "Subtitle narration snapshot lock poisoned".to_string())?;

    let latest_results_revision = snapshot.results_revision;
    let (results_revision, results): (usize, Vec<SubtitleNarrationResult>) =
        if latest_results_revision <= known_results_revision {
            (latest_results_revision, Vec::new())
        } else if matches!(snapshot.state.as_str(), "completed" | "cancelled" | "error") {
            let results = snapshot
                .result_events
                .iter()
                .filter(|event| event.revision > known_results_revision)
                .map(|event| event.result.clone())
                .collect();
            (latest_results_revision, results)
        } else if let Some(next_event) = snapshot
            .result_events
            .iter()
            .find(|event| event.revision > known_results_revision)
        {
            (next_event.revision, vec![next_event.result.clone()])
        } else {
            (latest_results_revision, Vec::new())
        };
    let errors: Vec<SubtitleNarrationError> = if known_error_count > 0 {
        snapshot
            .errors
            .iter()
            .skip(known_error_count)
            .cloned()
            .collect()
    } else {
        snapshot.errors.clone()
    };

    serde_json::to_value(serde_json::json!({
        "state": snapshot.state,
        "message": snapshot.message,
        "progress": snapshot.progress,
        "totalItems": snapshot.total_items,
        "completedItems": snapshot.completed_items,
        "activeSubtitleId": snapshot.active_subtitle_id,
        "resultsRevision": results_revision,
        "results": results,
        "errors": errors,
        "error": snapshot.error,
    }))
    .map_err(|error| format!("Serialize subtitle narration status: {error}"))
}

/// Returns the same option metadata the TTS Playground exposes so the
/// Narration tab on the frontend can render the *real* lists (voices, models,
/// language conditions, edge voices) and the canonical defaults.
mod metadata;
pub use metadata::handle_get_narration_tts_metadata;

pub fn handle_cancel_subtitle_narration(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let job_id = args["jobId"].as_str().ok_or("Missing jobId")?;
    let jobs = subtitle_narration_jobs()
        .lock()
        .map_err(|_| "Subtitle narration jobs lock poisoned".to_string())?;
    let handle = jobs
        .get(job_id)
        .ok_or_else(|| format!("Unknown subtitle narration job: {job_id}"))?;
    handle.cancelled.store(true, Ordering::SeqCst);
    if let Ok(mut snapshot) = handle.snapshot.lock() {
        snapshot.state = "cancelled".to_string();
        snapshot.message = "Subtitle narration cancelled".to_string();
        snapshot.active_subtitle_id = None;
    }
    Ok(serde_json::Value::Null)
}

mod alignment;
use alignment::*;

mod runner;
use runner::run_subtitle_narration;

mod parallel;
use parallel::run_gemini_subtitle_narration_parallel;

mod text;
use text::*;

mod synthesis;
use synthesis::{
    GeminiNarrationRetryRequest, NarrationSynthesisAttempt,
    synthesize_gemini_narration_group_with_retries, synthesize_narration_item_with_retries,
};

fn update_snapshot(
    snapshot: &Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    updater: impl FnOnce(&mut SubtitleNarrationJobSnapshot),
) -> Result<(), String> {
    let mut locked = snapshot
        .lock()
        .map_err(|_| "Subtitle narration snapshot lock poisoned".to_string())?;
    updater(&mut locked);
    Ok(())
}

fn uuid() -> String {
    format!(
        "subtitle-narration-{}-{}",
        chrono::Utc::now().timestamp_millis(),
        std::process::id()
    )
}

#[cfg(test)]
mod tests;
