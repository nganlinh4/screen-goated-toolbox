use std::path::PathBuf;

use super::{
    JobStatus, MAX_QUEUED_JOBS, RefineRequest, RefinementKind, RuntimeOperation, STATE,
    StartJobRequest, capabilities, default_output_dir, ensure_recovery_started, generation_mode,
    next_dispatch_id, next_job_id, now_ms, runtime_status_label, schedule_next,
};

pub(in crate::overlay::three_d_generator) fn start_job(
    mut request: StartJobRequest,
) -> Result<JobStatus, String> {
    crate::overlay::creation_close::ensure_accepting("3d")?;
    ensure_recovery_started();
    generation_mode::normalize_request(&mut request);
    capabilities::normalize_instruction(request.generation_mode, &mut request.instruction)?;
    if request.image_path.trim().is_empty() {
        return Err("Pick an image first.".to_string());
    }
    let inspected = crate::overlay::creation_source::inspect_image(&request.image_path)?;
    request.image_path = inspected.path.to_string_lossy().to_string();
    request.source_descriptors.clear();
    let source_bytes = inspected.size_bytes;
    let final_output_dir = match request
        .output_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(path) => PathBuf::from(path),
        None => default_output_dir()?,
    };
    std::fs::create_dir_all(&final_output_dir).map_err(|error| {
        format!(
            "Could not create output directory {}: {error}",
            final_output_dir.display()
        )
    })?;
    let final_output_dir = std::fs::canonicalize(&final_output_dir)
        .map_err(|error| format!("Could not use {}: {error}", final_output_dir.display()))?;
    crate::overlay::creation_intent_journal::validate_persisted_path(&final_output_dir)?;
    let job_id = next_job_id()?;
    request.dispatch_id = next_dispatch_id()?;
    request.output_name = crate::overlay::creation_output::assigned_name(
        &request.image_path,
        &request.dispatch_id,
        None,
        "glb",
    )?;
    crate::overlay::creation_output::require_unoccupied(&final_output_dir, &request.output_name)?;
    let staging = crate::overlay::creation_output::prepare_staging(
        &request.dispatch_id,
        &request.output_name,
    )?;
    request.output_dir = Some(staging.directory().to_string_lossy().to_string());
    request.final_output_dir = Some(final_output_dir.to_string_lossy().to_string());
    let status = queued_status(
        &job_id,
        QueuedStatusFields {
            source_image_path: Some(request.image_path.clone()),
            output_dir: Some(final_output_dir.to_string_lossy().to_string()),
            generation_mode: Some(request.generation_mode),
            polycount: Some(request.polycount),
            auto_segment: Some(request.auto_segment),
            instruction: request.instruction.clone(),
            output_path: None,
            output_name: None,
            runtime_status: runtime_status_label(),
        },
    );
    let mut state = STATE
        .lock()
        .map_err(|_| "3D generator state is unavailable".to_string())?;
    crate::overlay::creation_close::ensure_accepting("3d")?;
    if state.pending_count() >= MAX_QUEUED_JOBS {
        return Err("The model queue is full.".to_string());
    }
    let recorded = crate::overlay::generation_history::admit_and_record(
        "3d",
        &final_output_dir,
        source_bytes,
        1,
        || {
            let snapshot = crate::overlay::creation_source_snapshot::prepare(
                "3d",
                &request.dispatch_id,
                &[inspected],
            )?;
            request.image_path = snapshot.paths()[0].clone();
            request.source_descriptors = snapshot.descriptors().to_vec();
            let frozen = serde_json::to_value(&request)
                .map_err(|_| "Model request could not be saved.".to_string())?;
            let recorded = crate::overlay::creation_intent_journal::record(
                "3d",
                &job_id,
                &request.dispatch_id,
                frozen,
            )?;
            snapshot.persist();
            Ok(recorded)
        },
    )?;
    let staging_dir = staging.persist();
    state.insert_job(job_id.clone(), status.clone());
    state.operations.insert(
        job_id.clone(),
        RuntimeOperation::Generate {
            request,
            output_dir: staging_dir,
            final_output_dir,
        },
    );
    state
        .request_fingerprints
        .insert(job_id.clone(), recorded.arguments_fingerprint);
    state.deadlines.insert(job_id, recorded.deadline_at_ms);
    let active_demand = state.pending_count();
    drop(state);
    crate::overlay::creation_runtime::maintain_readiness_for_demand("3d", active_demand, false);
    schedule_next();
    Ok(status)
}

