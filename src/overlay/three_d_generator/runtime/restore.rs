use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use super::{
    Continuation, GenerationMode, JobStatus, RuntimeOperation, STATE, StartJobRequest,
    runtime_status_label, schedule_next,
};

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
        crate::log_info!("[3D Generator] Saved staging is unavailable: {error}");
        return;
    }
    if let Err(error) = crate::overlay::creation_source_snapshot::sweep() {
        crate::log_info!("[3D Generator] Saved sources are unavailable: {error}");
        return;
    }
    let pending_deliveries = match crate::overlay::creation_delivery::reconcile_product("3d") {
        Ok(pending) => pending,
        Err(error) => {
            crate::log_info!("[3D Generator] Saved completion is unavailable: {error}");
            return;
        }
    };
    let intents = match crate::overlay::creation_intent_journal::load("3d") {
        Ok(intents) => intents,
        Err(error) => {
            crate::log_info!("[3D Generator] Saved jobs are unavailable: {error}");
            return;
        }
    };
    for intent in intents {
        if pending_deliveries.contains(&intent.job_id) {
            continue;
        }
        let restored = if intent.arguments.get("parentDispatchId").is_some() {
            restore_segment(&intent)
        } else {
            restore_generation(&intent)
        };
        let Ok((operation, status)) = restored else {
            crate::overlay::creation_intent_journal::clear("3d", &intent.job_id);
            continue;
        };
        if intent.deadline_at_ms <= super::now_ms() {
            if let Ok(path) = crate::overlay::creation_output::staging_path(
                operation.dispatch_id(),
                operation.output_name(),
            ) {
                let _ = crate::overlay::creation_output::cleanup_staging(
                    operation.dispatch_id(),
                    operation.output_name(),
                    &path,
                );
            }
            crate::overlay::creation_intent_journal::clear("3d", &intent.job_id);
            let mut expired = status;
            expired.stage = "failed".to_string();
            expired.progress_text = "Creation was interrupted.".to_string();
            expired.phase = Some("failed".to_string());
            expired.error = Some("recovery_failed".to_string());
            if let Ok(mut state) = STATE.lock() {
                state.insert_job(intent.job_id, expired);
            }
            continue;
        }
        if let Ok(mut state) = STATE.lock() {
            state.insert_job(intent.job_id.clone(), status);
            state.operations.insert(intent.job_id.clone(), operation);
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

fn restore_generation(
    intent: &crate::overlay::creation_intent_journal::Intent,
) -> Result<(RuntimeOperation, JobStatus), ()> {
    let mut request: StartJobRequest =
        serde_json::from_value(intent.arguments.clone()).map_err(|_| ())?;
    request.dispatch_id = intent.dispatch_id.clone();
    verify(intent, &serde_json::to_value(&request).map_err(|_| ())?)?;
    if request.source_descriptors.len() != 1
        || request.source_descriptors[0].path != request.image_path
        || crate::overlay::creation_source_snapshot::validate_sources(&request.source_descriptors)
            .is_err()
    {
        return Err(());
    }
    let display_path =
        crate::overlay::creation_source_snapshot::original_paths(&request.source_descriptors)
            .ok()
            .and_then(|paths| paths.into_iter().next())
            .unwrap_or_default();
    let staging_dir = request.output_dir.as_deref().map(PathBuf::from).ok_or(())?;
    let final_output_dir = request
        .final_output_dir
        .as_deref()
        .map(PathBuf::from)
        .ok_or(())?;
    validate_directory(&staging_dir)?;
    validate_directory(&final_output_dir)?;
    let staging_path =
        crate::overlay::creation_output::assigned_path(&staging_dir, &request.output_name)
            .map_err(|_| ())?;
    crate::overlay::creation_output::validate_staging_path(
        &intent.dispatch_id,
        &request.output_name,
        &staging_path,
    )
    .map_err(|_| ())?;
    crate::overlay::creation_output::require_unoccupied(&final_output_dir, &request.output_name)
        .map_err(|_| ())?;
    let status = JobStatus {
        job_id: Some(intent.job_id.clone()),
        stage: "queued".to_string(),
        progress_text: "Queued.".to_string(),
        phase: Some("queued".to_string()),
        elapsed_ms: Some(0),
        estimated_total_ms: None,
        progress_ratio: Some(0.0),
        timing_sample_count: None,
        output_path: None,
        output_name: None,
        source_image_path: Some(display_path),
        output_dir: Some(final_output_dir.to_string_lossy().to_string()),
        generation_mode: Some(request.generation_mode),
        polycount: Some(request.polycount),
        auto_segment: Some(request.auto_segment),
        instruction: request.instruction.clone(),
        is_segmented: false,
        can_segment: false,
        error: None,
        runtime_status: runtime_status_label(),
    };
    Ok((
        RuntimeOperation::Generate {
            request,
            output_dir: staging_dir,
            final_output_dir,
        },
        status,
    ))
}

fn restore_segment(
    intent: &crate::overlay::creation_intent_journal::Intent,
) -> Result<(RuntimeOperation, JobStatus), ()> {
    verify(intent, &intent.arguments)?;
    let string = |key: &str| {
        intent
            .arguments
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(())
    };
    let image_path = string("imagePath")?;
    let staging_dir = PathBuf::from(string("outputDir")?);
    let output_dir = PathBuf::from(string("finalOutputDir")?);
    let output_name = string("outputName")?;
    let previous_output_path = PathBuf::from(string("previousOutputPath")?);
    let parent_dispatch_id = string("parentDispatchId")?;
    let generation_mode: GenerationMode =
        serde_json::from_value(intent.arguments.get("generationMode").cloned().ok_or(())?)
            .map_err(|_| ())?;
    let polycount = intent
        .arguments
        .get("polycount")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(())?;
    let auto_segment = intent
        .arguments
        .get("autoSegment")
        .and_then(Value::as_bool)
        .ok_or(())?;
    let instruction = match intent.arguments.get("instruction") {
        Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.clone()),
        _ => return Err(()),
    };
    if !super::generation_mode::frozen_settings_valid(generation_mode, polycount, auto_segment) {
        return Err(());
    }
    let descriptor: crate::overlay::creation_source::SourceDescriptor = intent
        .arguments
        .get("sourceDescriptors")
        .and_then(Value::as_array)
        .filter(|values| values.len() == 1)
        .and_then(|values| serde_json::from_value(values[0].clone()).ok())
        .ok_or(())?;
    if descriptor.path != image_path
        || crate::overlay::creation_source_snapshot::validate_sources(std::slice::from_ref(
            &descriptor,
        ))
        .is_err()
    {
        return Err(());
    }
    let display_path =
        crate::overlay::creation_source_snapshot::original_paths(std::slice::from_ref(&descriptor))
            .ok()
            .and_then(|paths| paths.into_iter().next())
            .unwrap_or_default();
    validate_directory(&staging_dir)?;
    validate_directory(&output_dir)?;
    let staging_path = crate::overlay::creation_output::assigned_path(&staging_dir, &output_name)
        .map_err(|_| ())?;
    crate::overlay::creation_output::validate_staging_path(
        &intent.dispatch_id,
        &output_name,
        &staging_path,
    )
    .map_err(|_| ())?;
    crate::overlay::creation_output::require_unoccupied(&output_dir, &output_name)
        .map_err(|_| ())?;
    super::super::asset_protocol::validate_generated(
        &previous_output_path.to_string_lossy(),
        &output_dir,
    )
    .map_err(|_| ())?;
    let continuation = Continuation {
        parent_dispatch_id,
        dispatch_id: intent.dispatch_id.clone(),
        image_path: image_path.clone(),
        source_descriptor: descriptor,
        output_dir,
        staging_dir,
        output_name,
        previous_output_path: previous_output_path.clone(),
        generation_mode,
        polycount,
        auto_segment,
        instruction: instruction.clone(),
        expires_at_ms: intent.expires_at_ms,
    };
    let status = JobStatus {
        job_id: Some(intent.job_id.clone()),
        stage: "queued".to_string(),
        progress_text: "Queued.".to_string(),
        phase: Some("queued".to_string()),
        elapsed_ms: Some(0),
        estimated_total_ms: None,
        progress_ratio: Some(0.0),
        timing_sample_count: None,
        output_path: Some(previous_output_path.to_string_lossy().to_string()),
        output_name: previous_output_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string()),
        source_image_path: Some(display_path),
        output_dir: Some(continuation.output_dir.to_string_lossy().to_string()),
        generation_mode: Some(generation_mode),
        polycount: Some(polycount),
        auto_segment: Some(auto_segment),
        instruction,
        is_segmented: false,
        can_segment: false,
        error: None,
        runtime_status: runtime_status_label(),
    };
    Ok((RuntimeOperation::Segment { continuation }, status))
}

fn verify(
    intent: &crate::overlay::creation_intent_journal::Intent,
    value: &Value,
) -> Result<(), ()> {
    crate::overlay::creation_intent_journal::verify_arguments(intent, value).map_err(|_| ())
}

fn validate_directory(path: &PathBuf) -> Result<(), ()> {
    crate::overlay::creation_intent_journal::validate_persisted_path(path).map_err(|_| ())?;
    (std::fs::canonicalize(path).ok().as_ref() == Some(path))
        .then_some(())
        .ok_or(())
}
