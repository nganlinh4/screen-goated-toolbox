use std::io::Write as _;
use std::path::PathBuf;
use std::process::Stdio;

use serde_json::{Value, json};

use super::failure::{fail, fail_retaining};
use super::message::{fresh_message, operation_name, request_value, sources_unchanged};
use super::process::{CommandNoWindowExt as _, finish_job, update_progress};
use super::{
    Continuation, JobStatus, RuntimeOperation, STATE, continuation_expiry, now_ms, prepare_runtime,
    runtime_command, runtime_status_label,
};

pub(super) fn run_stdio_operation(
    job_id: String,
    operation: RuntimeOperation,
    request_fingerprint: String,
    deadline_at_ms: u64,
    query_recovery: bool,
) {
    run_stdio_operation_attempt(
        job_id,
        operation,
        request_fingerprint,
        deadline_at_ms,
        query_recovery,
        true,
    );
}

fn run_stdio_operation_attempt(
    job_id: String,
    operation: RuntimeOperation,
    request_fingerprint: String,
    deadline_at_ms: u64,
    query_recovery: bool,
    allow_runtime_update: bool,
) {
    if !sources_unchanged(&operation) {
        fail(
            &job_id,
            &operation,
            "The source image changed after it was selected.",
        );
        crate::overlay::creation_intent_journal::clear("3d", &job_id);
        return;
    }
    if runtime_command().is_none() {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        if crate::overlay::creation_runtime::download_runtime(stop, true).is_err() {
            if allow_runtime_update
                && crate::overlay::creation_runtime::refresh_after_start_failure()
            {
                run_stdio_operation_attempt(
                    job_id,
                    operation,
                    request_fingerprint,
                    deadline_at_ms,
                    query_recovery,
                    false,
                );
                return;
            }
            fail(&job_id, &operation, "The 3D engine is unavailable.");
            return;
        }
    }
    let Some(mut command) = runtime_command() else {
        fail(&job_id, &operation, "The 3D engine is unavailable.");
        return;
    };
    let Ok(_component_lease) = crate::component_registry::acquire("creation-windows") else {
        fail(&job_id, &operation, "The 3D engine is being removed.");
        return;
    };
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags_windows();
    let child = {
        let Ok(mut state) = STATE.lock() else {
            return;
        };
        if state
            .jobs
            .get(&job_id)
            .is_none_or(|status| super::status_is_non_publishable(&status.stage))
        {
            return;
        }
        let child = command.spawn();
        if let Ok(child) = &child {
            state.pids.insert(job_id.clone(), child.id());
        }
        child
    };
    let Ok(mut child) = child else {
        if allow_runtime_update && crate::overlay::creation_runtime::refresh_after_start_failure() {
            run_stdio_operation_attempt(
                job_id,
                operation,
                request_fingerprint,
                deadline_at_ms,
                query_recovery,
                false,
            );
            return;
        }
        fail(&job_id, &operation, "The 3D engine could not start.");
        return;
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        crate::overlay::creation_recovery::terminate_process_tree(child.id());
        fail(&job_id, &operation, "The 3D engine could not start.");
        return;
    };
    let Ok(mut events) =
        crate::overlay::creation_process_supervisor::EventSupervisor::with_deadline(
            stdout,
            deadline_at_ms,
        )
    else {
        crate::overlay::creation_recovery::terminate_process_tree(child.id());
        fail(&job_id, &operation, "Creation exceeded its time limit.");
        return;
    };
    let identity = crate::overlay::creation_recovery::Identity {
        tool: "3d",
        operation: operation_name(&operation),
        dispatch_id: operation.dispatch_id(),
        request_fingerprint: &request_fingerprint,
    };
    let state = if query_recovery {
        let query_id = format!("{job_id}-query");
        if writeln!(
            stdin,
            "{}",
            crate::overlay::creation_recovery::query_message(&query_id, &identity)
        )
        .is_err()
        {
            crate::overlay::creation_recovery::terminate_process_tree(child.id());
            fail(&job_id, &operation, "Creation recovery could not start.");
            return;
        }
        let response = match events.next(child.id(), || job_cancelled(&job_id)) {
            Ok(Some(value)) => value,
            _ => {
                crate::overlay::creation_recovery::terminate_process_tree(child.id());
                fail(&job_id, &operation, "Creation recovery returned no status.");
                return;
            }
        };
        match crate::overlay::creation_recovery::parse_query_response(&response, &identity) {
            Ok(state) => state,
            Err(error) => {
                crate::overlay::creation_recovery::terminate_process_tree(child.id());
                fail(&job_id, &operation, &error);
                return;
            }
        }
    } else {
        crate::overlay::creation_recovery::State::None
    };
    let request = request_value(&operation);
    let message = if state == crate::overlay::creation_recovery::State::None {
        fresh_message(&job_id, &operation, &request_fingerprint)
    } else {
        crate::overlay::creation_recovery::resume_message(&job_id, &identity, request)
    };
    if writeln!(stdin, "{message}").is_err() {
        crate::overlay::creation_recovery::terminate_process_tree(child.id());
        fail(&job_id, &operation, "Creation recovery could not continue.");
        return;
    }
    drop(stdin);
    let mut accepted = state != crate::overlay::creation_recovery::State::None;
    let runtime_status = runtime_status_label();
    loop {
        let value = match events.next(child.id(), || job_cancelled(&job_id)) {
            Ok(Some(value)) => value,
            Ok(None) => break,
            Err(_) => {
                crate::overlay::creation_recovery::terminate_process_tree(child.id());
                break;
            }
        };
        if matches!(
            value.get("acceptanceState").and_then(Value::as_str),
            Some("accepted" | "recovering" | "completed")
        ) {
            accepted = true;
        }
        if value.get("event").and_then(Value::as_str) == Some("progress") {
            update_progress(&job_id, &value, &runtime_status);
        } else if value.get("ok").and_then(Value::as_bool) == Some(true) {
            finish_success(
                &job_id,
                &operation,
                &request_fingerprint,
                value.get("result").cloned().unwrap_or(Value::Null),
                &runtime_status,
            );
            let _ = child.wait();
            let _ = prepare_runtime();
            return;
        } else if value.get("ok").and_then(Value::as_bool) == Some(false) {
            let recovery_not_found =
                value.get("error").and_then(Value::as_str) == Some("creation.recovery_not_found");
            if !recovery_not_found {
                crate::overlay::creation_intent_journal::clear("3d", &job_id);
            }
            if query_recovery && recovery_not_found {
                fail_retaining(&job_id, &operation);
            } else {
                fail(&job_id, &operation, "Creation failed. Try again.");
            }
            let _ = child.wait();
            return;
        }
    }
    let _ = child.wait();
    if accepted {
        fail_retaining(&job_id, &operation);
    } else {
        fail(&job_id, &operation, "Creation was interrupted. Try again.");
    }
}

