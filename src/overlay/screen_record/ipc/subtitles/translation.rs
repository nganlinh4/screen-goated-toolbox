use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::config::Config;
use crate::model_config::{ModelConfig, ModelType, get_all_models_with_ollama, get_model_by_id, model_is_non_llm};
use crate::retry_model_chain::{
    RetryChainKind, preflight_skip_reason, provider_is_available, resolve_next_retry_model,
};
use crate::APP;

use super::translation_providers::{TranslationConversationTurn, translate_subtitle_chunk};
use super::types::{
    SubtitleTranslationCapabilities, SubtitleTranslationJobSnapshot, SubtitleTranslationItemRequest,
    SubtitleTranslationModelCapability, SubtitleTranslationRequest, SubtitleTranslationResultItem,
};

#[derive(Clone)]
struct SubtitleTranslationJobHandle {
    snapshot: Arc<Mutex<SubtitleTranslationJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
}

static SUBTITLE_TRANSLATION_JOBS: OnceLock<Mutex<HashMap<String, SubtitleTranslationJobHandle>>> =
    OnceLock::new();

fn subtitle_translation_jobs() -> &'static Mutex<HashMap<String, SubtitleTranslationJobHandle>> {
    SUBTITLE_TRANSLATION_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn handle_start_subtitle_translation(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: SubtitleTranslationRequest = serde_json::from_value(args.clone())
        .map_err(|error| format!("Invalid subtitle translation request: {error}"))?;
    let job_id = uuid();
    let snapshot = Arc::new(Mutex::new(SubtitleTranslationJobSnapshot {
        state: "queued".to_string(),
        message: "Queued subtitle translation".to_string(),
        message_key: Some("subtitleTranslationStatusQueued".to_string()),
        target_language: Some(request.target_language.clone()),
        ..SubtitleTranslationJobSnapshot::default()
    }));
    let cancelled = Arc::new(AtomicBool::new(false));
    subtitle_translation_jobs()
        .lock()
        .map_err(|_| "Subtitle translation jobs lock poisoned".to_string())?
        .insert(
            job_id.clone(),
            SubtitleTranslationJobHandle {
                snapshot: snapshot.clone(),
                cancelled: cancelled.clone(),
            },
        );

    std::thread::spawn(move || run_subtitle_translation(request, snapshot, cancelled));

    Ok(serde_json::json!({ "jobId": job_id }))
}

pub fn handle_get_subtitle_translation_status(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let job_id = args["jobId"].as_str().ok_or("Missing jobId")?;
    let jobs = subtitle_translation_jobs()
        .lock()
        .map_err(|_| "Subtitle translation jobs lock poisoned".to_string())?;
    let handle = jobs
        .get(job_id)
        .ok_or_else(|| format!("Unknown subtitle translation job: {job_id}"))?;
    let snapshot = handle
        .snapshot
        .lock()
        .map_err(|_| "Subtitle translation snapshot lock poisoned".to_string())?
        .clone();
    serde_json::to_value(snapshot)
        .map_err(|error| format!("Serialize subtitle translation status: {error}"))
}

pub fn handle_cancel_subtitle_translation(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let job_id = args["jobId"].as_str().ok_or("Missing jobId")?;
    let jobs = subtitle_translation_jobs()
        .lock()
        .map_err(|_| "Subtitle translation jobs lock poisoned".to_string())?;
    let handle = jobs
        .get(job_id)
        .ok_or_else(|| format!("Unknown subtitle translation job: {job_id}"))?;
    handle.cancelled.store(true, Ordering::SeqCst);
    if let Ok(mut snapshot) = handle.snapshot.lock() {
        snapshot.state = "cancelled".to_string();
        snapshot.message = "Subtitle translation cancelled".to_string();
        snapshot.message_key = Some("subtitleTranslationStatusCancelled".to_string());
    }
    Ok(serde_json::Value::Null)
}

