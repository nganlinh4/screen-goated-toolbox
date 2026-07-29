use std::io::Write as _;
use std::path::PathBuf;
use std::process::Stdio;

use serde_json::{Value, json};

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
            fail(&job_id, &operation, "The 3D engine is unavailable.");
            return;
        }
    }
    let Some(mut command) = runtime_command() else {
        fail(&job_id, &operation, "The 3D engine is unavailable.");
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

fn sources_unchanged(operation: &RuntimeOperation) -> bool {
    match operation {
        RuntimeOperation::Generate { request, .. } => {
            request.source_descriptors.len() == 1
                && request.source_descriptors[0].path == request.image_path
                && crate::overlay::creation_source_snapshot::validate_sources(
                    &request.source_descriptors,
                )
                .is_ok()
        }
        RuntimeOperation::Segment { continuation } => {
            continuation.source_descriptor.path == continuation.image_path
                && crate::overlay::creation_source_snapshot::validate_sources(std::slice::from_ref(
                    &continuation.source_descriptor,
                ))
                .is_ok()
        }
    }
}

fn operation_name(operation: &RuntimeOperation) -> &'static str {
    match operation {
        RuntimeOperation::Generate { .. } => "generate",
        RuntimeOperation::Segment { .. } => "segment",
    }
}

fn request_value(operation: &RuntimeOperation) -> Value {
    match operation {
        RuntimeOperation::Generate { request, .. } => {
            let mut value = serde_json::to_value(request).unwrap_or(Value::Null);
            if let Some(arguments) = value.as_object_mut() {
                arguments.remove("finalOutputDir");
            }
            value
        }
        RuntimeOperation::Segment { continuation } => json!({
            "parentDispatchId": &continuation.parent_dispatch_id,
            "outputPath": &continuation.previous_output_path,
            "outputName": &continuation.output_name,
            "imagePath": &continuation.image_path,
            "sourceDescriptors": [&continuation.source_descriptor],
            "outputDir": &continuation.staging_dir,
            "previousOutputPath": &continuation.previous_output_path,
            "generationMode": continuation.generation_mode,
            "polycount": continuation.polycount,
            "autoSegment": continuation.auto_segment,
            "instruction": &continuation.instruction,
        }),
    }
}

