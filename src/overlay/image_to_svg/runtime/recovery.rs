use super::*;
use crate::overlay::creation_recovery::retry::Retry;
use crate::overlay::creation_recovery::saved::{RestoreError, validate_directory};

static RETRY: Retry = Retry::new("svg", recover_background);

pub(super) fn ensure_recovery_started() {
    if cfg!(test) {
        return;
    }
    RETRY.start(
        || ADMISSION.try_acquire(),
        || recover_pending(schedule_next),
    );
}

fn recover_background() {
    RETRY.run(
        || ADMISSION.try_acquire(),
        || recover_pending(schedule_next),
    );
}

pub(super) fn recovery_pending() -> bool {
    RETRY.pending()
}

fn recover_pending(schedule: impl FnOnce()) -> bool {
    let pending_deliveries = match crate::overlay::creation_delivery::reconcile_product("svg") {
        Ok(pending) => pending,
        Err(error) => {
            crate::log_info!("[Creation] Saved completion is unavailable: {error}");
            return false;
        }
    };
    let intents = match crate::overlay::creation_intent_journal::load("svg") {
        Ok(intents) => intents,
        Err(error) => {
            crate::log_info!("[Creation] Saved jobs are unavailable: {error}");
            return false;
        }
    };
    let mut deferred = !pending_deliveries.is_empty();
    for intent in intents {
        if crate::overlay::creation_close::is_closing("svg") {
            deferred = true;
            break;
        }
        if pending_deliveries.contains(&intent.job_id) {
            continue;
        }
        match STATE.lock() {
            Ok(state) if state.jobs.contains_key(&intent.job_id) => continue,
            Err(_) => {
                deferred = true;
                continue;
            }
            _ => {}
        }
        if crate::overlay::creation_process_supervisor::deadline_expired(intent.deadline_at_ms) {
            if retire_expired(&intent).is_err() {
                deferred = true;
            }
            continue;
        }
        let request = match restore_request(&intent) {
            Ok(request) => request,
            Err(RestoreError::Invalid) => {
                if crate::overlay::creation_intent_journal::clear_required("svg", &intent.job_id)
                    .is_err()
                {
                    deferred = true;
                }
                continue;
            }
            Err(RestoreError::Deferred) => {
                deferred = true;
                continue;
            }
        };
        let status = status_for(&intent, &request);
        let Ok(mut state) = STATE.lock() else {
            deferred = true;
            continue;
        };
        if crate::overlay::creation_close::is_closing("svg") {
            deferred = true;
            break;
        }
        if state.jobs.contains_key(&intent.job_id) {
            continue;
        }
        state.order.push(intent.job_id.clone());
        state.jobs.insert(intent.job_id.clone(), status);
        let staging = PathBuf::from(
            request
                .output_dir
                .as_deref()
                .expect("validated staging assignment"),
        );
        state
            .requests
            .insert(intent.job_id.clone(), (request, staging));
        state
            .request_fingerprints
            .insert(intent.job_id.clone(), intent.arguments_fingerprint);
        state
            .deadlines
            .insert(intent.job_id.clone(), intent.deadline_at_ms);
        state.recovered_jobs.insert(intent.job_id);
    }
    if let Err(error) = crate::overlay::creation_output::sweep_staging() {
        crate::log_info!("[Creation] Saved staging is unavailable: {error}");
        deferred = true;
    }
    if let Err(error) = crate::overlay::creation_source_snapshot::sweep() {
        crate::log_info!("[Creation] Saved sources are unavailable: {error}");
        deferred = true;
    }
    if !crate::overlay::creation_close::is_closing("svg") {
        schedule();
    }
    !deferred
}

