use super::*;

pub(super) fn settle_reconciled_completions() {
    let Ok(active) = crate::overlay::creation_intent_journal::load("svg") else {
        return;
    };
    let active = active
        .into_iter()
        .map(|intent| intent.job_id)
        .collect::<std::collections::HashSet<_>>();
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    let completed = state
        .pending_completions
        .keys()
        .filter(|job_id| {
            !active.contains(*job_id)
                && state
                    .jobs
                    .get(*job_id)
                    .is_some_and(|job| !is_non_publishable(&job.stage))
        })
        .cloned()
        .collect::<Vec<_>>();
    for job_id in completed {
        if let Some(status) = state.pending_completions.remove(&job_id) {
            state.jobs.insert(job_id.clone(), status);
            state.requests.remove(&job_id);
            state.request_fingerprints.remove(&job_id);
            state.deadlines.remove(&job_id);
            state.recovered_jobs.remove(&job_id);
        }
    }
    state.prune_terminal_jobs();
}

pub(super) fn job_cancelled(job_id: &str) -> bool {
    STATE
        .lock()
        .ok()
        .and_then(|state| {
            state
                .jobs
                .get(job_id)
                .map(|job| is_non_publishable(&job.stage))
        })
        .unwrap_or(true)
}

pub(super) fn is_non_publishable(stage: &str) -> bool {
    matches!(stage, "cancelling" | "cancelled")
}

pub(super) fn update_progress(job_id: &str, value: &Value) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    let Some(job) = state.jobs.get_mut(job_id) else {
        return;
    };
    if is_non_publishable(&job.stage) {
        return;
    }
    let stage = match value.get("stage").and_then(Value::as_str) {
        Some("preparing") => "preparing",
        Some("finalizing") => "finalizing",
        Some("generating") => "generating",
        _ => "generating",
    };
    job.stage = stage.to_string();
    let (text, key) = match stage {
        "preparing" => ("Getting ready", "svg.preparing"),
        "finalizing" => ("Finishing vector", "svg.finalizing"),
        _ => ("Creating vector paths", "svg.creating"),
    };
    job.progress_text = text.to_string();
    job.progress_key = Some(key.to_string());
    job.phase = Some(stage.to_string());
    job.elapsed_ms = crate::overlay::creation_progress::elapsed_ms(value).or(job.elapsed_ms);
    job.estimated_total_ms =
        crate::overlay::creation_progress::estimated_total_ms(value).or(job.estimated_total_ms);
    job.progress_ratio = crate::overlay::creation_progress::ratio(value).or(job.progress_ratio);
}

pub(super) fn finish(job_id: &str, result: Result<Value, String>) {
    finish_with_intent(job_id, result, true);
}

pub(super) fn finish_retaining_intent(job_id: &str, result: Result<Value, String>) {
    finish_with_intent(job_id, result, false);
}

