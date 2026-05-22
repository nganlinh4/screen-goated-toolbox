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
    KOKORO_VOICES, MAGPIE_VOICE_LANGUAGES, MAGPIE_VOICES, SUPERTONIC_LANGUAGES, SUPERTONIC_VOICES,
    narration_tts_providers, normalize_kokoro_lang, normalize_magpie_voice, tts_method_id,
};
use crate::config::{
    EdgeTtsSettings, EdgeTtsVoiceConfig, KokoroSettings, KokoroVoiceConfig, MagpieSettings,
    MagpieVoiceConfig, StepAudioVoiceConfig, SupertonicSettings, SupertonicVoiceConfig,
    TtsLanguageCondition, TtsMethod, TtsPlaygroundSettings,
};
use crate::gui::settings_ui::tts_playground_data::{
    GEMINI_VOICES, SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES,
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

/// Wire shape for a Gemini language-instruction condition. Uses camelCase to
/// match what the frontend serializes; converts into `TtsLanguageCondition`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanguageConditionWire {
    language_code: String,
    language_name: String,
    #[serde(default)]
    instruction: String,
}

impl From<LanguageConditionWire> for TtsLanguageCondition {
    fn from(wire: LanguageConditionWire) -> Self {
        TtsLanguageCondition::new(&wire.language_code, &wire.language_name, &wire.instruction)
    }
}

/// Wire shape for an Edge TTS per-language voice config. Mirrors
/// `EdgeTtsVoiceConfig` with camelCase keys for the WebView frontend.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdgeVoiceConfigWire {
    language_code: String,
    language_name: String,
    voice_name: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct KokoroVoiceConfigWire {
    language_code: String,
    language_name: String,
    voice_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MagpieVoiceConfigWire {
    language_code: String,
    language_name: String,
    voice_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StepAudioVoiceConfigWire {
    language_code: String,
    language_name: String,
    voice_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SupertonicVoiceConfigWire {
    language_code: String,
    language_name: String,
    voice_id: String,
}

impl From<KokoroVoiceConfigWire> for KokoroVoiceConfig {
    fn from(wire: KokoroVoiceConfigWire) -> Self {
        KokoroVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_id)
    }
}

impl From<EdgeVoiceConfigWire> for EdgeTtsVoiceConfig {
    fn from(wire: EdgeVoiceConfigWire) -> Self {
        EdgeTtsVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_name)
    }
}

impl From<MagpieVoiceConfigWire> for MagpieVoiceConfig {
    fn from(wire: MagpieVoiceConfigWire) -> Self {
        MagpieVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_id)
    }
}

impl From<StepAudioVoiceConfigWire> for StepAudioVoiceConfig {
    fn from(wire: StepAudioVoiceConfigWire) -> Self {
        StepAudioVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_id)
    }
}

impl From<SupertonicVoiceConfigWire> for SupertonicVoiceConfig {
    fn from(wire: SupertonicVoiceConfigWire) -> Self {
        SupertonicVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_id)
    }
}

/// Wire shape for the per-request TTS profile. Mirrors the user's narration
/// settings on the frontend; gets converted into `TtsRequestProfile` here so
/// callers don't have to touch `app.config.tts_playground`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TtsProfileWire {
    method: TtsMethod,
    #[serde(default)]
    gemini_model: String,
    #[serde(default)]
    gemini_voice: String,
    #[serde(default)]
    gemini_speed: String,
    #[serde(default)]
    gemini_instruction: String,
    #[serde(default)]
    gemini_language_conditions: Vec<LanguageConditionWire>,
    #[serde(default = "default_gemini_parallel_requests")]
    gemini_parallel_requests: usize,
    #[serde(default)]
    google_speed: String,
    #[serde(default)]
    edge_voice: String,
    #[serde(default)]
    edge_pitch: i32,
    #[serde(default)]
    edge_rate: i32,
    #[serde(default)]
    edge_voice_configs: Vec<EdgeVoiceConfigWire>,
    #[serde(default)]
    step_audio_voice: String,
    #[serde(default)]
    step_audio_reference_voice_id: String,
    #[serde(default)]
    step_audio_voice_configs: Vec<StepAudioVoiceConfigWire>,
    #[serde(default)]
    step_audio_prompt_text: String,
    #[serde(default)]
    step_audio_use_custom_reference: bool,
    #[serde(default)]
    step_audio_reference_audio_path: String,
    #[serde(default)]
    step_audio_reference_text: String,
    #[serde(default)]
    step_audio_reference_label: String,
    #[serde(default)]
    magpie_voice: String,
    #[serde(default)]
    magpie_voice_configs: Vec<MagpieVoiceConfigWire>,
    #[serde(default)]
    kokoro_voice: String,
    #[serde(default)]
    kokoro_speed: Option<f32>,
    #[serde(default)]
    kokoro_num_threads: Option<i32>,
    #[serde(default)]
    kokoro_voice_configs: Vec<KokoroVoiceConfigWire>,
    #[serde(default)]
    supertonic_speed: Option<f32>,
    #[serde(default)]
    supertonic_num_steps: Option<i32>,
    #[serde(default)]
    supertonic_num_threads: Option<i32>,
    #[serde(default)]
    supertonic_voice_configs: Vec<SupertonicVoiceConfigWire>,
    #[serde(default)]
    vieneu_variant: String,
    #[serde(default)]
    vieneu_emotion: String,
    #[serde(default)]
    vieneu_reference_voice_id: String,
}

fn default_gemini_parallel_requests() -> usize {
    2
}

fn default_gemini_s2s_parallel_requests() -> usize {
    3
}