fn decode_request(
    intent: &crate::overlay::creation_intent_journal::Intent,
) -> Result<StartJobRequest, RestoreError> {
    let mut request: StartJobRequest =
        serde_json::from_value(intent.arguments.clone()).map_err(|_| ())?;
    request.dispatch_id = intent.dispatch_id.clone();
    crate::overlay::creation_intent_journal::verify_arguments(
        intent,
        &serde_json::to_value(&request).map_err(|_| ())?,
    )
    .map_err(|_| ())?;
    if request.source_descriptors.len() != 1
        || request.source_descriptors[0].path != request.image_path
    {
        return Err(RestoreError::Invalid);
    }
    let staging = request.output_dir.as_deref().map(PathBuf::from).ok_or(())?;
    let destination = request
        .final_output_dir
        .as_deref()
        .map(PathBuf::from)
        .ok_or(())?;
    for directory in [&staging, &destination] {
        crate::overlay::creation_intent_journal::validate_persisted_path(directory)
            .map_err(|_| ())?;
        crate::overlay::creation_path::normalized_absolute(directory).map_err(|_| ())?;
    }
    let output_name = request.output_name.as_str();
    let path =
        crate::overlay::creation_output::assigned_path(&staging, output_name).map_err(|_| ())?;
    crate::overlay::creation_output::validate_staging_assignment(
        &intent.dispatch_id,
        output_name,
        &path,
    )
    .map_err(|_| ())?;
    Ok(request)
}

fn restore_request(
    intent: &crate::overlay::creation_intent_journal::Intent,
) -> Result<StartJobRequest, RestoreError> {
    let request = decode_request(intent)?;
    crate::overlay::creation_source_snapshot::validate_sources(&request.source_descriptors)
        .map_err(|_| RestoreError::Deferred)?;
    let staging = PathBuf::from(request.output_dir.as_deref().ok_or(())?);
    let destination = PathBuf::from(request.final_output_dir.as_deref().ok_or(())?);
    validate_directory(&staging)?;
    validate_directory(&destination)?;
    let output_name = request.output_name.as_str();
    let path =
        crate::overlay::creation_output::assigned_path(&staging, output_name).map_err(|_| ())?;
    crate::overlay::creation_output::validate_staging_path(&intent.dispatch_id, output_name, &path)
        .map_err(|_| RestoreError::Deferred)?;
    crate::overlay::creation_output::require_unoccupied(&destination, output_name)
        .map_err(|_| RestoreError::Deferred)?;
    Ok(request)
}

fn status_for(
    intent: &crate::overlay::creation_intent_journal::Intent,
    request: &StartJobRequest,
) -> JobStatus {
    let display_path =
        crate::overlay::creation_source_snapshot::original_paths(&request.source_descriptors)
            .ok()
            .and_then(|paths| paths.into_iter().next())
            .unwrap_or_default();
    JobStatus {
        job_id: intent.job_id.clone(),
        stage: "queued".to_string(),
        progress_text: "Queued".to_string(),
        progress_key: Some("svg.queued".to_string()),
        phase: Some("queued".to_string()),
        elapsed_ms: Some(0),
        estimated_total_ms: None,
        progress_ratio: Some(0.0),
        output_path: None,
        output_name: None,
        source_image_path: display_path,
        output_dir: request.final_output_dir.clone().unwrap_or_default(),
        model: request.model.clone(),
        background_mode: request.background_mode.clone(),
        error: None,
    }
}

fn retire_expired(intent: &crate::overlay::creation_intent_journal::Intent) -> Result<(), String> {
    let mut status = decode_request(intent)
        .ok()
        .map(|request| status_for(intent, &request))
        .unwrap_or_else(|| JobStatus {
            job_id: intent.job_id.clone(),
            ..JobStatus::default()
        });
    crate::overlay::creation_intent_journal::clear_required("svg", &intent.job_id)?;
    if let Some(name) = intent.arguments.get("outputName").and_then(Value::as_str)
        && let Ok(path) = crate::overlay::creation_output::staging_path(&intent.dispatch_id, name)
    {
        let _ = crate::overlay::creation_output::cleanup_staging(&intent.dispatch_id, name, &path);
    }
    status.stage = "failed".to_string();
    status.progress_text = "Could not create vector".to_string();
    status.progress_key = Some("svg.failed".to_string());
    status.error = Some("Vector creation could not finish. Retry this image.".to_string());
    status.phase = Some("failed".to_string());
    let mut state = STATE
        .lock()
        .map_err(|_| "Creation job state is unavailable.".to_string())?;
    if !state.jobs.contains_key(&intent.job_id) {
        state.order.push(intent.job_id.clone());
        state.jobs.insert(intent.job_id.clone(), status);
    }
    Ok(())
}

#[cfg(test)]
#[path = "recovery/queued_tests.rs"]
mod queued_tests;
