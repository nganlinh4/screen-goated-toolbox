mod chunking;
mod diagnostics;
mod models;
mod retry;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::config::Config;
use crate::model_config::{
    ModelConfig, PRESET_TRANSLATE_ARENA_GTX_MODEL_ID, get_model_by_id_with_custom,
};

use self::chunking::{initial_translation_chunk_count, split_translation_items};
use self::diagnostics::{TranslationDiagnostics, TranslationStartLog};
use self::models::{
    collect_prioritized_translation_models, collect_translation_models, current_config,
    localized_model_label,
};
use self::retry::{AttemptModelArgs, AttemptOutcome, attempt_model};
use super::super::job_registry::{self, JobHandle};
use super::translation_providers::{
    TranslationConversationTurn, translate_subtitle_chunk, translate_subtitle_chunk_with_gtx,
};
use super::types::{
    SubtitleTranslationCapabilities, SubtitleTranslationItemRequest,
    SubtitleTranslationJobSnapshot, SubtitleTranslationModelCapability, SubtitleTranslationRequest,
    SubtitleTranslationResultItem,
};

const TRANSLATION_MODEL_ATTEMPTS: usize = 3;
const TRANSLATION_RETRY_BASE_DELAY_MS: u64 = 3_000;
const GTX_TRANSLATION_MODEL_ID: &str = "gtx";
const GTX_TRANSLATION_MODEL_LABEL: &str = "GTX";

static SUBTITLE_TRANSLATION_JOBS: OnceLock<
    Mutex<HashMap<String, JobHandle<SubtitleTranslationJobSnapshot>>>,
> = OnceLock::new();

fn subtitle_translation_jobs()
-> &'static Mutex<HashMap<String, JobHandle<SubtitleTranslationJobSnapshot>>> {
    job_registry::registry(&SUBTITLE_TRANSLATION_JOBS)
}

pub fn handle_start_subtitle_translation(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: SubtitleTranslationRequest = serde_json::from_value(args.clone())
        .map_err(|error| format!("Invalid subtitle translation request: {error}"))?;
    let job_id = job_registry::uuid("subtitle-translation");
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
            JobHandle {
                snapshot: snapshot.clone(),
                cancelled: cancelled.clone(),
            },
        );

    let thread_job_id = job_id.clone();
    std::thread::spawn(move || {
        run_subtitle_translation(&thread_job_id, request, snapshot, cancelled)
    });

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
    let gtx_model =
        get_model_by_id_with_custom(PRESET_TRANSLATE_ARENA_GTX_MODEL_ID, &config.custom_models);
    let mut model_payload = vec![gtx_model.map_or_else(
        || SubtitleTranslationModelCapability {
            model_id: GTX_TRANSLATION_MODEL_ID.to_string(),
            model_label: GTX_TRANSLATION_MODEL_LABEL.to_string(),
            model_name: "translate.googleapis.com/gtx".to_string(),
            provider: "google-gtx".to_string(),
            quality_tier: None,
            typical_latency_ms: None,
            performance_source: None,
        },
        |model| translation_model_capability(GTX_TRANSLATION_MODEL_ID, &model, &config.ui_language),
    )];
    model_payload.extend(models.into_iter().map(|model| {
        let model_id = model.id.clone();
        translation_model_capability(&model_id, &model, &config.ui_language)
    }));
    let payload = SubtitleTranslationCapabilities {
        available: true,
        reason: None,
        models: model_payload,
    };
    serde_json::to_value(payload)
        .map_err(|error| format!("Serialize subtitle translation capabilities: {error}"))
}

fn translation_model_capability(
    runtime_model_id: &str,
    model: &ModelConfig,
    ui_language: &str,
) -> SubtitleTranslationModelCapability {
    SubtitleTranslationModelCapability {
        model_id: runtime_model_id.to_string(),
        model_label: localized_model_label(model, ui_language),
        model_name: model.full_name.clone(),
        provider: model.provider.clone(),
        quality_tier: model.quality_tier,
        typical_latency_ms: model.typical_latency_ms,
        performance_source: model.performance_source.clone(),
    }
}