pub fn handle_get_subtitle_translation_capabilities(
    _args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let config = current_config()?;
    let models = collect_translation_models(&config);
    let payload = SubtitleTranslationCapabilities {
        available: !models.is_empty(),
        reason: if models.is_empty() {
            Some("No compatible text-to-text translation model is currently available.".to_string())
        } else {
            None
        },
        models: models
            .into_iter()
            .map(|model| SubtitleTranslationModelCapability {
                model_id: model.id.clone(),
                model_label: localized_model_label(&model, &config.ui_language),
                provider: model.provider.clone(),
            })
            .collect(),
    };
    serde_json::to_value(payload)
        .map_err(|error| format!("Serialize subtitle translation capabilities: {error}"))
}

fn run_subtitle_translation(
    request: SubtitleTranslationRequest,
    snapshot: Arc<Mutex<SubtitleTranslationJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
) {
    let result = run_subtitle_translation_inner(&request, &snapshot, &cancelled);
    let mut locked = match snapshot.lock() {
        Ok(locked) => locked,
        Err(_) => return,
    };
    if cancelled.load(Ordering::SeqCst) {
        locked.state = "cancelled".to_string();
        locked.message = "Subtitle translation cancelled".to_string();
        locked.message_key = Some("subtitleTranslationStatusCancelled".to_string());
        return;
    }
    match result {
        Ok(results) => {
            locked.state = "completed".to_string();
            locked.message = "Subtitle translation complete".to_string();
            locked.message_key = Some("subtitleTranslationStatusComplete".to_string());
            locked.progress = 1.0;
            locked.results = results;
        }
        Err(error) => {
            locked.state = "error".to_string();
            locked.message = error.clone();
            locked.message_key = Some("subtitleTranslationStatusFailed".to_string());
            locked.error = Some(error);
        }
    }
}

fn run_subtitle_translation_inner(
    request: &SubtitleTranslationRequest,
    snapshot: &Arc<Mutex<SubtitleTranslationJobSnapshot>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<Vec<SubtitleTranslationResultItem>, String> {
    if request.items.is_empty() {
        return Err("No subtitle items were provided for translation.".to_string());
    }
    let config = current_config()?;
    let candidate_models = collect_translation_models(&config);
    if candidate_models.is_empty() {
        return Err("No compatible text-to-text translation model is currently available.".to_string());
    }

    update_translation_snapshot(snapshot, |locked| {
        locked.state = "running".to_string();
        locked.message = "Translating subtitles…".to_string();
        locked.message_key = Some("subtitleTranslationStatusRunning".to_string());
        locked.progress = 0.0;
        locked.target_language = Some(request.target_language.clone());
    })?;

    let mut last_error =
        "Subtitle translation failed because every model attempt returned invalid output.".to_string();

    for chunk_count in 1..=request.items.len() {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(Vec::new());
        }
        let chunks = split_translation_items(&request.items, chunk_count);
        for model in &candidate_models {
            if cancelled.load(Ordering::SeqCst) {
                return Ok(Vec::new());
            }

            let mut history: Vec<TranslationConversationTurn> = Vec::new();
            let mut translated_results: Vec<SubtitleTranslationResultItem> = Vec::new();
            let mut model_failed = false;

            for (chunk_index, chunk) in chunks.iter().enumerate() {
                update_translation_snapshot(snapshot, |locked| {
                    locked.current_model_id = Some(model.id.clone());
                    locked.current_model_label =
                        Some(localized_model_label(model, &config.ui_language));
                    locked.current_chunk_count = chunk_count;
                    locked.current_chunk_index = chunk_index + 1;
                    locked.total_chunks = chunks.len();
                    locked.progress = chunk_index as f64 / chunks.len().max(1) as f64;
                    locked.message = format!(
                        "Translating subtitles with {} ({}/{})",
                        localized_model_label(model, &config.ui_language),
                        chunk_index + 1,
                        chunks.len()
                    );
                    locked.message_key = Some("subtitleTranslationStatusChunk".to_string());
                    locked.message_params = HashMap::from([
                        ("model".to_string(), localized_model_label(model, &config.ui_language)),
                        ("current".to_string(), (chunk_index + 1).to_string()),
                        ("total".to_string(), chunks.len().to_string()),
                    ]);
                })?;

                match translate_subtitle_chunk(
                    &config,
                    model,
                    &request.target_language,
                    chunk,
                    &history,
                ) {
                    Ok(response) => {
                        history.push(TranslationConversationTurn {
                            user_payload: response.user_payload,
                            assistant_payload: response.assistant_payload,
                        });
                        translated_results.extend(response.items);
                    }
                    Err(error) => {
                        last_error = error;
                        model_failed = true;
                        break;
                    }
                }
            }

            if !model_failed && translated_results.len() == request.items.len() {
                return Ok(translated_results);
            }
        }
    }

    Err(last_error)
}