impl TtsProfileWire {
    fn into_request_profile(self, language_code_override: Option<String>) -> TtsRequestProfile {
        // Pull catalog defaults whenever the frontend left a field blank,
        // so a fresh narration tab "just works" without forcing the user to
        // choose every value before the first run.
        let defaults = crate::config::TtsPlaygroundSettings::default();
        let trimmed_or = |value: String, fallback: String| -> String {
            if value.trim().is_empty() {
                fallback
            } else {
                value
            }
        };
        let edge_voice_configs: Vec<EdgeTtsVoiceConfig> = if self.edge_voice_configs.is_empty() {
            defaults.edge_settings.voice_configs
        } else {
            self.edge_voice_configs
                .into_iter()
                .map(EdgeTtsVoiceConfig::from)
                .collect()
        };
        let kokoro_voice = trimmed_or(self.kokoro_voice, defaults.kokoro_settings.voice.clone());
        let magpie_voice = normalize_magpie_voice(&trimmed_or(
            self.magpie_voice,
            defaults.magpie_settings.voice.clone(),
        ));
        let magpie_voice_configs = if self.magpie_voice_configs.is_empty() {
            defaults.magpie_settings.voice_configs
        } else {
            self.magpie_voice_configs
                .into_iter()
                .map(MagpieVoiceConfig::from)
                .collect()
        };
        let kokoro_voice_configs = if self.kokoro_voice_configs.is_empty() {
            defaults.kokoro_settings.voice_configs
        } else {
            self.kokoro_voice_configs
                .into_iter()
                .map(KokoroVoiceConfig::from)
                .collect()
        };
        let supertonic_voice_configs = if self.supertonic_voice_configs.is_empty() {
            defaults.supertonic_settings.voice_configs
        } else {
            self.supertonic_voice_configs
                .into_iter()
                .map(SupertonicVoiceConfig::from)
                .collect()
        };

        TtsRequestProfile {
            method: self.method,
            gemini_model: trimmed_or(self.gemini_model, defaults.gemini_model),
            gemini_voice: trimmed_or(self.gemini_voice, defaults.gemini_voice),
            gemini_speed: trimmed_or(self.gemini_speed, defaults.gemini_speed),
            gemini_instruction: self.gemini_instruction,
            gemini_language_conditions: if self.gemini_language_conditions.is_empty() {
                defaults.gemini_language_conditions
            } else {
                self.gemini_language_conditions
                    .into_iter()
                    .map(TtsLanguageCondition::from)
                    .collect()
            },
            gemini_parallel_requests: self.gemini_parallel_requests.clamp(1, 4),
            google_speed: trimmed_or(self.google_speed, defaults.google_speed),
            edge_voice: trimmed_or(self.edge_voice, defaults.edge_voice),
            edge_settings: EdgeTtsSettings {
                pitch: self.edge_pitch,
                rate: self.edge_rate,
                volume: 0,
                voice_configs: edge_voice_configs,
            },
            step_audio_settings: crate::config::StepAudioSettings {
                voice: trimmed_or(
                    self.step_audio_voice,
                    defaults.step_audio_settings.voice.clone(),
                ),
                voice_configs: if self.step_audio_voice_configs.is_empty() {
                    defaults.step_audio_settings.voice_configs
                } else {
                    self.step_audio_voice_configs
                        .into_iter()
                        .map(StepAudioVoiceConfig::from)
                        .collect()
                },
                reference_voice_id: self.step_audio_reference_voice_id,
                use_custom_reference: self.step_audio_use_custom_reference,
                reference_audio_path: self.step_audio_reference_audio_path,
                reference_text: self.step_audio_reference_text,
                reference_label: self.step_audio_reference_label,
                style_prompt: self.step_audio_prompt_text,
            },
            magpie_settings: MagpieSettings {
                voice: magpie_voice,
                voice_configs: magpie_voice_configs,
            },
            kokoro_settings: KokoroSettings {
                voice: kokoro_voice,
                speed: self
                    .kokoro_speed
                    .unwrap_or(defaults.kokoro_settings.speed)
                    .clamp(0.5, 2.0),
                lang: String::new(),
                num_threads: self
                    .kokoro_num_threads
                    .unwrap_or(defaults.kokoro_settings.num_threads)
                    .clamp(1, 8),
                voice_configs: kokoro_voice_configs,
            },
            supertonic_settings: SupertonicSettings {
                speaker_id: defaults.supertonic_settings.speaker_id,
                speed: self
                    .supertonic_speed
                    .unwrap_or(defaults.supertonic_settings.speed)
                    .clamp(0.5, 2.0),
                num_steps: self
                    .supertonic_num_steps
                    .unwrap_or(defaults.supertonic_settings.num_steps)
                    .clamp(1, 20),
                num_threads: self
                    .supertonic_num_threads
                    .unwrap_or(defaults.supertonic_settings.num_threads)
                    .clamp(1, 8),
                lang: String::new(),
                voice_configs: supertonic_voice_configs,
                silence_duration: defaults.supertonic_settings.silence_duration,
                seed: defaults.supertonic_settings.seed,
            },
            vieneu_settings: crate::config::VieneuSettings {
                variant: trimmed_or(self.vieneu_variant, defaults.vieneu_settings.variant),
                emotion: trimmed_or(self.vieneu_emotion, defaults.vieneu_settings.emotion),
                reference_voice_id: self.vieneu_reference_voice_id,
                use_custom_reference: false,
                reference_audio_path: String::new(),
                reference_text: String::new(),
                reference_label: String::new(),
            },
            language_code_override,
        }
    }
}

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
pub fn handle_get_narration_tts_metadata(
    _args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    crate::api::tts::edge_voices::load_edge_voices_async();
    let defaults = TtsPlaygroundSettings::default();

    let gemini_voices: Vec<serde_json::Value> = GEMINI_VOICES
        .iter()
        .map(|(name, gender)| serde_json::json!({ "name": name, "gender": gender }))
        .collect();

    let gemini_models: Vec<serde_json::Value> = tts_gemini_model_options()
        .iter()
        .map(|(api_model, label)| serde_json::json!({ "apiModel": api_model, "label": label }))
        .collect();

    let gemini_instruction_languages: Vec<serde_json::Value> =
        SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES
            .iter()
            .map(|(code, name)| serde_json::json!({ "languageCode": code, "languageName": name }))
            .collect();

    let default_language_conditions: Vec<serde_json::Value> = defaults
        .gemini_language_conditions
        .iter()
        .map(|condition| {
            serde_json::json!({
                "languageCode": condition.language_code,
                "languageName": condition.language_name,
                "instruction": condition.instruction,
            })
        })
        .collect();

    let default_edge_voice_configs: Vec<serde_json::Value> = defaults
        .edge_settings
        .voice_configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "languageCode": config.language_code,
                "languageName": config.language_name,
                "voiceName": config.voice_name,
            })
        })
        .collect();

    let (edge_voice_state, edge_voice_error, edge_voice_languages, edge_voices_by_language) = {
        let cache = crate::api::tts::edge_voices::EDGE_VOICE_CACHE
            .lock()
            .map_err(|_| "Lock Edge TTS voice cache".to_string())?;
        let state = if cache.loaded {
            "loaded"
        } else if cache.loading {
            "loading"
        } else if cache.error.is_some() {
            "error"
        } else {
            "idle"
        };

        let mut language_names = std::collections::HashMap::<String, String>::new();
        for voice in &cache.voices {
            let lang_code = voice
                .locale
                .split('-')
                .next()
                .unwrap_or(&voice.locale)
                .to_lowercase();
            language_names.entry(lang_code).or_insert_with(|| {
                voice
                    .friendly_name
                    .rfind(" - ")
                    .and_then(|dash_pos| {
                        let lang_region = &voice.friendly_name[dash_pos + 3..];
                        lang_region
                            .find(" (")
                            .map(|paren_pos| lang_region[..paren_pos].to_string())
                            .or_else(|| Some(lang_region.to_string()))
                    })
                    .unwrap_or_else(|| voice.locale.clone())
            });
        }

        let mut languages: Vec<serde_json::Value> = language_names
            .iter()
            .map(|(code, name)| {
                serde_json::json!({
                    "languageCode": code,
                    "languageName": name,
                })
            })
            .collect();
        languages.sort_by(|left, right| {
            let left_name = left["languageName"].as_str().unwrap_or_default();
            let right_name = right["languageName"].as_str().unwrap_or_default();
            left_name.cmp(right_name)
        });

        let voices_by_language = cache
            .by_language
            .iter()
            .map(|(code, voices)| {
                let options: Vec<serde_json::Value> = voices
                    .iter()
                    .map(|voice| {
                        serde_json::json!({
                            "shortName": voice.short_name,
                            "gender": voice.gender,
                            "friendlyName": voice.friendly_name,
                            "locale": voice.locale,
                        })
                    })
                    .collect();
                (code.clone(), serde_json::Value::Array(options))
            })
            .collect::<serde_json::Map<_, _>>();

        (
            state,
            cache.error.clone(),
            languages,
            serde_json::Value::Object(voices_by_language),
        )
    };

    let providers: Vec<serde_json::Value> = narration_tts_providers()
        .map(|provider| {
            serde_json::json!({
                "method": provider.id,
                "label": provider.label,
            })
        })
        .collect();
    let kokoro_voice_languages: Vec<serde_json::Value> = SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES
        .iter()
        .filter(|(code, _)| normalize_kokoro_lang(code).is_some())
        .map(|(code, name)| serde_json::json!({ "languageCode": code, "languageName": name }))
        .collect();
    let kokoro_voices: Vec<serde_json::Value> = KOKORO_VOICES
        .iter()
        .map(|voice| {
            serde_json::json!({
                "id": voice.id,
                "label": voice.label,
                "languageCode": voice.language_code,
            })
        })
        .collect();
    let magpie_voices: Vec<serde_json::Value> = MAGPIE_VOICES
        .iter()
        .map(|voice| {
            serde_json::json!({
                "id": voice.id,
                "label": voice.label,
            })
        })
        .collect();
    let magpie_voice_languages: Vec<serde_json::Value> = MAGPIE_VOICE_LANGUAGES
        .iter()
        .map(|(code, name)| serde_json::json!({ "languageCode": code, "languageName": name }))
        .collect();
    let supertonic_languages: Vec<serde_json::Value> = SUPERTONIC_LANGUAGES
        .iter()
        .map(|lang| serde_json::json!({ "languageCode": lang.code, "languageName": lang.label }))
        .collect();
    let supertonic_voices: Vec<serde_json::Value> = SUPERTONIC_VOICES
        .iter()
        .map(|voice| {
            serde_json::json!({
                "id": voice.id,
                "label": voice.label,
            })
        })
        .collect();
    let step_audio_reference_voices: Vec<serde_json::Value> = crate::APP
        .lock()
        .map(|app| {
            app.config
                .step_audio_reference_voices
                .iter()
                .map(|reference| {
                    serde_json::json!({
                        "id": reference.id,
                        "label": reference.label,
                        "audioPath": reference.audio_path,
                        "transcript": reference.transcript,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let step_audio_voices: Vec<serde_json::Value> = step_audio_reference_voices
        .iter()
        .filter_map(|voice| {
            let id = voice.get("id")?.as_str()?;
            let label = voice
                .get("label")
                .and_then(|value| value.as_str())
                .unwrap_or(id);
            Some(serde_json::json!({
                "id": id,
                "label": if label.trim().is_empty() { "Untitled reference" } else { label },
            }))
        })
        .collect();
    let step_audio_voice_languages: Vec<serde_json::Value> = defaults
        .step_audio_settings
        .voice_configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "languageCode": config.language_code,
                "languageName": config.language_name,
            })
        })
        .collect();
    let default_method = tts_method_id(&defaults.method);
    let default_magpie_voice_configs: Vec<serde_json::Value> = defaults
        .magpie_settings
        .voice_configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "languageCode": config.language_code,
                "languageName": config.language_name,
                "voiceId": config.voice_id,
            })
        })
        .collect();
    let default_kokoro_voice_configs: Vec<serde_json::Value> = defaults
        .kokoro_settings
        .voice_configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "languageCode": config.language_code,
                "languageName": config.language_name,
                "voiceId": config.voice_id,
            })
        })
        .collect();
    let default_supertonic_voice_configs: Vec<serde_json::Value> = defaults
        .supertonic_settings
        .voice_configs
        .iter()
        .map(|config| {
            serde_json::json!({
                "languageCode": config.language_code,
                "languageName": config.language_name,
                "voiceId": config.voice_id,
            })
        })
        .collect();
    let defaults_json = serde_json::json!({
        "method": default_method,
        "geminiModel": defaults.gemini_model,
        "geminiVoice": defaults.gemini_voice,
        "geminiSpeed": defaults.gemini_speed,
        "geminiInstruction": defaults.gemini_instruction,
        "geminiLanguageConditions": default_language_conditions,
        "geminiParallelRequests": default_gemini_parallel_requests(),
        "geminiS2sParallelRequests": default_gemini_s2s_parallel_requests(),
        "googleSpeed": defaults.google_speed,
        "edgeVoice": defaults.edge_voice,
        "edgePitch": defaults.edge_settings.pitch,
        "edgeRate": defaults.edge_settings.rate,
        "edgeVoiceConfigs": default_edge_voice_configs,
        "stepAudioVoice": defaults.step_audio_settings.voice,
        "stepAudioReferenceVoiceId": defaults.step_audio_settings.reference_voice_id,
        "stepAudioPromptText": defaults.step_audio_settings.style_prompt,
        "stepAudioUseCustomReference": defaults.step_audio_settings.use_custom_reference,
        "stepAudioReferenceAudioPath": defaults.step_audio_settings.reference_audio_path,
        "stepAudioReferenceText": defaults.step_audio_settings.reference_text,
        "stepAudioReferenceLabel": defaults.step_audio_settings.reference_label,
        "magpieVoice": defaults.magpie_settings.voice,
        "magpieVoiceConfigs": default_magpie_voice_configs,
        "kokoroVoice": defaults.kokoro_settings.voice,
        "kokoroSpeed": defaults.kokoro_settings.speed,
        "kokoroNumThreads": defaults.kokoro_settings.num_threads,
        "kokoroVoiceConfigs": default_kokoro_voice_configs,
        "supertonicSpeed": defaults.supertonic_settings.speed,
        "supertonicNumSteps": defaults.supertonic_settings.num_steps,
        "supertonicNumThreads": defaults.supertonic_settings.num_threads,
        "supertonicVoiceConfigs": default_supertonic_voice_configs,
        "vieneuVariant": defaults.vieneu_settings.variant,
        "vieneuEmotion": defaults.vieneu_settings.emotion,
        "vieneuReferenceVoiceId": defaults.vieneu_settings.reference_voice_id,
    });

    Ok(serde_json::json!({
        "providers": providers,
        "geminiVoices": gemini_voices,
        "geminiModels": gemini_models,
        "geminiInstructionLanguages": gemini_instruction_languages,
        "geminiSpeedOptions": ["Slow", "Normal", "Fast"],
        "googleSpeedOptions": ["Slow", "Normal"],
        "kokoroVoices": kokoro_voices,
        "kokoroVoiceLanguages": kokoro_voice_languages,
        "magpieVoices": magpie_voices,
        "magpieVoiceLanguages": magpie_voice_languages,
        "supertonicLanguages": supertonic_languages,
        "supertonicVoices": supertonic_voices,
        "stepAudioVoices": step_audio_voices,
        "stepAudioVoiceLanguages": step_audio_voice_languages,
        "stepAudioReferenceVoices": step_audio_reference_voices,
        "edgeVoiceState": edge_voice_state,
        "edgeVoiceError": edge_voice_error,
        "edgeVoiceLanguages": edge_voice_languages,
        "edgeVoicesByLanguage": edge_voices_by_language,
        "defaults": defaults_json,
    }))
}

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