fn run_subtitle_translation(
    job_id: &str,
    request: SubtitleTranslationRequest,
    snapshot: Arc<Mutex<SubtitleTranslationJobSnapshot>>,
    cancelled: Arc<AtomicBool>,
) {
    let result = run_subtitle_translation_inner(job_id, &request, &snapshot, &cancelled);
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
    job_id: &str,
    request: &SubtitleTranslationRequest,
    snapshot: &Arc<Mutex<SubtitleTranslationJobSnapshot>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<Vec<SubtitleTranslationResultItem>, String> {
    if request.items.is_empty() {
        return Err("No subtitle items were provided for translation.".to_string());
    }
    let config = current_config()?;
    let selected_model_id = request
        .model_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(GTX_TRANSLATION_MODEL_ID);
    let gtx_selected = selected_model_id == GTX_TRANSLATION_MODEL_ID;
    let candidate_models =
        collect_prioritized_translation_models(&config, selected_model_id, request.smart_fallback)?;
    if candidate_models.is_empty() && !gtx_selected {
        return Err(
            "No compatible text-to-text translation model is currently available.".to_string(),
        );
    }

    update_translation_snapshot(snapshot, |locked| {
        locked.state = "running".to_string();
        locked.message = "Translating subtitles…".to_string();
        locked.message_key = Some("subtitleTranslationStatusRunning".to_string());
        locked.progress = 0.0;
        locked.target_language = Some(request.target_language.clone());
    })?;

    let initial_chunk_count = initial_translation_chunk_count(
        request.chunk_count,
        &request.chunk_mode,
        request.items.len(),
    );
    let chunks = split_translation_items(&request.items, initial_chunk_count);
    let mut diagnostics = TranslationDiagnostics::new(job_id);
    diagnostics.log_start(TranslationStartLog {
        item_count: request.items.len(),
        chunk_count: chunks.len(),
        chunk_mode: request.chunk_mode.as_deref(),
        target_language: &request.target_language,
        instructions: request.instructions.as_deref(),
        selected_model_id,
        gtx_prioritized: gtx_selected,
        smart_fallback: request.smart_fallback,
        candidate_models: &candidate_models,
        ui_language: &config.ui_language,
    });
    let mut history: Vec<TranslationConversationTurn> = Vec::new();
    let mut previous_source_group_id: Option<String> = None;
    let mut translated_results: Vec<SubtitleTranslationResultItem> = Vec::new();
    let mut total_groups = chunks.len();
    let mut completed_groups = 0usize;

    for chunk in chunks {
        let chunk_source_group_id = chunk.first().and_then(|item| item.source_group_id.clone());
        if previous_source_group_id != chunk_source_group_id {
            history.clear();
            previous_source_group_id = chunk_source_group_id.clone();
        }
        translate_group_with_retry(TranslateGroupRequest {
            config: &config,
            candidate_models: &candidate_models,
            gtx_selected,
            smart_fallback: request.smart_fallback,
            target_language: &request.target_language,
            instructions: request.instructions.as_deref(),
            group: chunk,
            history: &mut history,
            translated_results: &mut translated_results,
            snapshot,
            cancelled,
            total_items: request.items.len(),
            total_groups: &mut total_groups,
            completed_groups: &mut completed_groups,
            diagnostics: &mut diagnostics,
        })?;
    }

    if translated_results.len() == request.items.len() {
        diagnostics.log_finish(
            "completed",
            translated_results.len(),
            request.items.len(),
            completed_groups,
            total_groups,
        );
        Ok(translated_results)
    } else {
        diagnostics.log_finish(
            "mismatched",
            translated_results.len(),
            request.items.len(),
            completed_groups,
            total_groups,
        );
        Err(format!(
            "Subtitle translation produced {} item(s) for {} requested subtitle(s)",
            translated_results.len(),
            request.items.len()
        ))
    }
}

struct TranslateGroupRequest<'a> {
    config: &'a Config,
    candidate_models: &'a [ModelConfig],
    gtx_selected: bool,
    smart_fallback: bool,
    target_language: &'a str,
    instructions: Option<&'a str>,
    group: Vec<SubtitleTranslationItemRequest>,
    history: &'a mut Vec<TranslationConversationTurn>,
    translated_results: &'a mut Vec<SubtitleTranslationResultItem>,
    snapshot: &'a Arc<Mutex<SubtitleTranslationJobSnapshot>>,
    cancelled: &'a Arc<AtomicBool>,
    total_items: usize,
    total_groups: &'a mut usize,
    completed_groups: &'a mut usize,
    diagnostics: &'a mut TranslationDiagnostics,
}