fn split_translation_items(
    items: &[SubtitleTranslationItemRequest],
    chunk_count: usize,
) -> Vec<Vec<SubtitleTranslationItemRequest>> {
    let safe_chunk_count = chunk_count.max(1).min(items.len().max(1));
    let mut chunks = Vec::with_capacity(safe_chunk_count);
    for chunk_index in 0..safe_chunk_count {
        let start = chunk_index * items.len() / safe_chunk_count;
        let end = (chunk_index + 1) * items.len() / safe_chunk_count;
        if start < end {
            chunks.push(items[start..end].to_vec());
        }
    }
    chunks
}

fn current_config() -> Result<Config, String> {
    APP.lock()
        .map(|app| app.config.clone())
        .map_err(|_| "App lock poisoned".to_string())
}

fn localized_model_label(model: &ModelConfig, ui_language: &str) -> String {
    match ui_language {
        "vi" => model.name_vi.clone(),
        "ko" => model.name_ko.clone(),
        _ => model.name_en.clone(),
    }
}

fn collect_translation_models(config: &Config) -> Vec<ModelConfig> {
    let blocked_providers = HashSet::new();
    let mut models = Vec::new();

    if let Some(initial_model) = resolve_initial_translation_model(config, &blocked_providers) {
        models.push(initial_model.clone());
        let mut failed_model_ids = vec![initial_model.id.clone()];
        let mut current_model = initial_model;
        while let Some(next_model) = resolve_next_retry_model(
            &current_model.id,
            &failed_model_ids,
            &blocked_providers,
            RetryChainKind::TextToText,
            config,
        ) {
            if failed_model_ids.iter().any(|failed| failed == &next_model.id) {
                break;
            }
            failed_model_ids.push(next_model.id.clone());
            current_model = next_model.clone();
            models.push(next_model);
        }
    }

    models
}

fn resolve_initial_translation_model(
    config: &Config,
    blocked_providers: &HashSet<String>,
) -> Option<ModelConfig> {
    for candidate_id in RetryChainKind::TextToText.configured_chain(config) {
        let Some(model) = get_model_by_id(candidate_id) else {
            continue;
        };
        if is_compatible_translation_model(&model, config, blocked_providers) {
            return Some(model);
        }
    }

    get_all_models_with_ollama()
        .into_iter()
        .find(|model| is_compatible_translation_model(model, config, blocked_providers))
}

fn is_compatible_translation_model(
    model: &ModelConfig,
    config: &Config,
    blocked_providers: &HashSet<String>,
) -> bool {
    model.enabled
        && model.model_type == ModelType::Text
        && !model_is_non_llm(&model.id)
        && !blocked_providers.contains(&model.provider)
        && provider_is_available(&model.provider, config)
        && preflight_skip_reason(&model.id, &model.provider, config, blocked_providers).is_none()
}

fn update_translation_snapshot(
    snapshot: &Arc<Mutex<SubtitleTranslationJobSnapshot>>,
    updater: impl FnOnce(&mut SubtitleTranslationJobSnapshot),
) -> Result<(), String> {
    let mut locked = snapshot
        .lock()
        .map_err(|_| "Subtitle translation snapshot lock poisoned".to_string())?;
    updater(&mut locked);
    Ok(())
}

fn uuid() -> String {
    format!(
        "subtitle-translation-{}-{}",
        chrono::Utc::now().timestamp_millis(),
        std::process::id()
    )
}