fn job_cancelled(job_id: &str) -> bool {
    STATE.lock().is_ok_and(|state| {
        state
            .jobs
            .get(job_id)
            .is_none_or(|status| super::status_is_non_publishable(&status.stage))
    })
}

fn finish_success(
    job_id: &str,
    operation: &RuntimeOperation,
    request_fingerprint: &str,
    result: Value,
    runtime_status: &str,
) {
    let source_descriptors = match operation {
        RuntimeOperation::Generate { request, .. } => request.source_descriptors.as_slice(),
        RuntimeOperation::Segment { continuation } | RuntimeOperation::Refine { continuation } => {
            std::slice::from_ref(&continuation.source_descriptor)
        }
    };
    let Ok(presentation_source) =
        crate::overlay::creation_source_snapshot::presentation_path(source_descriptors)
    else {
        fail(
            job_id,
            operation,
            "The model preview could not be prepared.",
        );
        return;
    };
    if presentation_source.is_empty() {
        fail(
            job_id,
            operation,
            "The model preview could not be prepared.",
        );
        return;
    }
    let project_thumbnail =
        crate::overlay::creation_preview::project_thumbnail_data_url(&presentation_source).ok();
    let staging_path = result
        .get("outputPath")
        .and_then(Value::as_str)
        .and_then(|path| {
            super::super::asset_protocol::validate_generated_exact(
                path,
                operation.output_dir(),
                operation.output_name(),
            )
            .ok()
        });
    let Some(staging_path) = staging_path else {
        fail(job_id, operation, "The model result is invalid.");
        return;
    };
    let Ok(output_path) = crate::overlay::creation_output::assigned_path(
        operation.final_output_dir(),
        operation.output_name(),
    ) else {
        fail(job_id, operation, "The model destination is invalid.");
        return;
    };
    let companion =
        match super::companion::from_result(&result, operation, &staging_path, &output_path) {
            Ok(companion) => companion,
            Err(error) => {
                fail(job_id, operation, &error);
                return;
            }
        };
    if job_cancelled(job_id) {
        if let Some(companion) = &companion {
            companion.cleanup_staging();
        }
        let _ = crate::overlay::creation_output::cleanup_staging(
            operation.dispatch_id(),
            operation.output_name(),
            &staging_path,
        );
        return;
    }
    let is_segmented = result
        .get("isSegmented")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let can_segment = result
        .get("canSegment")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let available_actions = result
        .get("availableActions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut supported_actions = result
        .get("supportedActions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if result.get("supportedActions").is_none() {
        supported_actions.clone_from(&available_actions);
    }
    let can_refine = result
        .get("canRefine")
        .and_then(Value::as_bool)
        .unwrap_or(can_segment || !available_actions.is_empty());
    let is_textured = result
        .get("isTextured")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_pbr = result
        .get("isPbr")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_rigged = result
        .get("isRigged")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let rig_type = result
        .get("rigType")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut continuation = match operation {
        RuntimeOperation::Generate {
            request,
            final_output_dir,
            ..
        } if super::generation_mode::continuation_advertised(is_segmented, can_refine) => {
            continuation_expiry(
                result
                    .get("continuationExpiresAtMs")
                    .and_then(Value::as_u64),
                now_ms(),
            )
            .map(|expires_at_ms| Continuation {
                parent_dispatch_id: request.dispatch_id.clone(),
                dispatch_id: String::new(),
                image_path: request.image_path.clone(),
                source_descriptor: request.source_descriptors[0].clone(),
                output_dir: final_output_dir.clone(),
                staging_dir: PathBuf::new(),
                output_name: String::new(),
                previous_output_path: output_path.clone(),
                generation_mode: request.generation_mode,
                polycount: request.polycount,
                auto_segment: request.auto_segment,
                instruction: request.instruction.clone(),
                project_id: job_id.to_string(),
                supported_actions: supported_actions.clone(),
                available_actions: available_actions.clone(),
                is_segmented,
                is_textured,
                is_pbr,
                is_rigged,
                rig_type: rig_type.clone(),
                refinement: None,
                expires_at_ms,
            })
        }
        RuntimeOperation::Refine { continuation } if can_refine => continuation_expiry(
            result
                .get("continuationExpiresAtMs")
                .and_then(Value::as_u64),
            now_ms(),
        )
        .map(|expires_at_ms| Continuation {
            parent_dispatch_id: continuation.dispatch_id.clone(),
            dispatch_id: String::new(),
            image_path: continuation.image_path.clone(),
            source_descriptor: continuation.source_descriptor.clone(),
            output_dir: continuation.output_dir.clone(),
            staging_dir: PathBuf::new(),
            output_name: String::new(),
            previous_output_path: output_path.clone(),
            generation_mode: continuation.generation_mode,
            polycount: continuation.polycount,
            auto_segment: continuation.auto_segment,
            instruction: continuation.instruction.clone(),
            project_id: continuation.project_id.clone(),
            supported_actions: supported_actions.clone(),
            available_actions: available_actions.clone(),
            is_segmented,
            is_textured,
            is_pbr,
            is_rigged,
            rig_type: rig_type.clone(),
            refinement: None,
            expires_at_ms,
        }),
        _ => None,
    };
    if let Some(saved) = &continuation
        && let Err(error) = crate::overlay::creation_source_snapshot::retain_continuation(
            source_descriptors,
            operation.dispatch_id(),
            job_id,
            saved.expires_at_ms,
        )
    {
        eprintln!("[3d-generator] continuation retention failed: {error}");
        continuation = None;
    } else if can_refine && continuation.is_none() {
        eprintln!("[3d-generator] continuation expiry was not accepted");
    }
    let status = JobStatus {
        job_id: Some(job_id.to_string()),
        stage: "done".to_string(),
        progress_text: if is_segmented {
            "Parts ready"
        } else {
            "Model ready"
        }
        .to_string(),
        phase: Some("complete".to_string()),
        elapsed_ms: None,
        estimated_total_ms: None,
        progress_ratio: Some(1.0),
        timing_sample_count: None,
        output_name: output_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string()),
        output_path: Some(output_path.to_string_lossy().to_string()),
        download_path: companion
            .as_ref()
            .map(|companion| companion.final_path.to_string_lossy().to_string()),
        download_name: companion
            .as_ref()
            .map(|companion| companion.final_name.clone()),
        source_image_path: Some(presentation_source.clone()),
        output_dir: Some(operation.final_output_dir().to_string_lossy().to_string()),
        generation_mode: Some(operation.generation_mode()),
        polycount: Some(operation.polycount()),
        auto_segment: Some(operation.auto_segment()),
        instruction: operation.instruction().map(str::to_string),
        project_id: Some(match operation {
            RuntimeOperation::Generate { .. } => job_id.to_string(),
            RuntimeOperation::Segment { continuation }
            | RuntimeOperation::Refine { continuation } => continuation.project_id.clone(),
        }),
        parent_revision_id: match operation {
            RuntimeOperation::Refine { continuation } => continuation
                .refinement
                .as_ref()
                .map(|request| request.continuation_id.clone()),
            _ => None,
        },
        revision_kind: Some(match operation {
            RuntimeOperation::Generate { .. } => "generation".to_string(),
            RuntimeOperation::Segment { .. } => "separate_parts".to_string(),
            RuntimeOperation::Refine { continuation } => continuation
                .refinement
                .as_ref()
                .map(|request| request.action().to_string())
                .unwrap_or_else(|| "refinement".to_string()),
        }),
        supported_actions: if continuation.is_some() {
            supported_actions
        } else {
            Vec::new()
        },
        available_actions: if continuation.is_some() {
            available_actions
        } else {
            Vec::new()
        },
        is_textured,
        is_pbr,
        is_rigged,
        rig_type,
        can_refine: continuation.is_some(),
        is_segmented,
        can_segment: can_segment && continuation.is_some(),
        error: None,
        runtime_status: runtime_status.to_string(),
    };
    let delivery = crate::overlay::creation_delivery::commit(
        crate::overlay::creation_delivery::PublishedDelivery {
            product: "3d",
            job_id: job_id.to_string(),
            dispatch_id: operation.dispatch_id().to_string(),
            request_fingerprint: request_fingerprint.to_string(),
            source_path: presentation_source,
            output_name: operation.output_name().to_string(),
            staging_path: staging_path.to_string_lossy().to_string(),
            output_path: output_path.to_string_lossy().to_string(),
            companion: companion.map(|companion| companion.delivery),
            metadata: json!({
                "isSegmented": status.is_segmented,
                "generationMode": status.generation_mode,
                "polycount": status.polycount,
                "autoSegment": status.auto_segment,
                "instruction": status.instruction,
                "outputDir": status.output_dir,
                "projectId": status.project_id,
                "parentRevisionId": status.parent_revision_id,
                "revisionKind": status.revision_kind,
                "supportedActions": status.supported_actions,
                "availableActions": status.available_actions,
                "isTextured": status.is_textured,
                "isPbr": status.is_pbr,
                "isRigged": status.is_rigged,
                "rigType": status.rig_type,
                "projectThumbnail": project_thumbnail,
            }),
        },
    );
    match delivery {
        Ok(()) => finish_job(job_id, status, continuation),
        Err(error) => {
            crate::log_info!("[3D Generator] Completion is still pending: {error}");
            super::process::finish_job_pending(job_id, status, continuation);
        }
    }
}

#[cfg(test)]
mod tests;