fn translate_group_with_retry(request: TranslateGroupRequest<'_>) -> Result<(), String> {
    let TranslateGroupRequest {
        config,
        candidate_models,
        gtx_selected,
        smart_fallback,
        target_language,
        instructions,
        mut group,
        history,
        translated_results,
        snapshot,
        cancelled,
        total_items,
        total_groups,
        completed_groups,
        diagnostics,
    } = request;

    if group.is_empty() || cancelled.load(Ordering::SeqCst) {
        return Ok(());
    }

    let mut last_error =
        "Subtitle translation failed because every model attempt returned invalid output."
            .to_string();

    if gtx_selected {
        let mut gtx_fn = |group: &[SubtitleTranslationItemRequest],
                          _history: &[TranslationConversationTurn]| {
            translate_subtitle_chunk_with_gtx(target_language, group)
        };
        match attempt_model(
            AttemptModelArgs {
                model_id: GTX_TRANSLATION_MODEL_ID,
                model_label: GTX_TRANSLATION_MODEL_LABEL,
                group: &group,
                history,
                translated_results,
                snapshot,
                cancelled,
                total_items,
                total_groups: *total_groups,
                completed_groups,
                diagnostics,
                last_error: &mut last_error,
            },
            &mut gtx_fn,
        )? {
            AttemptOutcome::Succeeded | AttemptOutcome::Cancelled => return Ok(()),
            AttemptOutcome::Failed => {}
        }
        if !smart_fallback {
            // Keep retry/split behavior, but do not switch away from the chosen GTX model.
            if candidate_models.is_empty() && group.len() <= 1 {
                return Err(last_error);
            }
        }
    }

    for model in candidate_models {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(());
        }

        let model_label = localized_model_label(model, &config.ui_language);
        let mut model_fn = |group: &[SubtitleTranslationItemRequest],
                            history: &[TranslationConversationTurn]| {
            translate_subtitle_chunk(config, model, target_language, instructions, group, history)
        };
        match attempt_model(
            AttemptModelArgs {
                model_id: &model.id,
                model_label: &model_label,
                group: &group,
                history,
                translated_results,
                snapshot,
                cancelled,
                total_items,
                total_groups: *total_groups,
                completed_groups,
                diagnostics,
                last_error: &mut last_error,
            },
            &mut model_fn,
        )? {
            AttemptOutcome::Succeeded | AttemptOutcome::Cancelled => return Ok(()),
            AttemptOutcome::Failed => {}
        }
    }

    if group.len() > 1 {
        let midpoint = group.len() / 2;
        let right = group.split_off(midpoint);
        let left = group;
        *total_groups += 1;
        diagnostics.record_split(left.len(), right.len(), &last_error);
        translate_group_with_retry(TranslateGroupRequest {
            config,
            candidate_models,
            gtx_selected,
            smart_fallback,
            target_language,
            instructions,
            group: left,
            history,
            translated_results,
            snapshot,
            cancelled,
            total_items,
            total_groups,
            completed_groups,
            diagnostics,
        })?;
        translate_group_with_retry(TranslateGroupRequest {
            config,
            candidate_models,
            gtx_selected,
            smart_fallback,
            target_language,
            instructions,
            group: right,
            history,
            translated_results,
            snapshot,
            cancelled,
            total_items,
            total_groups,
            completed_groups,
            diagnostics,
        })?;
        return Ok(());
    }

    Err(last_error)
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