#[derive(Clone)]
struct CleanNarrationItem {
    id: String,
    text: String,
    tts_text: String,
    aligner_text: String,
    start_time: f64,
    end_time: f64,
    text_units: usize,
}

#[derive(Clone)]
struct NarrationRequestGroup {
    id: String,
    items: Vec<CleanNarrationItem>,
    text: String,
    spans: Vec<NarrationGroupTextSpan>,
}

#[derive(Clone)]
struct NarrationGroupTextSpan {
    subtitle_id: String,
    text: String,
    start_char: usize,
    end_char: usize,
}

#[derive(Clone)]
struct NarrationAlignedRange {
    start_sec: f64,
    end_sec: f64,
    confidence: f64,
}

struct NarrationSplitResult {
    ranges: Vec<NarrationAlignedRange>,
    mode: &'static str,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NarrationAlignerRequest<'a> {
    audio_path: &'a str,
    prompt_text: &'a str,
    language_code: Option<&'a str>,
    items: Vec<NarrationAlignerItem<'a>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NarrationAlignerItem<'a> {
    subtitle_id: &'a str,
    text: &'a str,
    start_char: usize,
    end_char: usize,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NarrationAlignerResponse {
    ranges: Vec<NarrationAlignerRange>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NarrationAlignerRange {
    subtitle_id: String,
    source_in_point: f64,
    source_out_point: f64,
    #[serde(default = "default_alignment_confidence")]
    confidence: f64,
}

fn default_alignment_confidence() -> f64 {
    1.0
}

fn estimate_narration_speech_units(text: &str) -> usize {
    let mut word_count = 0usize;
    let mut in_word = false;
    let mut alnum_chars = 0usize;
    let mut has_whitespace = false;
    for ch in text.nfc() {
        if ch.is_whitespace() {
            has_whitespace = true;
            if in_word {
                word_count += 1;
                in_word = false;
            }
            continue;
        }
        if ch.is_alphanumeric() {
            alnum_chars += 1;
            in_word = true;
        } else if in_word {
            word_count += 1;
            in_word = false;
        }
    }
    if in_word {
        word_count += 1;
    }
    if has_whitespace && word_count > 1 {
        word_count
    } else {
        ((alnum_chars + 3) / 4).max(1)
    }
}

fn normalize_group_sentence(text: &str) -> String {
    let mut trimmed = text.trim().trim_matches(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '"' | '\'' | '`' | '*' | '_' | '~' | '|' | '♪' | '♫' | '♩' | '♬'
            )
    });
    while trimmed
        .chars()
        .last()
        .is_some_and(|ch| matches!(ch, ',' | ';' | ':' | '-' | '—' | '–' | '.' | '…'))
    {
        trimmed = trimmed
            .trim_end_matches(|ch| matches!(ch, ',' | ';' | ':' | '-' | '—' | '–' | '.' | '…'))
            .trim_end();
    }
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed
        .chars()
        .last()
        .is_some_and(|ch| matches!(ch, '!' | '?' | '！' | '？' | '。'))
    {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

fn normalize_alignment_text(text: &str) -> String {
    text.nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn append_group_sentence(
    text_parts: &mut Vec<String>,
    spans: &mut Vec<NarrationGroupTextSpan>,
    item: &CleanNarrationItem,
    cursor: &mut usize,
) {
    let sentence = normalize_group_sentence(&item.tts_text);
    if sentence.is_empty() {
        return;
    }
    if !text_parts.is_empty() {
        *cursor += 1;
    }
    let start_char = *cursor;
    *cursor += sentence.chars().count();
    spans.push(NarrationGroupTextSpan {
        subtitle_id: item.id.clone(),
        text: item.aligner_text.clone(),
        start_char,
        end_char: *cursor,
    });
    text_parts.push(sentence);
}

fn build_group_text_and_spans(
    items: &[CleanNarrationItem],
) -> (String, Vec<NarrationGroupTextSpan>) {
    let mut text_parts = Vec::new();
    let mut spans = Vec::new();
    let mut cursor = 0usize;
    for item in items {
        append_group_sentence(&mut text_parts, &mut spans, item, &mut cursor);
    }
    (text_parts.join(" "), spans)
}

fn build_narration_groups(
    items: Vec<CleanNarrationItem>,
    grouping: &SubtitleNarrationGroupingRequest,
) -> Vec<NarrationRequestGroup> {
    let text_budget = grouping.text_budget_units.clamp(
        NARRATION_GROUP_MIN_TEXT_BUDGET,
        NARRATION_GROUP_MAX_TEXT_BUDGET,
    );
    let mut groups = Vec::new();
    let mut current = Vec::<CleanNarrationItem>::new();
    let mut current_units = 0usize;
    let mut current_chars = 0usize;
    let mut previous_end: Option<f64> = None;

    let flush_current = |groups: &mut Vec<NarrationRequestGroup>,
                         current: &mut Vec<CleanNarrationItem>,
                         current_units: &mut usize,
                         current_chars: &mut usize| {
        if current.is_empty() {
            return;
        }
        let group_index = groups.len();
        let (text, spans) = build_group_text_and_spans(current);
        groups.push(NarrationRequestGroup {
            id: format!("group-{group_index}"),
            items: std::mem::take(current),
            text,
            spans,
        });
        *current_units = 0;
        *current_chars = 0;
    };

    for item in items {
        let item_chars = item.tts_text.chars().count();
        let gap = previous_end.map_or(0.0, |end| item.start_time - end);
        let should_start_new = !current.is_empty()
            && (current.len() >= NARRATION_GROUP_MAX_ITEMS
                || current_chars + item_chars > NARRATION_GROUP_MAX_CHARS
                || gap > NARRATION_GROUP_GAP_BREAK_SEC
                || current_units + item.text_units > text_budget);
        if should_start_new {
            flush_current(
                &mut groups,
                &mut current,
                &mut current_units,
                &mut current_chars,
            );
        }
        previous_end = Some(item.end_time);
        current_units += item.text_units;
        current_chars += item_chars;
        current.push(item);
    }
    flush_current(
        &mut groups,
        &mut current,
        &mut current_units,
        &mut current_chars,
    );
    groups
}

fn split_group_audio_ranges(
    group: &NarrationRequestGroup,
    audio: &TtsCollectedAudio,
    vad_search_radius_sec: f64,
) -> NarrationSplitResult {
    let total_items = group.items.len();
    let duration_sec = (audio.duration_ms as f64 / 1000.0).max(0.05);
    if total_items <= 1 {
        return NarrationSplitResult {
            ranges: vec![NarrationAlignedRange {
                start_sec: 0.0,
                end_sec: duration_sec,
                confidence: 1.0,
            }],
            mode: "single",
        };
    }
    if audio.pcm_samples.len() < total_items {
        return NarrationSplitResult {
            ranges: (0..total_items)
                .map(|index| {
                    let start = duration_sec * index as f64 / total_items as f64;
                    let end = duration_sec * (index + 1) as f64 / total_items as f64;
                    NarrationAlignedRange {
                        start_sec: start,
                        end_sec: end.max(start + 0.05).min(duration_sec),
                        confidence: 0.25,
                    }
                })
                .collect(),
            mode: "estimated",
        };
    }
    NarrationSplitResult {
        ranges: split_group_audio_ranges_estimated(group, audio, vad_search_radius_sec),
        mode: "estimated",
    }
}

fn split_group_audio_ranges_estimated(
    group: &NarrationRequestGroup,
    audio: &TtsCollectedAudio,
    vad_search_radius_sec: f64,
) -> Vec<NarrationAlignedRange> {
    let total_items = group.items.len();
    let duration_sec = (audio.duration_ms as f64 / 1000.0).max(0.05);
    let punctuation_total: f64 = group
        .items
        .iter()
        .map(|item| narration_pause_weight(&item.tts_text))
        .sum::<f64>()
        .max(0.0);
    let text_total: f64 = group
        .items
        .iter()
        .map(|item| item.text_units.max(1) as f64 + narration_pause_weight(&item.tts_text))
        .sum::<f64>()
        .max(1.0);
    let time_total: f64 = group
        .items
        .iter()
        .map(|item| (item.end_time - item.start_time).max(0.05))
        .sum::<f64>()
        .max(0.05);
    let weights = group
        .items
        .iter()
        .map(|item| {
            let text_ratio = (item.text_units.max(1) as f64
                + narration_pause_weight(&item.tts_text))
                / text_total;
            let time_ratio = (item.end_time - item.start_time).max(0.05) / time_total;
            (text_ratio * 0.82 + time_ratio * 0.18).max(0.001)
        })
        .collect::<Vec<_>>();

    let sample_rate = audio.sample_rate.max(1);
    let total_frames = audio.pcm_samples.len();
    let mut cumulative = 0.0;
    let ideal_frames = weights
        .iter()
        .take(total_items - 1)
        .map(|weight| {
            cumulative += *weight;
            ((cumulative * total_frames as f64).round() as usize)
                .min(total_frames.saturating_sub(1))
        })
        .collect::<Vec<_>>();
    let snapped_frames = if total_frames > 2 {
        snap_split_frames_to_silence(
            &audio.pcm_samples,
            1,
            sample_rate,
            &ideal_frames,
            vad_search_radius_sec.clamp(0.05, 1.0),
        )
    } else {
        ideal_frames
    };

    let min_gap_frames = ((sample_rate as f64 * 0.05).round() as usize).max(1);
    let candidate_frames = if snapped_frames.len() == total_items - 1 {
        snapped_frames
    } else {
        (1..total_items)
            .map(|index| total_frames * index / total_items)
            .collect::<Vec<_>>()
    };
    let mut boundaries = Vec::with_capacity(total_items + 1);
    boundaries.push(0usize);
    let mut previous = 0usize;
    for frame in candidate_frames {
        let min_next = previous.saturating_add(min_gap_frames);
        let max_next = total_frames.saturating_sub(min_gap_frames);
        let boundary = frame.max(min_next).min(max_next);
        if boundary > previous && boundary < total_frames {
            boundaries.push(boundary);
            previous = boundary;
        }
    }
    if boundaries.len() != total_items {
        boundaries = (0..=total_items)
            .map(|index| total_frames * index / total_items)
            .collect();
    } else {
        boundaries.push(total_frames);
    }
    let base_confidence = if punctuation_total > 0.0 { 0.5 } else { 0.42 };
    boundaries
        .windows(2)
        .take(total_items)
        .map(|pair| {
            let start = pair[0] as f64 / sample_rate as f64;
            let end = pair[1] as f64 / sample_rate as f64;
            NarrationAlignedRange {
                start_sec: start.min(duration_sec),
                end_sec: end.max(start + 0.05).min(duration_sec),
                confidence: base_confidence,
            }
        })
        .collect()
}

fn narration_pause_weight(text: &str) -> f64 {
    let trimmed = text.trim();
    let Some(last) = trimmed.chars().rev().find(|ch| !ch.is_whitespace()) else {
        return 0.0;
    };
    match last {
        '.' | '。' => 0.9,
        '!' | '?' | '！' | '？' => 1.1,
        '…' => 1.2,
        ',' | ';' | ':' => 0.45,
        _ => 0.0,
    }
}

fn align_group_audio_ranges(
    group: &NarrationRequestGroup,
    audio_path: &str,
    audio: &TtsCollectedAudio,
    language_code: Option<&str>,
    vad_search_radius_sec: f64,
) -> NarrationSplitResult {
    if let Some(aligned) = try_external_narration_aligner(group, audio_path, audio, language_code) {
        return aligned;
    }
    split_group_audio_ranges(group, audio, vad_search_radius_sec)
}

fn try_external_narration_aligner(
    group: &NarrationRequestGroup,
    audio_path: &str,
    audio: &TtsCollectedAudio,
    language_code: Option<&str>,
) -> Option<NarrationSplitResult> {
    let command = std::env::var(NARRATION_ALIGNER_ENV).ok()?;
    let command = command.trim();
    if command.is_empty() {
        return None;
    }

    let items = group
        .spans
        .iter()
        .map(|span| NarrationAlignerItem {
            subtitle_id: &span.subtitle_id,
            text: &span.text,
            start_char: span.start_char,
            end_char: span.end_char,
        })
        .collect();
    let request = NarrationAlignerRequest {
        audio_path,
        prompt_text: &group.text,
        language_code,
        items,
    };
    let payload = serde_json::to_vec(&request).ok()?;
    let mut child = std::process::Command::new(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    if let Some(stdin) = child.stdin.as_mut()
        && stdin.write_all(&payload).is_err()
    {
        let _ = child.kill();
        return None;
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        eprintln!(
            "[Narration][Aligner] command failed status={:?} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(600)
                .collect::<String>()
        );
        return None;
    }
    let response: NarrationAlignerResponse = serde_json::from_slice(&output.stdout).ok()?;
    if response.ranges.len() != group.items.len() {
        return None;
    }
    let duration_sec = (audio.duration_ms as f64 / 1000.0).max(0.05);
    let mut by_id = response
        .ranges
        .into_iter()
        .map(|range| (range.subtitle_id.clone(), range))
        .collect::<HashMap<_, _>>();
    let mut previous_end = 0.0;
    let mut ranges = Vec::with_capacity(group.items.len());
    for item in &group.items {
        let range = by_id.remove(&item.id)?;
        let start = range
            .source_in_point
            .max(previous_end)
            .clamp(0.0, duration_sec);
        let end = range.source_out_point.max(start + 0.05).min(duration_sec);
        previous_end = end;
        ranges.push(NarrationAlignedRange {
            start_sec: start,
            end_sec: end,
            confidence: range.confidence.clamp(0.0, 1.0),
        });
    }
    Some(NarrationSplitResult {
        ranges,
        mode: "aligned",
    })
}

fn run_subtitle_narration(
    job_id: &str,
    request: SubtitleNarrationRequest,
    profile: TtsRequestProfile,
    snapshot: Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
) {
    let total = request.items.len();
    eprintln!("[Narration][job={}] running total_items={}", job_id, total);
    let _ = update_snapshot(&snapshot, |state| {
        state.state = "running".to_string();
        state.message = "Generating narration audio".to_string();
        state.progress = 0.0;
        state.total_items = total;
    });

    let mut clean_items = Vec::new();
    for (index, item) in request.items.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            eprintln!(
                "[Narration][job={}] cancelled at item {}/{}",
                job_id,
                index + 1,
                total
            );
            let _ = update_snapshot(&snapshot, |state| {
                state.state = "cancelled".to_string();
                state.message = "Subtitle narration cancelled".to_string();
                state.active_subtitle_id = None;
            });
            return;
        }

        let clean_text = item.text.trim();
        if clean_text.is_empty() {
            eprintln!(
                "[Narration][job={}] skip empty item {}/{} subtitle_id={}",
                job_id,
                index + 1,
                total,
                item.id
            );
            let _ = update_snapshot(&snapshot, |state| {
                state.completed_items += 1;
                state.progress = state.completed_items as f64 / total.max(1) as f64;
            });
            continue;
        }

        let Some(narration_text) = normalize_narration_input_text(clean_text, &profile.method)
        else {
            eprintln!(
                "[Narration][job={}] skip invalid narration text item {}/{} subtitle_id={} text_json={}",
                job_id,
                index + 1,
                total,
                item.id,
                serde_json::to_string(clean_text)
                    .unwrap_or_else(|_| "\"<unserializable>\"".to_string())
            );
            let _ = update_snapshot(&snapshot, |state| {
                state.completed_items += 1;
                state.progress = state.completed_items as f64 / total.max(1) as f64;
            });
            continue;
        };

        let tts_text = prepare_narration_tts_text(&narration_text, &profile.method);
        clean_items.push(CleanNarrationItem {
            id: item.id.clone(),
            text: clean_text.to_string(),
            text_units: estimate_narration_speech_units(&narration_text),
            aligner_text: normalize_alignment_text(&narration_text),
            tts_text,
            start_time: item.start_time,
            end_time: item.end_time,
        });
    }

    let groups = build_narration_groups(clean_items, &request.grouping);
    eprintln!(
        "[Narration][job={}] grouped total_items={} groups={} text_budget={} vad_radius={:.2}",
        job_id,
        total,
        groups.len(),
        request.grouping.text_budget_units,
        request.grouping.vad_search_radius_sec
    );

    if profile.method == TtsMethod::GeminiLive && profile.gemini_parallel_requests > 1 {
        run_gemini_subtitle_narration_parallel(
            job_id,
            total,
            &groups,
            &profile,
            &request.grouping,
            snapshot.clone(),
            cancelled.clone(),
        );
        return;
    }

    for (group_index, group) in groups.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            eprintln!(
                "[Narration][job={}] cancelled at group {}/{}",
                job_id,
                group_index + 1,
                groups.len()
            );
            let _ = update_snapshot(&snapshot, |state| {
                state.state = "cancelled".to_string();
                state.message = "Subtitle narration cancelled".to_string();
                state.active_subtitle_id = None;
            });
            return;
        }

        let Some(first_item) = group.items.first() else {
            continue;
        };
        let _ = update_snapshot(&snapshot, |state| {
            state.active_subtitle_id = Some(first_item.id.clone());
            state.message = format!(
                "Generating narration {}/{}",
                (state.completed_items + 1).min(total),
                total
            );
        });
        eprintln!(
            "[Narration][job={}] group {}/{} group_id={} items={} first_subtitle_id={} method={:?} lang_override='{}' text_chars={} text_json={}",
            job_id,
            group_index + 1,
            groups.len(),
            group.id,
            group.items.len(),
            first_item.id,
            profile.method,
            profile.language_code_override.as_deref().unwrap_or(""),
            group.text.chars().count(),
            serde_json::to_string(&group.text)
                .unwrap_or_else(|_| "\"<unserializable>\"".to_string())
        );

        let result = synthesize_narration_item_with_retries(NarrationSynthesisAttempt {
            job_id,
            index: group_index,
            total: groups.len(),
            item_id: &first_item.id,
            clean_text: &group.text,
            profile: &profile,
            snapshot: &snapshot,
            cancelled: &cancelled,
        });

        match &result {
            Ok((path, audio, attempts)) => eprintln!(
                "[Narration][job={}] group {}/{} group_id={} items={} OK duration_sec={:.3} attempts={} path={}",
                job_id,
                group_index + 1,
                groups.len(),
                group.id,
                group.items.len(),
                audio.duration_ms as f64 / 1000.0,
                attempts,
                path
            ),
            Err(message) => eprintln!(
                "[Narration][job={}] group {}/{} group_id={} first_subtitle_id={} FAILED attempts={} error={}",
                job_id,
                group_index + 1,
                groups.len(),
                group.id,
                first_item.id,
                NARRATION_TTS_MAX_ATTEMPTS,
                message
            ),
        }

        let _ = update_snapshot(&snapshot, |state| {
            state.completed_items += group.items.len();
            state.progress = state.completed_items as f64 / total.max(1) as f64;
            match result {
                Ok((path, audio, _attempts)) => {
                    let duration = audio.duration_ms as f64 / 1000.0;
                    let split = align_group_audio_ranges(
                        group,
                        &path,
                        &audio,
                        profile.language_code_override.as_deref(),
                        request.grouping.vad_search_radius_sec,
                    );
                    let alignment_mode = split.mode.to_string();
                    let take_id = format!("{job_id}-{}", group.id);
                    for (item, range) in group.items.iter().zip(split.ranges.into_iter()) {
                        let result = SubtitleNarrationResult {
                            subtitle_id: item.id.clone(),
                            text: item.text.clone(),
                            path: path.clone(),
                            duration,
                            source_in_point: range.start_sec,
                            source_out_point: range.end_sec,
                            group_id: group.id.clone(),
                            narration_group_take_id: take_id.clone(),
                            narration_group_prompt_text: group.text.clone(),
                            narration_group_source_start_time: first_item.start_time,
                            alignment_mode: alignment_mode.clone(),
                            alignment_confidence: range.confidence,
                            start_time: item.start_time,
                            end_time: item.end_time,
                        };
                        state.results.push(result.clone());
                        state.results_revision += 1;
                        let revision = state.results_revision;
                        state
                            .result_events
                            .push(SubtitleNarrationResultEvent { revision, result });
                    }
                }
                Err(message) => {
                    for item in &group.items {
                        state.errors.push(SubtitleNarrationError {
                            subtitle_id: item.id.clone(),
                            message: message.clone(),
                        });
                    }
                }
            }
        });
    }

    let _ = update_snapshot(&snapshot, |state| {
        state.active_subtitle_id = None;
        state.progress = 1.0;
        if state.results.is_empty() && !state.errors.is_empty() {
            state.state = "error".to_string();
            // Include the first underlying error in the human-readable message
            // so the side panel shows it instead of a generic "failed" string.
            let first_error = state
                .errors
                .first()
                .map(|error| error.message.clone())
                .unwrap_or_else(|| "Subtitle narration failed".to_string());
            state.message = format!("Subtitle narration failed: {}", first_error);
            state.error = Some(first_error);
        } else {
            state.state = "completed".to_string();
            state.message = if state.errors.is_empty() {
                "Subtitle narration complete".to_string()
            } else {
                format!(
                    "Subtitle narration complete with {} failed item(s)",
                    state.errors.len()
                )
            };
        }
    });
    if let Ok(state) = snapshot.lock() {
        eprintln!(
            "[Narration][job={}] done state={} results={} errors={} message=\"{}\"",
            job_id,
            state.state,
            state.results.len(),
            state.errors.len(),
            state.message
        );
        for (i, error) in state.errors.iter().enumerate() {
            eprintln!(
                "[Narration][job={}] error[{}] subtitle_id={} message={}",
                job_id, i, error.subtitle_id, error.message
            );
        }
    }
}

struct ParallelNarrationGroupResult {
    group_index: usize,
    group_total: usize,
    group: NarrationRequestGroup,
    result: Result<(String, TtsCollectedAudio, usize), String>,
}

fn run_gemini_subtitle_narration_parallel(
    job_id: &str,
    total_items: usize,
    groups: &[NarrationRequestGroup],
    profile: &TtsRequestProfile,
    grouping: &SubtitleNarrationGroupingRequest,
    snapshot: Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
) {
    let parallel = profile.gemini_parallel_requests.clamp(1, 4);
    eprintln!(
        "[Narration][job={}] Gemini parallel generation groups={} parallel={}",
        job_id,
        groups.len(),
        parallel
    );
    let (tx, rx) = mpsc::channel::<ParallelNarrationGroupResult>();
    let mut next_index = 0usize;
    let mut active = 0usize;

    while (next_index < groups.len() || active > 0) && !cancelled.load(Ordering::SeqCst) {
        while active < parallel && next_index < groups.len() && !cancelled.load(Ordering::SeqCst) {
            let group_index = next_index;
            next_index += 1;
            let group = groups[group_index].clone();
            let tx = tx.clone();
            let profile = profile.clone();
            let cancelled = cancelled.clone();
            let thread_job_id = job_id.to_string();
            let snapshot_for_retry = snapshot.clone();
            let group_total = groups.len();
            let first_item_id = group
                .items
                .first()
                .map(|item| item.id.clone())
                .unwrap_or_default();
            let group_text = group.text.clone();
            let group_for_result = group.clone();
            active += 1;
            let _ = update_snapshot(&snapshot, |state| {
                state.active_subtitle_id = Some(first_item_id.clone());
                state.message = format!(
                    "Generating narration {}/{}",
                    (state.completed_items + 1).min(total_items),
                    total_items
                );
            });
            std::thread::spawn(move || {
                let result = synthesize_gemini_narration_group_with_retries(
                    &thread_job_id,
                    group_index,
                    group_total,
                    &first_item_id,
                    &group_text,
                    &profile,
                    &snapshot_for_retry,
                    &cancelled,
                );
                let _ = tx.send(ParallelNarrationGroupResult {
                    group_index,
                    group_total,
                    group: group_for_result,
                    result,
                });
            });
        }

        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(group_result) => {
                active = active.saturating_sub(1);
                apply_narration_group_result(
                    job_id,
                    total_items,
                    grouping,
                    &snapshot,
                    group_result,
                    profile.language_code_override.as_deref(),
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = update_snapshot(&snapshot, |state| {
        state.active_subtitle_id = None;
        if cancelled.load(Ordering::SeqCst) {
            state.state = "cancelled".to_string();
            state.message = "Subtitle narration cancelled".to_string();
            return;
        }
        state.progress = 1.0;
        if state.results.is_empty() && !state.errors.is_empty() {
            state.state = "error".to_string();
            let first_error = state
                .errors
                .first()
                .map(|error| error.message.clone())
                .unwrap_or_else(|| "Subtitle narration failed".to_string());
            state.message = format!("Subtitle narration failed: {}", first_error);
            state.error = Some(first_error);
        } else {
            state.state = "completed".to_string();
            state.message = if state.errors.is_empty() {
                "Subtitle narration complete".to_string()
            } else {
                format!(
                    "Subtitle narration complete with {} failed item(s)",
                    state.errors.len()
                )
            };
        }
    });
}

fn apply_narration_group_result(
    job_id: &str,
    total_items: usize,
    grouping: &SubtitleNarrationGroupingRequest,
    snapshot: &Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    group_result: ParallelNarrationGroupResult,
    language_code_override: Option<&str>,
) {
    let ParallelNarrationGroupResult {
        group_index,
        group_total,
        group,
        result,
    } = group_result;
    let Some(first_item) = group.items.first() else {
        return;
    };
    match &result {
        Ok((path, audio, attempts)) => eprintln!(
            "[Narration][job={}] group {}/{} group_id={} items={} OK duration_sec={:.3} attempts={} path={}",
            job_id,
            group_index + 1,
            group_total,
            group.id,
            group.items.len(),
            audio.duration_ms as f64 / 1000.0,
            attempts,
            path
        ),
        Err(message) => eprintln!(
            "[Narration][job={}] group {}/{} group_id={} first_subtitle_id={} FAILED attempts={} error={}",
            job_id,
            group_index + 1,
            group_total,
            group.id,
            first_item.id,
            NARRATION_TTS_MAX_ATTEMPTS,
            message
        ),
    }

    let _ = update_snapshot(snapshot, |state| {
        state.completed_items += group.items.len();
        state.progress = state.completed_items as f64 / total_items.max(1) as f64;
        match result {
            Ok((path, audio, _attempts)) => {
                let duration = audio.duration_ms as f64 / 1000.0;
                let split = align_group_audio_ranges(
                    &group,
                    &path,
                    &audio,
                    language_code_override,
                    grouping.vad_search_radius_sec,
                );
                let alignment_mode = split.mode.to_string();
                let take_id = format!("{job_id}-{}", group.id);
                for (item, range) in group.items.iter().zip(split.ranges.into_iter()) {
                    let result = SubtitleNarrationResult {
                        subtitle_id: item.id.clone(),
                        text: item.text.clone(),
                        path: path.clone(),
                        duration,
                        source_in_point: range.start_sec,
                        source_out_point: range.end_sec,
                        group_id: group.id.clone(),
                        narration_group_take_id: take_id.clone(),
                        narration_group_prompt_text: group.text.clone(),
                        narration_group_source_start_time: first_item.start_time,
                        alignment_mode: alignment_mode.clone(),
                        alignment_confidence: range.confidence,
                        start_time: item.start_time,
                        end_time: item.end_time,
                    };
                    state.results.push(result.clone());
                    state.results_revision += 1;
                    let revision = state.results_revision;
                    state
                        .result_events
                        .push(SubtitleNarrationResultEvent { revision, result });
                }
            }
            Err(message) => {
                for item in &group.items {
                    state.errors.push(SubtitleNarrationError {
                        subtitle_id: item.id.clone(),
                        message: message.clone(),
                    });
                }
            }
        }
    });
}

fn prepare_narration_tts_text(text: &str, method: &TtsMethod) -> String {
    let trimmed = text.trim();
    if *method != TtsMethod::MagpieMultilingual || trimmed.is_empty() {
        return trimmed.to_string();
    }
    if trimmed
        .chars()
        .rev()
        .find(|ch| !ch.is_whitespace() && !matches!(ch, '"' | '\'' | ')' | ']' | '}'))
        .is_some_and(is_sentence_terminal_punctuation)
    {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

fn is_sentence_terminal_punctuation(ch: char) -> bool {
    matches!(ch, '.' | '!' | '?' | '…' | '。' | '！' | '？')
}

fn normalize_narration_input_text(text: &str, method: &TtsMethod) -> Option<String> {
    let edge_trimmed = trim_narration_noise(text);
    let repaired = repair_cp949_mojibake(&edge_trimmed);
    let repaired_from_mojibake = repaired.is_some();
    let repaired = repaired.unwrap_or(edge_trimmed);
    let normalized = trim_narration_noise(&repaired);
    if !has_speakable_text(&normalized) {
        return None;
    }
    if matches!(
        method,
        TtsMethod::VieneuTts
            | TtsMethod::Supertonic
            | TtsMethod::MagpieMultilingual
            | TtsMethod::StepAudioEditX
    ) && (unresolved_mojibake_score(&normalized) >= 3
        || (repaired_from_mojibake && unresolved_placeholder_score(&normalized) > 0))
    {
        return None;
    }
    Some(normalized)
}

fn trim_narration_noise(text: &str) -> String {
    let lines = text
        .lines()
        .map(trim_narration_edge_noise)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    trim_narration_edge_noise(&lines).to_string()
}

fn trim_narration_edge_noise(text: &str) -> &str {
    text.trim_matches(|ch: char| {
        ch.is_whitespace()
            || ch == '\u{fffd}'
            || ch == '\u{feff}'
            || matches!(
                ch,
                '?' | '¿'
                    | '¡'
                    | '!'
                    | '"'
                    | '\''
                    | '`'
                    | '*'
                    | '_'
                    | '~'
                    | '|'
                    | '·'
                    | '•'
                    | '♪'
                    | '♫'
                    | '♩'
                    | '♬'
                    | '♭'
                    | '♮'
                    | '♯'
            )
    })
}

fn has_speakable_text(text: &str) -> bool {
    text.chars().any(|ch| ch.is_alphanumeric())
}

fn repair_cp949_mojibake(text: &str) -> Option<String> {
    if unresolved_mojibake_score(text) < 2 {
        return None;
    }
    let (bytes, _, had_encode_errors) = encoding_rs::EUC_KR.encode(text);
    if had_encode_errors {
        return None;
    }
    let repaired = std::str::from_utf8(&bytes).ok()?.trim();
    if repaired.is_empty() || !has_speakable_text(repaired) {
        return None;
    }
    if unresolved_mojibake_score(repaired) < unresolved_mojibake_score(text) {
        Some(repaired.to_string())
    } else {
        None
    }
}

fn unresolved_mojibake_score(text: &str) -> usize {
    text.chars()
        .filter(|ch| {
            ('\u{3400}'..='\u{9fff}').contains(ch)
                || ('\u{ac00}'..='\u{d7af}').contains(ch)
                || *ch == '\u{fffd}'
        })
        .count()
}

fn unresolved_placeholder_score(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    chars
        .iter()
        .enumerate()
        .filter(|(index, ch)| {
            if **ch != '?' {
                return false;
            }
            let prev_is_word = chars[..*index]
                .iter()
                .rev()
                .find(|candidate| !candidate.is_whitespace())
                .is_some_and(|candidate| candidate.is_alphanumeric());
            let next_is_word = chars[*index + 1..]
                .iter()
                .find(|candidate| !candidate.is_whitespace())
                .is_some_and(|candidate| candidate.is_alphanumeric());
            prev_is_word && next_is_word
        })
        .count()
}

struct NarrationSynthesisAttempt<'a> {
    job_id: &'a str,
    index: usize,
    total: usize,
    item_id: &'a str,
    clean_text: &'a str,
    profile: &'a TtsRequestProfile,
    snapshot: &'a Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    cancelled: &'a Arc<AtomicBool>,
}

fn synthesize_narration_item_with_retries(
    attempt_ctx: NarrationSynthesisAttempt<'_>,
) -> Result<(String, TtsCollectedAudio, usize), String> {
    let NarrationSynthesisAttempt {
        job_id,
        index,
        total,
        item_id,
        clean_text,
        profile,
        snapshot,
        cancelled,
    } = attempt_ctx;
    let mut last_error = String::new();
    for attempt in 1..=NARRATION_TTS_MAX_ATTEMPTS {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Subtitle narration cancelled".to_string());
        }

        if attempt > 1 {
            let _ = update_snapshot(snapshot, |state| {
                state.message = format!(
                    "Retrying narration {}/{} ({}/{})",
                    index + 1,
                    total,
                    attempt,
                    NARRATION_TTS_MAX_ATTEMPTS
                );
            });
        }

        let synth_started_at = std::time::Instant::now();
        let result = TTS_MANAGER
            .synthesize_to_wav_with_profile_cancel(clean_text, profile.clone(), cancelled.clone())
            .map_err(|error| error.to_string())
            .and_then(|audio| {
                media_server::write_managed_narration_wav(job_id, index, &audio.wav_data)
                    .map(|path| (path, audio))
            });
        let synth_elapsed_ms = synth_started_at.elapsed().as_millis();

        match result {
            Ok((path, duration)) => {
                return Ok((path, duration, attempt));
            }
            Err(message) => {
                last_error = message;
                eprintln!(
                    "[Narration][job={}] item {}/{} subtitle_id={} attempt {}/{} failed elapsed_ms={} error={}",
                    job_id,
                    index + 1,
                    total,
                    item_id,
                    attempt,
                    NARRATION_TTS_MAX_ATTEMPTS,
                    synth_elapsed_ms,
                    last_error
                );
                if attempt < NARRATION_TTS_MAX_ATTEMPTS {
                    let delay_ms = NARRATION_TTS_RETRY_BASE_DELAY_MS * attempt as u64;
                    let mut slept_ms = 0;
                    while slept_ms < delay_ms {
                        if cancelled.load(Ordering::SeqCst) {
                            return Err("Subtitle narration cancelled".to_string());
                        }
                        let step_ms = (delay_ms - slept_ms).min(100);
                        std::thread::sleep(std::time::Duration::from_millis(step_ms));
                        slept_ms += step_ms;
                    }
                }
            }
        }
    }

    Err(last_error)
}

fn synthesize_gemini_narration_group_with_retries(
    job_id: &str,
    index: usize,
    total: usize,
    item_id: &str,
    clean_text: &str,
    profile: &TtsRequestProfile,
    snapshot: &Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<(String, TtsCollectedAudio, usize), String> {
    let mut last_error = String::new();
    for attempt in 1..=NARRATION_TTS_MAX_ATTEMPTS {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Subtitle narration cancelled".to_string());
        }
        if attempt > 1 {
            let _ = update_snapshot(snapshot, |state| {
                state.message = format!(
                    "Retrying narration {}/{} ({}/{})",
                    index + 1,
                    total,
                    attempt,
                    NARRATION_TTS_MAX_ATTEMPTS
                );
            });
        }

        let synth_started_at = std::time::Instant::now();
        let result = crate::api::tts::worker::synthesize_gemini_live_to_wav_cancel(
            clean_text,
            profile.clone(),
            cancelled.clone(),
        )
        .map_err(|error| error.to_string())
        .and_then(|audio| {
            media_server::write_managed_narration_wav(job_id, index, &audio.wav_data)
                .map(|path| (path, audio))
        });
        let synth_elapsed_ms = synth_started_at.elapsed().as_millis();

        match result {
            Ok((path, audio)) => return Ok((path, audio, attempt)),
            Err(message) => {
                last_error = message;
                eprintln!(
                    "[Narration][job={}] Gemini parallel group {}/{} subtitle_id={} attempt {}/{} failed elapsed_ms={} error={}",
                    job_id,
                    index + 1,
                    total,
                    item_id,
                    attempt,
                    NARRATION_TTS_MAX_ATTEMPTS,
                    synth_elapsed_ms,
                    last_error
                );
                if attempt < NARRATION_TTS_MAX_ATTEMPTS {
                    let delay_ms = NARRATION_TTS_RETRY_BASE_DELAY_MS * attempt as u64;
                    let mut slept_ms = 0;
                    while slept_ms < delay_ms {
                        if cancelled.load(Ordering::SeqCst) {
                            return Err("Subtitle narration cancelled".to_string());
                        }
                        let step_ms = (delay_ms - slept_ms).min(100);
                        std::thread::sleep(std::time::Duration::from_millis(step_ms));
                        slept_ms += step_ms;
                    }
                }
            }
        }
    }
    Err(last_error)
}

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
mod tests {
    use super::{
        CleanNarrationItem, NarrationRequestGroup, SubtitleNarrationGroupingRequest,
        TtsProfileWire, build_narration_groups, handle_get_narration_tts_metadata,
        normalize_group_sentence, normalize_narration_input_text, prepare_narration_tts_text,
        split_group_audio_ranges,
    };
    use crate::api::tts::types::TtsCollectedAudio;
    use crate::config::TtsMethod;

    #[test]
    fn step_audio_profile_wire_preserves_reference_id_and_prompt() {
        let wire: TtsProfileWire = serde_json::from_value(serde_json::json!({
            "method": "StepAudioEditX",
            "stepAudioVoice": "",
            "stepAudioReferenceVoiceId": "ref-demo",
            "stepAudioPromptText": "Use a calm narration delivery."
        }))
        .expect("deserialize Step Audio narration profile");

        let profile = wire.into_request_profile(Some("cmn".to_string()));

        assert_eq!(profile.method, TtsMethod::StepAudioEditX);
        assert_eq!(profile.language_code_override.as_deref(), Some("cmn"));
        assert_eq!(
            profile.step_audio_settings.style_prompt,
            "Use a calm narration delivery."
        );
        assert_eq!(profile.step_audio_settings.reference_voice_id, "ref-demo");
    }

    #[test]
    fn narration_tts_metadata_exposes_step_audio_options() {
        let metadata = handle_get_narration_tts_metadata(&serde_json::Value::Null)
            .expect("get narration TTS metadata");

        assert!(
            metadata["providers"]
                .as_array()
                .expect("providers array")
                .iter()
                .any(|provider| provider["method"] == "StepAudioEditX")
        );
        assert!(metadata["stepAudioReferenceVoices"].is_array());
        assert!(metadata["defaults"]["stepAudioReferenceVoiceId"].is_string());
    }

    #[test]
    fn magpie_narration_adds_terminal_punctuation_to_fragments() {
        assert_eq!(
            prepare_narration_tts_text("Đêm giông bão", &TtsMethod::MagpieMultilingual),
            "Đêm giông bão."
        );
        assert_eq!(
            prepare_narration_tts_text("Đêm giông bão!", &TtsMethod::MagpieMultilingual),
            "Đêm giông bão!"
        );
        assert_eq!(
            prepare_narration_tts_text("Đêm giông bão", &TtsMethod::GeminiLive),
            "Đêm giông bão"
        );
    }

    #[test]
    fn narration_normalization_repairs_cp949_mojibake() {
        assert_eq!(
            normalize_narration_input_text("휂챗m gi척ng b찾o", &TtsMethod::VieneuTts).as_deref(),
            Some("Đêm giông bão")
        );
        assert_eq!(
            normalize_narration_input_text(
                "??R梳캮G CH횣NG T횚I C횙 CH梳짽??",
                &TtsMethod::VieneuTts
            )
            .as_deref(),
            Some("RẰNG CHÚNG TÔI CÓ CHẤT")
        );
    }

    #[test]
    fn narration_normalization_skips_unrecoverable_mojibake_placeholders() {
        assert_eq!(
            normalize_narration_input_text(
                "??V? NH梳줪 THEO NH沼둗 휂I沼괣- ??",
                &TtsMethod::VieneuTts
            ),
            None
        );
        assert_eq!(
            normalize_narration_input_text("[V?O 휂칩A]", &TtsMethod::VieneuTts),
            None
        );
    }

    #[test]
    fn narration_normalization_skips_unspeakable_fragments() {
        assert_eq!(
            normalize_narration_input_text("????", &TtsMethod::VieneuTts),
            None
        );
    }

    #[test]
    fn narration_normalization_strips_music_wrappers_per_line() {
        assert_eq!(
            normalize_narration_input_text(
                "♪♪TÔI VÀ CÔ GÁI CỦA TÔI♪♪\n♪♪MỐI QUAN HỆ NÀY♪♪",
                &TtsMethod::VieneuTts
            )
            .as_deref(),
            Some("TÔI VÀ CÔ GÁI CỦA TÔI\nMỐI QUAN HỆ NÀY")
        );
    }

    fn clean_item(id: &str, text: &str, start_time: f64, end_time: f64) -> CleanNarrationItem {
        CleanNarrationItem {
            id: id.to_string(),
            text: text.to_string(),
            tts_text: text.to_string(),
            aligner_text: super::normalize_alignment_text(text),
            start_time,
            end_time,
            text_units: super::estimate_narration_speech_units(text),
        }
    }

    #[test]
    fn narration_grouping_respects_text_budget_and_timing_gaps() {
        let grouping = SubtitleNarrationGroupingRequest {
            text_budget_units: 4,
            vad_search_radius_sec: 0.35,
        };
        let groups = build_narration_groups(
            vec![
                clean_item("a", "one two", 0.0, 0.5),
                clean_item("b", "three four", 0.6, 1.0),
                clean_item("c", "five", 3.0, 3.4),
            ],
            &grouping,
        );

        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0]
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(groups[1].items[0].id, "c");
    }

    #[test]
    fn narration_group_sentence_join_adds_periods_for_vad_pauses() {
        assert_eq!(
            normalize_group_sentence("♪ TÔI VÀ CÔ GÁI ♪"),
            "TÔI VÀ CÔ GÁI."
        );
        assert_eq!(normalize_group_sentence("Bạn ổn không?"), "Bạn ổn không?");
        assert_eq!(normalize_group_sentence("Chạy nào,"), "Chạy nào.");
    }

    #[test]
    fn narration_group_split_ranges_are_monotonic() {
        let group = NarrationRequestGroup {
            id: "group-0".to_string(),
            items: vec![
                clean_item("a", "xin chào", 0.0, 0.6),
                clean_item("b", "tạm biệt mọi người", 0.6, 1.4),
            ],
            text: "xin chào. tạm biệt mọi người.".to_string(),
            spans: Vec::new(),
        };
        let audio = TtsCollectedAudio {
            pcm_samples: vec![0; 24_000],
            wav_data: Vec::new(),
            sample_rate: 24_000,
            duration_ms: 1000,
        };
        let split = split_group_audio_ranges(&group, &audio, 0.35);
        let ranges = split.ranges;

        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].start_sec, 0.0);
        assert!(ranges[0].end_sec > ranges[0].start_sec);
        assert!(ranges[1].start_sec >= ranges[0].end_sec);
        assert_eq!(ranges[1].end_sec, 1.0);
    }
}