fn fresh_message(job_id: &str, operation: &RuntimeOperation, fingerprint: &str) -> Value {
    let mut args = request_value(operation);
    if let Some(args) = args.as_object_mut() {
        args.insert("dispatchId".to_string(), json!(operation.dispatch_id()));
        args.insert("requestFingerprint".to_string(), json!(fingerprint));
    }
    json!({
        "id": job_id,
        "cmd": match operation {
            RuntimeOperation::Generate { .. } => "start_job",
            RuntimeOperation::Segment { .. } => "segment_job",
        },
        "args": args,
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
        RuntimeOperation::Segment { continuation } => {
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
    if job_cancelled(job_id) {
        let _ = crate::overlay::creation_output::cleanup_staging(
            operation.dispatch_id(),
            operation.output_name(),
            &staging_path,
        );
        return;
    }
    let Ok(output_path) = crate::overlay::creation_output::assigned_path(
        operation.final_output_dir(),
        operation.output_name(),
    ) else {
        fail(job_id, operation, "The model destination is invalid.");
        return;
    };
    let is_segmented = result
        .get("isSegmented")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let can_segment = result
        .get("canSegment")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut continuation = match operation {
        RuntimeOperation::Generate {
            request,
            final_output_dir,
            ..
        } if super::generation_mode::continuation_advertised(is_segmented, can_segment) => {
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
                expires_at_ms,
            })
        }
        _ => None,
    };
    if let Some(saved) = &continuation
        && crate::overlay::creation_source_snapshot::retain_continuation(
            source_descriptors,
            operation.dispatch_id(),
            job_id,
            saved.expires_at_ms,
        )
        .is_err()
    {
        continuation = None;
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
        source_image_path: Some(presentation_source.clone()),
        output_dir: Some(operation.final_output_dir().to_string_lossy().to_string()),
        generation_mode: Some(operation.generation_mode()),
        polycount: Some(operation.polycount()),
        auto_segment: Some(operation.auto_segment()),
        instruction: operation.instruction().map(str::to_string),
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
            metadata: json!({
                "isSegmented": status.is_segmented,
                "generationMode": status.generation_mode,
                "polycount": status.polycount,
                "autoSegment": status.auto_segment,
                "instruction": status.instruction,
                "outputDir": status.output_dir,
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

fn fail(job_id: &str, operation: &RuntimeOperation, _message: &str) {
    finish_failed(job_id, operation, false);
}

fn fail_retaining(job_id: &str, operation: &RuntimeOperation) {
    finish_failed(job_id, operation, true);
}

fn finish_failed(job_id: &str, operation: &RuntimeOperation, retain_intent: bool) {
    let status = JobStatus {
        job_id: Some(job_id.to_string()),
        stage: "failed".to_string(),
        progress_text: "Creation was interrupted.".to_string(),
        phase: Some("failed".to_string()),
        elapsed_ms: None,
        estimated_total_ms: None,
        progress_ratio: None,
        timing_sample_count: None,
        output_path: None,
        output_name: None,
        source_image_path: Some(operation.source_image_path().to_string()),
        output_dir: Some(operation.final_output_dir().to_string_lossy().to_string()),
        generation_mode: Some(operation.generation_mode()),
        polycount: Some(operation.polycount()),
        auto_segment: Some(operation.auto_segment()),
        instruction: operation.instruction().map(str::to_string),
        is_segmented: false,
        can_segment: false,
        error: Some("recovery_failed".to_string()),
        runtime_status: runtime_status_label(),
    };
    if retain_intent {
        super::process::finish_job_retaining_intent(job_id, status, None);
    } else {
        finish_job(job_id, status, None);
    }
}

#[cfg(test)]
mod tests {
    use super::{fresh_message, sources_unchanged};
    use crate::overlay::three_d_generator::runtime::{
        RuntimeOperation, StartJobRequest, generation_mode::GenerationMode,
    };
    use image::{ImageBuffer, Rgb};

    #[test]
    fn fresh_dispatch_uses_immutable_snapshot_when_original_changes() {
        let root = std::env::temp_dir().join(format!(
            "sgt-3d-source-freeze-{}-{}",
            std::process::id(),
            super::now_ms()
        ));
        std::fs::create_dir(&root).unwrap();
        let image_path = root.join("source.png");
        ImageBuffer::from_pixel(1, 1, Rgb([1_u8, 2, 3]))
            .save(&image_path)
            .unwrap();
        let inspected = crate::overlay::creation_source::inspect_image(&image_path).unwrap();
        let snapshot =
            crate::overlay::creation_source_snapshot::prepare("3d", "dispatch-1", &[inspected])
                .unwrap();
        let descriptor = snapshot.descriptors()[0].clone();
        snapshot.persist();
        let operation = RuntimeOperation::Generate {
            request: StartJobRequest {
                image_path: descriptor.path.clone(),
                source_descriptors: vec![descriptor.clone()],
                output_dir: Some(root.to_string_lossy().to_string()),
                final_output_dir: Some(root.to_string_lossy().to_string()),
                polycount: 5_000,
                mode: "topology_mesh".to_string(),
                generation_mode: GenerationMode::Quality,
                output_format: "glb_plain".to_string(),
                auto_segment: false,
                segmentation_mode: "none".to_string(),
                instruction: None,
                output_name: "result-dispatch-1.glb".to_string(),
                dispatch_id: "dispatch-1".to_string(),
            },
            output_dir: root.clone(),
            final_output_dir: root.clone(),
        };
        assert!(sources_unchanged(&operation));
        ImageBuffer::from_pixel(2, 1, Rgb([4_u8, 5, 6]))
            .save(&image_path)
            .unwrap();
        assert!(sources_unchanged(&operation));
        let message = fresh_message("job-1", &operation, "fingerprint-1");
        assert_eq!(
            message["args"]["sourceDescriptors"][0]["sha256"],
            descriptor.sha256
        );
        assert_eq!(message["args"]["requestFingerprint"], "fingerprint-1");
        crate::overlay::creation_source_snapshot::release_for_cleared_intent(
            &crate::overlay::creation_intent_journal::Intent {
                product: "3d".to_string(),
                job_id: "job-1".to_string(),
                dispatch_id: "dispatch-1".to_string(),
                created_at_ms: 1,
                accepted_at_ms: 1,
                deadline_at_ms: 1 + crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS,
                expires_at_ms: 2,
                arguments: serde_json::json!({"sourceDescriptors": [descriptor]}),
                arguments_fingerprint: "0".repeat(64),
            },
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
