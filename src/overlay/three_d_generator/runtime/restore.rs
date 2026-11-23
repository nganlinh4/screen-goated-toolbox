use std::path::PathBuf;

mod retry;

use serde_json::Value;

use super::{
    Continuation, GenerationMode, JobStatus, RefineRequest, RuntimeOperation, STATE,
    StartJobRequest, runtime_status_label, schedule_next,
};

use crate::overlay::creation_recovery::saved::{RestoreError, validate_directory};

pub(in crate::overlay::three_d_generator) fn recovery_pending() -> bool {
    ensure_recovery_started();
    retry::pending()
}

pub(super) fn ensure_recovery_started() {
    if cfg!(test) || crate::overlay::creation_close::is_closing("3d") {
        return;
    }
    let Some(_admission) = super::admission::try_acquire() else {
        retry::schedule();
        return;
    };
    let Some(mut attempt) = retry::begin() else {
        if retry::pending() {
            retry::schedule();
        }
        return;
    };
    // Keep retrying even if the interface has no jobs to monitor.
    retry::schedule();
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
    let mut deferred = !pending_deliveries.is_empty();
    for intent in intents {
        if crate::overlay::creation_close::is_closing("3d") {
            deferred = true;
            break;
        }
        if pending_deliveries.contains(&intent.job_id) {
            continue;
        }
        // Admission and recovery share a gate through durable registration;
        // worker state remains available while either owner performs IO.
        match STATE.lock() {
            Ok(state) if state.jobs.contains_key(&intent.job_id) => continue,
            Err(_) => {
                deferred = true;
                continue;
            }
            _ => {}
        }
        if intent.deadline_at_ms <= super::now_ms() {
            let Ok(mut state) = STATE.lock() else {
                deferred = true;
                continue;
            };
            if crate::overlay::creation_close::is_closing("3d") {
                deferred = true;
                break;
            }
            if state.jobs.contains_key(&intent.job_id) {
                continue;
            }
            if crate::overlay::creation_intent_journal::clear_required("3d", &intent.job_id)
                .is_err()
            {
                deferred = true;
                continue;
            }
            state.insert_job(intent.job_id.clone(), expired_status(&intent.job_id));
            drop(state);
            if let Some(output_name) = intent.arguments.get("outputName").and_then(Value::as_str)
                && let Ok(path) =
                    crate::overlay::creation_output::staging_path(&intent.dispatch_id, output_name)
            {
                let _ = crate::overlay::creation_output::cleanup_staging(
                    &intent.dispatch_id,
                    output_name,
                    &path,
                );
            }
            continue;
        }
        let restored = if intent.arguments.get("parentDispatchId").is_some() {
            restore_segment(&intent)
        } else {
            restore_generation(&intent)
        };
        let (operation, status) = match restored {
            Ok(value) => value,
            Err(RestoreError::Invalid) => {
                if crate::overlay::creation_intent_journal::clear_required("3d", &intent.job_id)
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
        if let Ok(mut state) = STATE.lock() {
            if crate::overlay::creation_close::is_closing("3d") {
                deferred = true;
                break;
            }
            if state.jobs.contains_key(&intent.job_id) {
                continue;
            }
            state.insert_job(intent.job_id.clone(), status);
            state.operations.insert(intent.job_id.clone(), operation);
            state
                .request_fingerprints
                .insert(intent.job_id.clone(), intent.arguments_fingerprint);
            state
                .deadlines
                .insert(intent.job_id.clone(), intent.deadline_at_ms);
            state.recovered_jobs.insert(intent.job_id);
        } else {
            deferred = true;
        }
    }
    // Cleanup is independent of the outcome of each saved request. A transient
    // sweep error must not prevent an already-expired request from terminating.
    if let Err(error) = crate::overlay::creation_output::sweep_staging() {
        crate::log_info!("[3D Generator] Saved staging is unavailable: {error}");
        deferred = true;
    }
    if let Err(error) = crate::overlay::creation_source_snapshot::sweep() {
        crate::log_info!("[3D Generator] Saved sources are unavailable: {error}");
        deferred = true;
    }
    if !deferred {
        attempt.finish();
    }
    drop(attempt);
    schedule_next();
}

fn expired_status(job_id: &str) -> JobStatus {
    let mut status = super::idle_status();
    status.job_id = Some(job_id.to_string());
    status.stage = "failed".to_string();
    status.progress_text = "Creation was interrupted.".to_string();
    status.phase = Some("failed".to_string());
    status.error = Some("recovery_deadline".to_string());
    status
}

fn restore_generation(
    intent: &crate::overlay::creation_intent_journal::Intent,
) -> Result<(RuntimeOperation, JobStatus), RestoreError> {
    let mut request: StartJobRequest =
        serde_json::from_value(intent.arguments.clone()).map_err(|_| ())?;
    request.dispatch_id = intent.dispatch_id.clone();
    if !super::generation_mode::frozen_settings_valid(
        request.generation_mode,
        request.polycount,
        request.auto_segment,
    ) {
        return Err(RestoreError::Invalid);
    }
    verify(intent, &serde_json::to_value(&request).map_err(|_| ())?)?;
    if request.source_descriptors.len() != 1
        || request.source_descriptors[0].path != request.image_path
    {
        return Err(RestoreError::Invalid);
    }
    crate::overlay::creation_source_snapshot::validate_sources(&request.source_descriptors)
        .map_err(|_| RestoreError::Deferred)?;
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
    .map_err(|_| RestoreError::Deferred)?;
    crate::overlay::creation_output::require_unoccupied(&final_output_dir, &request.output_name)
        .map_err(|_| RestoreError::Deferred)?;
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
        download_path: None,
        download_name: None,
        source_image_path: Some(display_path),
        output_dir: Some(final_output_dir.to_string_lossy().to_string()),
        generation_mode: Some(request.generation_mode),
        polycount: Some(request.polycount),
        auto_segment: Some(request.auto_segment),
        segmentation_level: Some(request.segmentation_level.clone()),
        topology: request.topology.clone(),
        instruction: request.instruction.clone(),
        project_id: None,
        parent_revision_id: None,
        revision_kind: None,
        supported_actions: Vec::new(),
        available_actions: Vec::new(),
        is_textured: false,
        is_pbr: false,
        is_rigged: false,
        rig_type: None,
        can_refine: false,
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
) -> Result<(RuntimeOperation, JobStatus), RestoreError> {
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
        _ => return Err(RestoreError::Invalid),
    };
    let project_id = string("projectId")?;
    let available_actions = intent
        .arguments
        .get("availableActions")
        .and_then(Value::as_array)
        .ok_or(())?
        .iter()
        .map(|value| value.as_str().map(str::to_string).ok_or(()))
        .collect::<Result<Vec<_>, _>>()?;
    let supported_actions = match intent.arguments.get("supportedActions") {
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| value.as_str().map(str::to_string).ok_or(()))
            .collect::<Result<Vec<_>, _>>()?,
        None => available_actions.clone(),
        _ => return Err(RestoreError::Invalid),
    };
    let bool_value = |key: &str| intent.arguments.get(key).and_then(Value::as_bool).ok_or(());
    let is_segmented = bool_value("isSegmented")?;
    let is_textured = bool_value("isTextured")?;
    let is_pbr = bool_value("isPbr")?;
    let is_rigged = bool_value("isRigged")?;
    let rig_type = intent
        .arguments
        .get("rigType")
        .and_then(|value| match value {
            Value::Null => Some(None),
            Value::String(value) => Some(Some(value.clone())),
            _ => None,
        })
        .ok_or(())?;
    let refinement = if intent.arguments.get("kind").is_some() {
        let request: RefineRequest =
            serde_json::from_value(intent.arguments.clone()).map_err(|_| ())?;
        Some(request)
    } else {
        None
    };
    if !super::generation_mode::frozen_settings_valid(generation_mode, polycount, auto_segment) {
        return Err(RestoreError::Invalid);
    }
    let descriptor: crate::overlay::creation_source::SourceDescriptor = intent
        .arguments
        .get("sourceDescriptors")
        .and_then(Value::as_array)
        .filter(|values| values.len() == 1)
        .and_then(|values| serde_json::from_value(values[0].clone()).ok())
        .ok_or(())?;
    if descriptor.path != image_path {
        return Err(RestoreError::Invalid);
    }
    crate::overlay::creation_source_snapshot::validate_sources(std::slice::from_ref(&descriptor))
        .map_err(|_| RestoreError::Deferred)?;
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
    .map_err(|_| RestoreError::Deferred)?;
    crate::overlay::creation_output::require_unoccupied(&output_dir, &output_name)
        .map_err(|_| RestoreError::Deferred)?;
    super::super::asset_protocol::validate_generated(
        &previous_output_path.to_string_lossy(),
        &output_dir,
    )
    .map_err(|_| RestoreError::Deferred)?;
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
        segmentation_level: restored_segmentation_level(&intent.arguments)?,
        instruction: instruction.clone(),
        topology: intent
            .arguments
            .get("generationTopology")
            .and_then(Value::as_str)
            .map(str::to_string),
        project_id: project_id.clone(),
        supported_actions,
        available_actions,
        is_segmented,
        is_textured,
        is_pbr,
        is_rigged,
        rig_type: rig_type.clone(),
        refinement: refinement.clone(),
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
        download_path: None,
        download_name: None,
        source_image_path: Some(display_path),
        output_dir: Some(continuation.output_dir.to_string_lossy().to_string()),
        generation_mode: Some(generation_mode),
        polycount: Some(polycount),
        auto_segment: Some(auto_segment),
        segmentation_level: Some(continuation.segmentation_level.clone()),
        topology: continuation.topology.clone(),
        instruction,
        project_id: Some(project_id),
        parent_revision_id: None,
        revision_kind: refinement
            .as_ref()
            .map(|request| request.action().to_string()),
        supported_actions: Vec::new(),
        available_actions: Vec::new(),
        is_textured,
        is_pbr,
        is_rigged,
        rig_type,
        can_refine: false,
        is_segmented,
        can_segment: false,
        error: None,
        runtime_status: runtime_status_label(),
    };
    let operation = if refinement.is_some() {
        RuntimeOperation::Refine { continuation }
    } else {
        RuntimeOperation::Segment { continuation }
    };
    Ok((operation, status))
}