pub(in crate::overlay::three_d_generator) fn start_segmentation(
    continuation_id: &str,
) -> Result<JobStatus, String> {
    start_refinement(RefineRequest {
        continuation_id: continuation_id.to_string(),
        kind: RefinementKind::SeparateParts,
        segmentation_level: Some("balanced".to_string()),
        topology: None,
        face_limit: None,
        animation: None,
    })
}

pub(in crate::overlay::three_d_generator) fn start_refinement(
    request: RefineRequest,
) -> Result<JobStatus, String> {
    crate::overlay::creation_close::ensure_accepting("3d")?;
    ensure_recovery_started();
    if request.continuation_id.trim().is_empty() {
        return Err("The model continuation is missing.".to_string());
    }
    validate_refinement(&request)?;
    let (mut continuation, runtime_status) = {
        let mut state = STATE
            .lock()
            .map_err(|_| "3D generator state is unavailable")?;
        if state.pending_count() >= MAX_QUEUED_JOBS {
            return Err("The model queue is full.".to_string());
        }
        (
            state.peek_continuation(&request.continuation_id, now_ms())?,
            runtime_status_label(),
        )
    };
    if !continuation
        .available_actions
        .iter()
        .any(|action| action == request.action())
    {
        return Err("This refinement is not available for the current version.".to_string());
    }
    if continuation.source_descriptor.path != continuation.image_path
        || crate::overlay::creation_source_snapshot::validate_sources(std::slice::from_ref(
            &continuation.source_descriptor,
        ))
        .is_err()
        || !std::fs::canonicalize(&continuation.output_dir)
            .is_ok_and(|path| path == continuation.output_dir)
        || super::super::asset_protocol::validate_generated(
            &continuation.previous_output_path.to_string_lossy(),
            &continuation.output_dir,
        )
        .is_err()
    {
        return Err("This model can no longer be refined.".to_string());
    }
    let job_id = next_job_id()?;
    continuation.dispatch_id = next_dispatch_id()?;
    continuation.output_name = crate::overlay::creation_output::assigned_name(
        &continuation.image_path,
        &continuation.dispatch_id,
        Some(request.suffix()),
        "glb",
    )?;
    crate::overlay::creation_output::require_unoccupied(
        &continuation.output_dir,
        &continuation.output_name,
    )?;
    let staging = crate::overlay::creation_output::prepare_staging(
        &continuation.dispatch_id,
        &continuation.output_name,
    )?;
    continuation.staging_dir = staging.directory().to_path_buf();
    continuation.refinement = Some(request.clone());
    let status = queued_status(
        &job_id,
        QueuedStatusFields {
            source_image_path: Some(continuation.image_path.clone()),
            output_dir: Some(continuation.output_dir.to_string_lossy().to_string()),
            generation_mode: Some(continuation.generation_mode),
            polycount: Some(continuation.polycount),
            auto_segment: Some(continuation.auto_segment),
            instruction: continuation.instruction.clone(),
            output_path: Some(
                continuation
                    .previous_output_path
                    .to_string_lossy()
                    .to_string(),
            ),
            output_name: continuation
                .previous_output_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string()),
            runtime_status,
        },
    );
    let frozen = serde_json::json!({
        "continuationId": &request.continuation_id,
        "parentDispatchId": &continuation.parent_dispatch_id,
        "outputPath": &continuation.previous_output_path,
        "outputName": &continuation.output_name,
        "imagePath": &continuation.image_path,
        "sourceDescriptors": [&continuation.source_descriptor],
        "outputDir": &continuation.staging_dir,
        "finalOutputDir": &continuation.output_dir,
        "previousOutputPath": &continuation.previous_output_path,
        "generationMode": continuation.generation_mode,
        "polycount": continuation.polycount,
        "autoSegment": continuation.auto_segment,
        "instruction": &continuation.instruction,
        "projectId": &continuation.project_id,
        "supportedActions": &continuation.supported_actions,
        "availableActions": &continuation.available_actions,
        "isSegmented": continuation.is_segmented,
        "isTextured": continuation.is_textured,
        "isPbr": continuation.is_pbr,
        "isRigged": continuation.is_rigged,
        "rigType": &continuation.rig_type,
        "kind": request.kind,
        "segmentationLevel": request.segmentation_level,
        "topology": request.topology,
        "faceLimit": request.face_limit,
        "animation": request.animation,
    });
    let mut state = STATE
        .lock()
        .map_err(|_| "3D generator state is unavailable".to_string())?;
    crate::overlay::creation_close::ensure_accepting("3d")?;
    if state.pending_count() >= MAX_QUEUED_JOBS {
        return Err("The model queue is full.".to_string());
    }
    let current = state.peek_continuation(&request.continuation_id, now_ms())?;
    if current.parent_dispatch_id != continuation.parent_dispatch_id
        || current.image_path != continuation.image_path
        || current.previous_output_path != continuation.previous_output_path
        || current.generation_mode != continuation.generation_mode
        || current.polycount != continuation.polycount
        || current.auto_segment != continuation.auto_segment
        || current.instruction != continuation.instruction
    {
        return Err("This model can no longer be refined.".to_string());
    }
    let (source_bytes, source_count) = retained_source_reservation(&continuation);
    let recorded = crate::overlay::generation_history::admit_and_record(
        "3d",
        &continuation.output_dir,
        source_bytes,
        source_count,
        || {
            let recorded = crate::overlay::creation_intent_journal::record(
                "3d",
                &job_id,
                &continuation.dispatch_id,
                frozen,
            )?;
            if let Err(error) = crate::overlay::creation_source_snapshot::claim_intent(
                std::slice::from_ref(&continuation.source_descriptor),
                &continuation.dispatch_id,
            ) {
                crate::overlay::creation_intent_journal::clear("3d", &job_id);
                return Err(error);
            }
            Ok(recorded)
        },
    )?;
    let staging_dir = staging.persist();
    let Ok(mut consumed) = state.take_continuation(&request.continuation_id, now_ms()) else {
        crate::overlay::creation_intent_journal::clear("3d", &job_id);
        return Err("This model can no longer be refined.".to_string());
    };
    consumed.dispatch_id = continuation.dispatch_id;
    consumed.output_name = continuation.output_name;
    consumed.staging_dir = staging_dir;
    consumed.refinement = Some(request.clone());
    let _ = crate::overlay::creation_source_snapshot::release_continuation(
        std::slice::from_ref(&consumed.source_descriptor),
        &request.continuation_id,
    );
    state.insert_job(job_id.clone(), status.clone());
    state.operations.insert(
        job_id.clone(),
        RuntimeOperation::Refine {
            continuation: consumed,
        },
    );
    state
        .request_fingerprints
        .insert(job_id.clone(), recorded.arguments_fingerprint);
    state.deadlines.insert(job_id, recorded.deadline_at_ms);
    drop(state);
    schedule_next();
    Ok(status)
}