fn finish_with_intent(job_id: &str, result: Result<Value, String>, clear_intent: bool) {
    let snapshot = STATE.lock().ok().and_then(|mut state| {
        state.pids.remove(job_id);
        let job = state.jobs.get(job_id)?.clone();
        if is_non_publishable(&job.stage) {
            return None;
        }
        let (request, _) = state.requests.get(job_id)?.clone();
        let fingerprint = state.request_fingerprints.get(job_id)?.clone();
        Some((job, request, fingerprint))
    });
    let Some((mut completed, request, request_fingerprint)) = snapshot else {
        start_preparation();
        schedule_next();
        return;
    };
    let value = match result {
        Ok(value) => value,
        Err(_) => {
            mark_failed(job_id);
            if clear_intent {
                crate::overlay::creation_intent_journal::clear("svg", job_id);
                cleanup_request_staging(&request);
            }
            start_preparation();
            schedule_next();
            return;
        }
    };
    let Some(staging_path) = value
        .get("outputPath")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        mark_failed(job_id);
        start_preparation();
        schedule_next();
        return;
    };
    let Some(final_dir) = request.final_output_dir.as_deref().map(PathBuf::from) else {
        mark_failed(job_id);
        start_preparation();
        schedule_next();
        return;
    };
    let Ok(output_path) =
        crate::overlay::creation_output::assigned_path(&final_dir, &request.output_name)
    else {
        mark_failed(job_id);
        start_preparation();
        schedule_next();
        return;
    };
    completed.stage = "done".to_string();
    completed.progress_text = "Vector ready".to_string();
    completed.progress_key = Some("svg.done".to_string());
    completed.phase = Some("complete".to_string());
    completed.progress_ratio = Some(1.0);
    completed.output_path = Some(output_path.to_string_lossy().to_string());
    completed.output_name = Some(request.output_name.clone());
    completed.source_image_path = match crate::overlay::creation_source_snapshot::presentation_path(
        &request.source_descriptors,
    ) {
        Ok(path) if !path.is_empty() => path,
        _ => {
            mark_failed(job_id);
            if clear_intent {
                crate::overlay::creation_intent_journal::clear("svg", job_id);
                cleanup_request_staging(&request);
            }
            start_preparation();
            schedule_next();
            return;
        }
    };
    let delivery = crate::overlay::creation_delivery::commit(
        crate::overlay::creation_delivery::PublishedDelivery {
            product: "svg",
            job_id: job_id.to_string(),
            dispatch_id: request.dispatch_id.clone(),
            request_fingerprint,
            source_path: completed.source_image_path.clone(),
            output_name: request.output_name.clone(),
            staging_path,
            output_path: output_path.to_string_lossy().to_string(),
            companion: None,
            metadata: json!({
                "model": completed.model,
                "backgroundMode": completed.background_mode,
            }),
        },
    );
    match delivery {
        Ok(()) => mark_completed(job_id, completed),
        Err(error) => {
            crate::log_info!("[Image to SVG] Completion is still pending: {error}");
            if let Ok(mut state) = STATE.lock()
                && let Some(job) = state.jobs.get_mut(job_id)
                && !is_non_publishable(&job.stage)
            {
                job.stage = "finalizing".to_string();
                job.progress_text = "Finishing vector".to_string();
                job.progress_key = Some("svg.finalizing".to_string());
                job.phase = Some("finalizing".to_string());
                state
                    .pending_completions
                    .insert(job_id.to_string(), completed);
            }
        }
    }
    start_preparation();
    schedule_next();
}

fn mark_failed(job_id: &str) {
    if let Ok(mut state) = STATE.lock() {
        let Some(job) = state.jobs.get_mut(job_id) else {
            return;
        };
        if is_non_publishable(&job.stage) {
            return;
        }
        job.stage = "failed".to_string();
        job.progress_text = "Could not create vector".to_string();
        job.progress_key = Some("svg.failed".to_string());
        job.phase = Some("failed".to_string());
        job.error = Some("Vector creation could not finish. Retry this image.".to_string());
        state.requests.remove(job_id);
        state.request_fingerprints.remove(job_id);
        state.deadlines.remove(job_id);
        state.recovered_jobs.remove(job_id);
        state.prune_terminal_jobs();
    }
}

fn mark_completed(job_id: &str, completed: JobStatus) {
    if let Ok(mut state) = STATE.lock()
        && state
            .jobs
            .get(job_id)
            .is_some_and(|job| job.stage != "cancelled")
    {
        state.jobs.insert(job_id.to_string(), completed);
        state.requests.remove(job_id);
        state.request_fingerprints.remove(job_id);
        state.deadlines.remove(job_id);
        state.recovered_jobs.remove(job_id);
        state.pending_completions.remove(job_id);
        state.prune_terminal_jobs();
    }
}

pub(super) fn cleanup_request_staging(request: &StartJobRequest) {
    let Ok(path) =
        crate::overlay::creation_output::staging_path(&request.dispatch_id, &request.output_name)
    else {
        return;
    };
    let _ = crate::overlay::creation_output::cleanup_staging(
        &request.dispatch_id,
        &request.output_name,
        &path,
    );
}