fn restored_segmentation_level(arguments: &Value) -> Result<String, ()> {
    let value = match arguments.get("generationSegmentationLevel") {
        Some(value) => value.as_str().ok_or(())?,
        None => match arguments.get("segmentationLevel") {
            None | Some(Value::Null) => "detailed",
            Some(value) => value.as_str().ok_or(())?,
        },
    };
    matches!(value, "simple" | "balanced" | "detailed")
        .then(|| value.to_string())
        .ok_or(())
}

fn verify(
    intent: &crate::overlay::creation_intent_journal::Intent,
    value: &Value,
) -> Result<(), ()> {
    crate::overlay::creation_intent_journal::verify_arguments(intent, value).map_err(|_| ())
}

#[cfg(test)]
#[path = "restore/settings_tests.rs"]
mod settings_tests;

#[cfg(test)]
mod recovery_gate_tests {
    use super::*;

    #[test]
    fn unavailable_directory_is_deferred_but_relative_path_is_invalid() {
        let missing = std::env::temp_dir().join(format!(
            "sgt-recovery-missing-{}-{}",
            std::process::id(),
            super::super::now_ms()
        ));
        assert!(!missing.exists());
        assert_eq!(validate_directory(&missing), Err(RestoreError::Deferred));
        assert_eq!(
            validate_directory(&PathBuf::from("relative")),
            Err(RestoreError::Invalid)
        );
    }

    #[test]
    fn malformed_request_is_structurally_invalid() {
        let intent = crate::overlay::creation_intent_journal::Intent {
            product: "3d".into(),
            job_id: "job".into(),
            dispatch_id: "dispatch".into(),
            created_at_ms: 0,
            accepted_at_ms: 0,
            deadline_at_ms: 1,
            expires_at_ms: 1,
            arguments: serde_json::json!({}),
            arguments_fingerprint: String::new(),
        };
        assert!(matches!(
            restore_generation(&intent),
            Err(RestoreError::Invalid)
        ));
    }

    #[test]
    fn expired_job_has_terminal_status_without_needing_files() {
        let status = expired_status("expired");
        assert_eq!(status.job_id.as_deref(), Some("expired"));
        assert_eq!(status.stage, "failed");
        assert_eq!(status.error.as_deref(), Some("recovery_deadline"));
        assert!(status.output_path.is_none());
    }
}
