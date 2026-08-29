use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{JobStatus, STATE, StartJobRequest, schedule_next};

static STARTED: AtomicBool = AtomicBool::new(false);

pub(super) fn ensure_recovery_started() {
    if cfg!(test)
        || STARTED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
    {
        return;
    }
    if let Err(error) = crate::overlay::creation_output::sweep_staging() {
        crate::log_info!("[Image to SVG] Saved staging is unavailable: {error}");
        return;
    }
    if let Err(error) = crate::overlay::creation_source_snapshot::sweep() {
        crate::log_info!("[Image to SVG] Saved sources are unavailable: {error}");
        return;
    }
    let pending_deliveries = match crate::overlay::creation_delivery::reconcile_product("svg") {
        Ok(pending) => pending,
        Err(error) => {
            crate::log_info!("[Image to SVG] Saved completion is unavailable: {error}");
            return;
        }
    };
    let intents = match crate::overlay::creation_intent_journal::load("svg") {
        Ok(intents) => intents,
        Err(error) => {
            crate::log_info!("[Image to SVG] Saved jobs are unavailable: {error}");
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
            if request.source_descriptors.len() != 1
                || request.source_descriptors[0].path != request.image_path
                || crate::overlay::creation_source_snapshot::validate_sources(
                    &request.source_descriptors,
                )
                .is_err()
            {
                return Err(());
            }
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
            {
                return Err(());
            }
            let staging_path =
                crate::overlay::creation_output::assigned_path(&staging_dir, &request.output_name)
                    .map_err(|_| ())?;
            crate::overlay::creation_output::validate_staging_path(
                &intent.dispatch_id,
                &request.output_name,
                &staging_path,
            )
            .map_err(|_| ())?;
            crate::overlay::creation_output::require_unoccupied(
                &final_output_dir,
                &request.output_name,
            )
            .map_err(|_| ())?;
            Ok((request, staging_dir, final_output_dir))
        })();
        let Ok((request, staging_dir, final_output_dir)) = restored else {
            crate::overlay::creation_intent_journal::clear("svg", &intent.job_id);
            continue;
        };
        let display_path =
            crate::overlay::creation_source_snapshot::original_paths(&request.source_descriptors)
                .ok()
                .and_then(|paths| paths.into_iter().next())
                .unwrap_or_default();
        let status = JobStatus {
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
            output_dir: final_output_dir.to_string_lossy().to_string(),
            model: request.model.clone(),
            background_mode: request.background_mode.clone(),
            error: None,
        };
        if crate::overlay::creation_process_supervisor::deadline_expired(intent.deadline_at_ms) {
            super::cleanup_request_staging(&request);
            crate::overlay::creation_intent_journal::clear("svg", &intent.job_id);
            let mut expired = status;
            expired.stage = "failed".to_string();
            expired.progress_text = "Could not create vector".to_string();
            expired.progress_key = Some("svg.failed".to_string());
            expired.phase = Some("failed".to_string());
            expired.error = Some("Vector creation could not finish. Retry this image.".to_string());
            if let Ok(mut state) = STATE.lock() {
                state.order.push(intent.job_id.clone());
                state.jobs.insert(intent.job_id, expired);
            }
            continue;
        }
        if let Ok(mut state) = STATE.lock() {
            state.order.push(intent.job_id.clone());
            state.jobs.insert(intent.job_id.clone(), status);
            state
                .requests
                .insert(intent.job_id.clone(), (request, staging_dir));
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