fn validate_refinement(request: &RefineRequest) -> Result<(), String> {
    match request.kind {
        RefinementKind::SeparateParts => match request.segmentation_level.as_deref() {
            None | Some("simple" | "balanced" | "detailed") => Ok(()),
            _ => Err("Choose a supported separation level.".to_string()),
        },
        RefinementKind::OptimizeMesh => {
            if !matches!(
                request.topology.as_deref(),
                None | Some("triangle" | "quad")
            ) || !request
                .face_limit
                .is_none_or(|value| (100..=50_000).contains(&value))
            {
                return Err("Choose supported mesh optimization settings.".to_string());
            }
            Ok(())
        }
        RefinementKind::Animate => match request.animation.as_deref() {
            None | Some("idle" | "walk" | "run" | "jump" | "wave_goodbye_01") => Ok(()),
            _ => Err("Choose a supported animation.".to_string()),
        },
        RefinementKind::AddMaterials | RefinementKind::GeneratePbr | RefinementKind::Rig => Ok(()),
    }
}

pub(super) fn retained_source_reservation(continuation: &super::Continuation) -> (u64, usize) {
    (continuation.source_descriptor.size_bytes, 1)
}

struct QueuedStatusFields {
    source_image_path: Option<String>,
    output_dir: Option<String>,
    generation_mode: Option<super::GenerationMode>,
    polycount: Option<u32>,
    auto_segment: Option<bool>,
    instruction: Option<String>,
    output_path: Option<String>,
    output_name: Option<String>,
    runtime_status: String,
}

fn queued_status(job_id: &str, fields: QueuedStatusFields) -> JobStatus {
    JobStatus {
        job_id: Some(job_id.to_string()),
        stage: "queued".to_string(),
        progress_text: "Queued.".to_string(),
        phase: Some("queued".to_string()),
        elapsed_ms: Some(0),
        estimated_total_ms: None,
        progress_ratio: Some(0.0),
        timing_sample_count: None,
        output_path: fields.output_path,
        output_name: fields.output_name,
        download_path: None,
        download_name: None,
        source_image_path: fields.source_image_path,
        output_dir: fields.output_dir,
        generation_mode: fields.generation_mode,
        polycount: fields.polycount,
        auto_segment: fields.auto_segment,
        instruction: fields.instruction,
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
        runtime_status: fields.runtime_status,
    }
}
