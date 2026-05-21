use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::APP;
use crate::config::Config;
use crate::model_config::{
    ModelConfig, ModelType, get_all_models_with_ollama, get_model_by_id, model_is_non_llm,
};
use crate::retry_model_chain::{
    RetryChainKind, preflight_skip_reason, provider_is_available, resolve_next_retry_model,
};

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
    let mut model_payload = vec![SubtitleTranslationModelCapability {
        model_id: GTX_TRANSLATION_MODEL_ID.to_string(),
        model_label: GTX_TRANSLATION_MODEL_LABEL.to_string(),
        provider: "gtx".to_string(),
    }];
    model_payload.extend(
        models
            .into_iter()
            .map(|model| SubtitleTranslationModelCapability {
                model_id: model.id.clone(),
                model_label: localized_model_label(&model, &config.ui_language),
                provider: model.provider.clone(),
            }),
    );
    let payload = SubtitleTranslationCapabilities {
        available: true,
        reason: None,
        models: model_payload,
    };
    serde_json::to_value(payload)
        .map_err(|error| format!("Serialize subtitle translation capabilities: {error}"))
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
    let gtx_prioritized = selected_model_id == GTX_TRANSLATION_MODEL_ID;
    let candidate_models = collect_prioritized_translation_models(&config, selected_model_id)?;
    if candidate_models.is_empty() && !gtx_prioritized {
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
    diagnostics.log_start(
        request.items.len(),
        chunks.len(),
        request.chunk_mode.as_deref(),
        &request.target_language,
        request.instructions.as_deref(),
        selected_model_id,
        gtx_prioritized,
        &candidate_models,
        &config.ui_language,
    );
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
            gtx_prioritized,
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
    gtx_prioritized: bool,
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
        gtx_prioritized,
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

    if gtx_prioritized {
        update_translation_snapshot(snapshot, |locked| {
            locked.current_model_id = Some(GTX_TRANSLATION_MODEL_ID.to_string());
            locked.current_model_label = Some(GTX_TRANSLATION_MODEL_LABEL.to_string());
            locked.current_chunk_count = *total_groups;
            locked.current_chunk_index = *completed_groups + 1;
            locked.total_chunks = *total_groups;
            locked.progress = translated_results.len() as f64 / total_items.max(1) as f64;
            locked.results = translated_results.clone();
            locked.message = format!(
                "Translating subtitles with {} ({}/{})",
                GTX_TRANSLATION_MODEL_LABEL,
                *completed_groups + 1,
                *total_groups
            );
            locked.message_key = Some("subtitleTranslationStatusChunk".to_string());
            locked.message_params = HashMap::from([
                ("model".to_string(), GTX_TRANSLATION_MODEL_LABEL.to_string()),
                ("current".to_string(), (*completed_groups + 1).to_string()),
                ("total".to_string(), total_groups.to_string()),
            ]);
        })?;
        match translate_subtitle_chunk_with_gtx(target_language, &group) {
            Ok(response) => {
                diagnostics.record_success(GTX_TRANSLATION_MODEL_ID, group.len(), 1);
                history.push(TranslationConversationTurn {
                    user_payload: response.user_payload,
                    assistant_payload: response.assistant_payload,
                });
                translated_results.extend(response.items);
                *completed_groups += 1;
                update_translation_snapshot(snapshot, |locked| {
                    locked.progress = translated_results.len() as f64 / total_items.max(1) as f64;
                    locked.results = translated_results.clone();
                })?;
                return Ok(());
            }
            Err(error) => {
                diagnostics.record_failure(
                    GTX_TRANSLATION_MODEL_ID,
                    GTX_TRANSLATION_MODEL_LABEL,
                    1,
                    group.len(),
                    history.len(),
                    &error,
                );
                last_error = error;
            }
        }
    }

    for model in candidate_models {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(());
        }

        let model_label = localized_model_label(model, &config.ui_language);
        update_translation_snapshot(snapshot, |locked| {
            locked.current_model_id = Some(model.id.clone());
            locked.current_model_label = Some(model_label.clone());
            locked.current_chunk_count = *total_groups;
            locked.current_chunk_index = *completed_groups + 1;
            locked.total_chunks = *total_groups;
            locked.progress = translated_results.len() as f64 / total_items.max(1) as f64;
            locked.results = translated_results.clone();
            locked.message = format!(
                "Translating subtitles with {} ({}/{})",
                model_label,
                *completed_groups + 1,
                *total_groups
            );
            locked.message_key = Some("subtitleTranslationStatusChunk".to_string());
            locked.message_params = HashMap::from([
                ("model".to_string(), model_label.clone()),
                ("current".to_string(), (*completed_groups + 1).to_string()),
                ("total".to_string(), total_groups.to_string()),
            ]);
        })?;

        for attempt_index in 0..TRANSLATION_MODEL_ATTEMPTS {
            if cancelled.load(Ordering::SeqCst) {
                return Ok(());
            }
            if attempt_index > 0 {
                let delay = translation_retry_delay(attempt_index);
                update_translation_snapshot(snapshot, |locked| {
                    locked.message = format!(
                        "Retrying subtitle translation with {} ({}/{}, attempt {}/{})",
                        model_label,
                        *completed_groups + 1,
                        *total_groups,
                        attempt_index + 1,
                        TRANSLATION_MODEL_ATTEMPTS
                    );
                    locked.message_key = Some("subtitleTranslationStatusRetry".to_string());
                    locked.message_params = HashMap::from([
                        ("model".to_string(), model_label.clone()),
                        ("current".to_string(), (*completed_groups + 1).to_string()),
                        ("total".to_string(), total_groups.to_string()),
                        ("attempt".to_string(), (attempt_index + 1).to_string()),
                        (
                            "attempts".to_string(),
                            TRANSLATION_MODEL_ATTEMPTS.to_string(),
                        ),
                    ]);
                })?;
                sleep_cancelable(cancelled, delay);
                if cancelled.load(Ordering::SeqCst) {
                    return Ok(());
                }
            }

            match translate_subtitle_chunk(
                config,
                model,
                target_language,
                instructions,
                &group,
                history,
            ) {
                Ok(response) => {
                    diagnostics.record_success(&model.id, group.len(), attempt_index + 1);
                    history.push(TranslationConversationTurn {
                        user_payload: response.user_payload,
                        assistant_payload: response.assistant_payload,
                    });
                    translated_results.extend(response.items);
                    *completed_groups += 1;
                    update_translation_snapshot(snapshot, |locked| {
                        locked.progress =
                            translated_results.len() as f64 / total_items.max(1) as f64;
                        locked.results = translated_results.clone();
                    })?;
                    return Ok(());
                }
                Err(error) => {
                    diagnostics.record_failure(
                        &model.id,
                        &model_label,
                        attempt_index + 1,
                        group.len(),
                        history.len(),
                        &error,
                    );
                    last_error = error;
                }
            }
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
            gtx_prioritized,
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
            gtx_prioritized,
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

struct TranslationDiagnostics {
    job_id: String,
    started_at: std::time::Instant,
    attempt_count: usize,
    success_count: usize,
    retry_success_count: usize,
    split_count: usize,
    failure_counts: HashMap<String, usize>,
    model_attempt_counts: HashMap<String, usize>,
}

impl TranslationDiagnostics {
    fn new(job_id: &str) -> Self {
        Self {
            job_id: job_id.to_string(),
            started_at: std::time::Instant::now(),
            attempt_count: 0,
            success_count: 0,
            retry_success_count: 0,
            split_count: 0,
            failure_counts: HashMap::new(),
            model_attempt_counts: HashMap::new(),
        }
    }

    fn log_start(
        &self,
        item_count: usize,
        chunk_count: usize,
        chunk_mode: Option<&str>,
        target_language: &str,
        instructions: Option<&str>,
        selected_model_id: &str,
        gtx_prioritized: bool,
        candidate_models: &[ModelConfig],
        ui_language: &str,
    ) {
        let model_chain = candidate_models
            .iter()
            .map(|model| {
                format!(
                    "{}:{}",
                    model.provider,
                    localized_model_label(model, ui_language)
                )
            })
            .collect::<Vec<_>>()
            .join(" > ");
        eprintln!(
            "[SubtitleTranslation][job={}] start items={} initial_chunks={} chunk_mode={} target=\"{}\" instructions={} prioritized_model=\"{}\" gtx_prioritized={} fallback_chain=\"{}\"",
            self.job_id,
            item_count,
            chunk_count,
            chunk_mode.unwrap_or("auto"),
            target_language,
            instructions
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            selected_model_id,
            gtx_prioritized,
            if model_chain.is_empty() {
                "none"
            } else {
                &model_chain
            }
        );
    }

    fn record_success(&mut self, model_id: &str, chunk_items: usize, attempts: usize) {
        self.success_count += 1;
        if attempts > 1 {
            self.retry_success_count += 1;
            eprintln!(
                "[SubtitleTranslation][job={}] retry-success model={} chunk_items={} attempts={}",
                self.job_id, model_id, chunk_items, attempts
            );
        }
    }

    fn record_failure(
        &mut self,
        model_id: &str,
        model_label: &str,
        attempt: usize,
        chunk_items: usize,
        history_turns: usize,
        error: &str,
    ) {
        self.attempt_count += 1;
        *self
            .model_attempt_counts
            .entry(model_id.to_string())
            .or_default() += 1;
        let category = classify_translation_error(error);
        let key = format!("{model_id}:{category}");
        let category_count = self.failure_counts.entry(key).or_default();
        *category_count += 1;

        // Log the first occurrence and then sparse repeats. This keeps long
        // translation jobs readable while still showing persistent failure modes.
        if *category_count == 1 || *category_count == 5 || *category_count % 20 == 0 {
            eprintln!(
                "[SubtitleTranslation][job={}] failure model=\"{}\" category={} count={} attempt={} chunk_items={} history_turns={} detail=\"{}\"",
                self.job_id,
                model_label,
                category,
                category_count,
                attempt,
                chunk_items,
                history_turns,
                truncate_log_detail(error, 220)
            );
        }
    }

    fn record_split(&mut self, left_items: usize, right_items: usize, last_error: &str) {
        self.split_count += 1;
        eprintln!(
            "[SubtitleTranslation][job={}] split #{} left_items={} right_items={} reason_category={} reason=\"{}\"",
            self.job_id,
            self.split_count,
            left_items,
            right_items,
            classify_translation_error(last_error),
            truncate_log_detail(last_error, 180)
        );
    }

    fn log_finish(
        &self,
        state: &str,
        translated_items: usize,
        requested_items: usize,
        completed_groups: usize,
        total_groups: usize,
    ) {
        let failures = summarize_count_map(&self.failure_counts);
        let model_attempts = summarize_count_map(&self.model_attempt_counts);
        eprintln!(
            "[SubtitleTranslation][job={}] finish state={} translated_items={}/{} groups={}/{} attempts={} successes={} retry_successes={} splits={} elapsed_ms={} failures=\"{}\" model_attempts=\"{}\"",
            self.job_id,
            state,
            translated_items,
            requested_items,
            completed_groups,
            total_groups,
            self.attempt_count,
            self.success_count,
            self.retry_success_count,
            self.split_count,
            self.started_at.elapsed().as_millis(),
            failures,
            model_attempts
        );
    }
}

fn classify_translation_error(error: &str) -> &'static str {
    let lower = error.to_lowercase();
    if lower.contains("gtx") {
        "gtx"
    } else if lower.contains("parse structured translation json") || lower.contains("json") {
        "json"
    } else if lower.contains("returned")
        && (lower.contains("item") || lower.contains("id") || lower.contains("empty text"))
    {
        "schema"
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "timeout"
    } else if lower.contains("rate limit") || lower.contains("429") {
        "rate-limit"
    } else if lower.contains("401") || lower.contains("403") || lower.contains("api key") {
        "auth"
    } else if lower.contains("network")
        || lower.contains("connection")
        || lower.contains("dns")
        || lower.contains("http")
        || lower.contains("request")
    {
        "transport"
    } else if lower.contains("no content") || lower.contains("empty") {
        "empty"
    } else {
        "other"
    }
}

fn truncate_log_detail(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    let mut truncated = compact.chars().take(max_chars).collect::<String>();
    truncated.push('…');
    truncated
}

fn summarize_count_map(counts: &HashMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    let mut entries = counts
        .iter()
        .map(|(key, count)| (key.as_str(), *count))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    entries
        .into_iter()
        .map(|(key, count)| format!("{key}={count}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn translation_retry_delay(attempt_index: usize) -> Duration {
    let multiplier = 1u64 << attempt_index.saturating_sub(1).min(4);
    Duration::from_millis(TRANSLATION_RETRY_BASE_DELAY_MS * multiplier)
}

fn sleep_cancelable(cancelled: &AtomicBool, duration: Duration) {
    let step = Duration::from_millis(100);
    let mut slept = Duration::ZERO;
    while slept < duration && !cancelled.load(Ordering::SeqCst) {
        let remaining = duration.saturating_sub(slept);
        let next = remaining.min(step);
        std::thread::sleep(next);
        slept += next;
    }
}

fn initial_translation_chunk_count(
    chunk_count: Option<usize>,
    chunk_mode: &Option<String>,
    item_count: usize,
) -> usize {
    if item_count <= 1 {
        return 1;
    }
    if let Some(chunk_count) = chunk_count {
        return chunk_count.max(1).min(item_count);
    }

    let items_per_chunk = match chunk_mode.as_deref() {
        Some("small") => 25,
        Some("tiny") => 10,
        _ => item_count,
    };
    ((item_count + items_per_chunk - 1) / items_per_chunk)
        .max(1)
        .min(item_count)
}

fn split_translation_items(
    items: &[SubtitleTranslationItemRequest],
    chunk_count: usize,
) -> Vec<Vec<SubtitleTranslationItemRequest>> {
    let mut grouped: Vec<(Option<String>, Vec<SubtitleTranslationItemRequest>)> = Vec::new();
    for item in items {
        let group_id = item.source_group_id.clone();
        if let Some((last_group_id, last_items)) = grouped.last_mut() {
            if *last_group_id == group_id {
                last_items.push(item.clone());
                continue;
            }
        }
        grouped.push((group_id, vec![item.clone()]));
    }

    let total_items = items.len().max(1);
    let safe_chunk_count = chunk_count.max(1).min(total_items);
    let mut chunks = Vec::with_capacity(safe_chunk_count);
    for (_group_id, group_items) in grouped {
        let group_chunk_count = ((safe_chunk_count * group_items.len() + total_items - 1)
            / total_items)
            .max(1)
            .min(group_items.len().max(1));
        for chunk_index in 0..group_chunk_count {
            let start = chunk_index * group_items.len() / group_chunk_count;
            let end = (chunk_index + 1) * group_items.len() / group_chunk_count;
            if start < end {
                chunks.push(group_items[start..end].to_vec());
            }
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
    let mut seen_model_ids = HashSet::new();

    if let Some(initial_model) = resolve_initial_translation_model(config, &blocked_providers) {
        seen_model_ids.insert(initial_model.id.clone());
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
            if failed_model_ids
                .iter()
                .any(|failed| failed == &next_model.id)
            {
                break;
            }
            failed_model_ids.push(next_model.id.clone());
            current_model = next_model.clone();
            if seen_model_ids.insert(next_model.id.clone()) {
                models.push(next_model);
            }
        }
    }

    for model in get_all_models_with_ollama()
        .into_iter()
        .filter(|model| is_compatible_translation_model(model, config, &blocked_providers))
    {
        if seen_model_ids.insert(model.id.clone()) {
            models.push(model);
        }
    }

    models
}

fn collect_prioritized_translation_models(
    config: &Config,
    model_id: &str,
) -> Result<Vec<ModelConfig>, String> {
    if model_id == GTX_TRANSLATION_MODEL_ID {
        return Ok(collect_translation_models(config));
    }
    let blocked_providers = HashSet::new();
    let Some(model) = get_model_by_id(model_id).or_else(|| {
        get_all_models_with_ollama()
            .into_iter()
            .find(|model| model.id == model_id)
    }) else {
        return Err(format!("Unknown subtitle translation model: {model_id}"));
    };
    if !is_compatible_translation_model(&model, config, &blocked_providers) {
        return Err(format!(
            "Subtitle translation model '{}' is not currently available.",
            localized_model_label(&model, &config.ui_language)
        ));
    }
    let mut models = Vec::new();
    let mut seen_model_ids = HashSet::new();
    seen_model_ids.insert(model.id.clone());
    models.push(model);
    for fallback in collect_translation_models(config) {
        if seen_model_ids.insert(fallback.id.clone()) {
            models.push(fallback);
        }
    }
    Ok(models)
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
