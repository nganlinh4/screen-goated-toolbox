use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

static RECOVERY_STARTED: AtomicBool = AtomicBool::new(false);

pub(super) fn ensure_recovery_started() {
    if cfg!(test)
        || RECOVERY_STARTED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
    {
        return;
    }
    if let Err(error) = crate::overlay::creation_output::sweep_staging() {
        crate::log_info!("[Image creator] Saved staging is unavailable: {error}");
        return;
    }
    if let Err(error) = crate::overlay::creation_source_snapshot::sweep() {
        crate::log_info!("[Image creator] Saved sources are unavailable: {error}");
        return;
    }
    let pending_deliveries = match crate::overlay::creation_delivery::reconcile_product("image") {
        Ok(pending) => pending,
        Err(error) => {
            crate::log_info!("[Image creator] Saved completion is unavailable: {error}");
            return;
        }
    };
    let intents = match crate::overlay::creation_intent_journal::load("image") {
        Ok(intents) => intents,
        Err(error) => {
            crate::log_info!("[Image creator] Saved jobs are unavailable: {error}");
            return;
        }
    };
    for intent in intents {
        if pending_deliveries.contains(&intent.job_id) {
            continue;
        }
        let restored = (|| {
            let mut request: StartJobRequest =
                serde_json::from_value(intent.arguments.clone()).map_err(|_| ())?;
            request.dispatch_id = intent.dispatch_id.clone();
            crate::overlay::creation_intent_journal::verify_arguments(
                &intent,
                &serde_json::to_value(&request).map_err(|_| ())?,
            )
            .map_err(|_| ())?;
            revalidate_request_sources(&request).map_err(|_| ())?;
            let staging_dir = request.output_dir.as_deref().map(PathBuf::from).ok_or(())?;
            let final_output_dir = request
                .final_output_dir
                .as_deref()
                .map(PathBuf::from)
                .ok_or(())?;
            crate::overlay::creation_intent_journal::validate_persisted_path(&staging_dir)
                .map_err(|_| ())?;
            crate::overlay::creation_intent_journal::validate_persisted_path(&final_output_dir)
                .map_err(|_| ())?;
            if std::fs::canonicalize(&staging_dir).map_err(|_| ())? != staging_dir
                || std::fs::canonicalize(&final_output_dir).map_err(|_| ())? != final_output_dir
                || request.prompt.is_empty()
                || request.prompt.chars().count() > MAX_PROMPT_CHARACTERS
            {
                return Err(());
            }
            let output_name = request.output_name.as_deref().ok_or(())?;
            let staging_path =
                crate::overlay::creation_output::assigned_path(&staging_dir, output_name)
                    .map_err(|_| ())?;
            crate::overlay::creation_output::validate_staging_path(
                &intent.dispatch_id,
                output_name,
                &staging_path,
            )
            .map_err(|_| ())?;
            crate::overlay::creation_output::require_unoccupied(&final_output_dir, output_name)
                .map_err(|_| ())?;
            Ok(request)
        })();
        let Ok(request) = restored else {
            crate::overlay::creation_intent_journal::clear("image", &intent.job_id);
            continue;
        };
        let display_paths =
            crate::overlay::creation_source_snapshot::original_paths(&request.source_descriptors)
                .unwrap_or_default();
        let status = JobStatus {
            job_id: intent.job_id.clone(),
            operation: OPERATION.to_string(),
            stage: "queued".to_string(),
            progress_text: public_progress::text("queued", !request.image_paths.is_empty())
                .to_string(),
            progress_key: Some(public_progress::key("queued")),
            phase: Some("queued".to_string()),
            elapsed_ms: Some(0),
            estimated_total_ms: Some(180_000),
            progress_ratio: Some(0.0),
            output_path: None,
            output_name: None,
            source_image_path: display_paths.first().cloned().unwrap_or_default(),
            source_image_paths: display_paths,
            output_dir: request.final_output_dir.clone().unwrap_or_default(),
            prompt: request.prompt.clone(),
            mime_type: None,
            width: None,
            height: None,
            error: None,
        };
        if crate::overlay::creation_process_supervisor::deadline_expired(intent.deadline_at_ms) {
            super::cleanup_request_staging(&request);
            crate::overlay::creation_intent_journal::clear("image", &intent.job_id);
            let mut expired = status;
            expired.stage = "failed".to_string();
            expired.progress_text = public_progress::text("failed", false).to_string();
            expired.progress_key = Some(public_progress::key("failed"));
            expired.phase = Some("failed".to_string());
            expired.error = Some("Image creation could not finish. Try again.".to_string());
            if let Ok(mut state) = STATE.lock() {
                state.order.push(intent.job_id.clone());
                state.jobs.insert(intent.job_id, expired);
            }
            continue;
        }
        if let Ok(mut state) = STATE.lock() {
            state.order.push(intent.job_id.clone());
            state.jobs.insert(intent.job_id.clone(), status);
            state.requests.insert(intent.job_id.clone(), request);
            state
                .request_fingerprints
                .insert(intent.job_id.clone(), intent.arguments_fingerprint);
            state
                .deadlines
                .insert(intent.job_id.clone(), intent.deadline_at_ms);
            state.recovered_jobs.insert(intent.job_id);
        }
    }
    schedule_next();
}
