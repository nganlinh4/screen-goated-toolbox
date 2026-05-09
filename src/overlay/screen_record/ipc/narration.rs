use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::api::tts::TTS_MANAGER;
use crate::api::tts::types::TtsRequestProfile;
use crate::config::{
    EdgeTtsSettings, EdgeTtsVoiceConfig, TtsLanguageCondition, TtsMethod, TtsPlaygroundSettings,
};
use crate::gui::settings_ui::tts_playground_data::{
    GEMINI_VOICES, SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES,
};
use crate::model_config::tts_gemini_model_options;

use super::media_server;

const NARRATION_TTS_MAX_ATTEMPTS: usize = 4;
const NARRATION_TTS_RETRY_BASE_DELAY_MS: u64 = 350;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleNarrationItemRequest {
    id: String,
    text: String,
    start_time: f64,
    end_time: f64,
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

impl From<EdgeVoiceConfigWire> for EdgeTtsVoiceConfig {
    fn from(wire: EdgeVoiceConfigWire) -> Self {
        EdgeTtsVoiceConfig::new(&wire.language_code, &wire.language_name, &wire.voice_name)
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
}

impl From<TtsProfileWire> for TtsRequestProfile {
    fn from(wire: TtsProfileWire) -> Self {
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
        let edge_voice_configs: Vec<EdgeTtsVoiceConfig> = if wire.edge_voice_configs.is_empty() {
            defaults.edge_settings.voice_configs
        } else {
            wire.edge_voice_configs
                .into_iter()
                .map(EdgeTtsVoiceConfig::from)
                .collect()
        };

        Self {
            method: wire.method,
            gemini_model: trimmed_or(wire.gemini_model, defaults.gemini_model),
            gemini_voice: trimmed_or(wire.gemini_voice, defaults.gemini_voice),
            gemini_speed: trimmed_or(wire.gemini_speed, defaults.gemini_speed),
            gemini_instruction: wire.gemini_instruction,
            gemini_language_conditions: if wire.gemini_language_conditions.is_empty() {
                defaults.gemini_language_conditions
            } else {
                wire.gemini_language_conditions
                    .into_iter()
                    .map(TtsLanguageCondition::from)
                    .collect()
            },
            google_speed: trimmed_or(wire.google_speed, defaults.google_speed),
            edge_voice: trimmed_or(wire.edge_voice, defaults.edge_voice),
            edge_settings: EdgeTtsSettings {
                pitch: wire.edge_pitch,
                rate: wire.edge_rate,
                volume: 0,
                voice_configs: edge_voice_configs,
            },
            language_code_override: None,
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
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleNarrationResult {
    subtitle_id: String,
    text: String,
    path: String,
    duration: f64,
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

    let mut profile: TtsRequestProfile = request.profile.clone().into();
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
    profile.language_code_override = narration_language_code.clone();
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

fn detect_narration_job_language(
    items: &[SubtitleNarrationItemRequest],
) -> (Option<String>, String) {
    let sample = build_narration_language_sample(items);
    if sample.trim().is_empty() {
        return (None, String::new());
    }
    (crate::lang_detect::detect_language(&sample), sample)
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

    let default_method = match defaults.method {
        TtsMethod::GeminiLive => "GeminiLive",
        TtsMethod::GoogleTranslate => "GoogleTranslate",
        TtsMethod::EdgeTTS => "EdgeTTS",
    };

    Ok(serde_json::json!({
        "geminiVoices": gemini_voices,
        "geminiModels": gemini_models,
        "geminiInstructionLanguages": gemini_instruction_languages,
        "geminiSpeedOptions": ["Slow", "Normal", "Fast"],
        "googleSpeedOptions": ["Slow", "Normal"],
        "edgeVoiceState": edge_voice_state,
        "edgeVoiceError": edge_voice_error,
        "edgeVoiceLanguages": edge_voice_languages,
        "edgeVoicesByLanguage": edge_voices_by_language,
        "defaults": {
            "method": default_method,
            "geminiModel": defaults.gemini_model,
            "geminiVoice": defaults.gemini_voice,
            "geminiSpeed": defaults.gemini_speed,
            "geminiInstruction": defaults.gemini_instruction,
            "geminiLanguageConditions": default_language_conditions,
            "googleSpeed": defaults.google_speed,
            "edgeVoice": defaults.edge_voice,
            "edgePitch": defaults.edge_settings.pitch,
            "edgeRate": defaults.edge_settings.rate,
            "edgeVoiceConfigs": default_edge_voice_configs,
        },
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

        let _ = update_snapshot(&snapshot, |state| {
            state.active_subtitle_id = Some(item.id.clone());
            state.message = format!("Generating narration {}/{}", index + 1, total);
        });
        eprintln!(
            "[Narration][job={}] item {}/{} subtitle_id={} text_len={} text_preview=\"{}\"",
            job_id,
            index + 1,
            total,
            item.id,
            clean_text.chars().count(),
            clean_text.chars().take(60).collect::<String>()
        );

        let result = synthesize_narration_item_with_retries(
            job_id, index, total, item, clean_text, &profile, &snapshot, &cancelled,
        );

        match &result {
            Ok((path, duration, attempts)) => eprintln!(
                "[Narration][job={}] item {}/{} subtitle_id={} OK duration_sec={:.3} attempts={} path={}",
                job_id,
                index + 1,
                total,
                item.id,
                duration,
                attempts,
                path
            ),
            Err(message) => eprintln!(
                "[Narration][job={}] item {}/{} subtitle_id={} FAILED attempts={} error={}",
                job_id,
                index + 1,
                total,
                item.id,
                NARRATION_TTS_MAX_ATTEMPTS,
                message
            ),
        }

        let _ = update_snapshot(&snapshot, |state| {
            state.completed_items += 1;
            state.progress = state.completed_items as f64 / total.max(1) as f64;
            match result {
                Ok((path, duration, _attempts)) => {
                    let result = SubtitleNarrationResult {
                        subtitle_id: item.id.clone(),
                        text: clean_text.to_string(),
                        path,
                        duration,
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
                Err(message) => state.errors.push(SubtitleNarrationError {
                    subtitle_id: item.id.clone(),
                    message,
                }),
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

fn synthesize_narration_item_with_retries(
    job_id: &str,
    index: usize,
    total: usize,
    item: &SubtitleNarrationItemRequest,
    clean_text: &str,
    profile: &TtsRequestProfile,
    snapshot: &Arc<Mutex<SubtitleNarrationJobSnapshot>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<(String, f64, usize), String> {
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
            .synthesize_to_wav_with_profile(clean_text, profile.clone())
            .map_err(|error| error.to_string())
            .and_then(|audio| {
                media_server::write_managed_narration_wav(job_id, index, &audio.wav_data)
                    .map(|path| (path, audio.duration_ms as f64 / 1000.0))
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
                    item.id,
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
