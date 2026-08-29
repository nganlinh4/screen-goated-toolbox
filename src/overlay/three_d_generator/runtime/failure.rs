use super::process::{finish_job, finish_job_retaining_intent};
use super::{JobStatus, RuntimeOperation, runtime_status_label};

pub(super) fn fail(job_id: &str, operation: &RuntimeOperation, _message: &str) {
    finish_failed(job_id, operation, false);
}

pub(super) fn fail_retaining(job_id: &str, operation: &RuntimeOperation) {
    finish_failed(job_id, operation, true);
}

fn finish_failed(job_id: &str, operation: &RuntimeOperation, retain_intent: bool) {
    let (output_path, output_name) = failure_output(operation);
    let status = JobStatus {
        job_id: Some(job_id.to_string()),
        stage: "failed".to_string(),
        progress_text: "Creation was interrupted.".to_string(),
        phase: Some("failed".to_string()),
        elapsed_ms: None,
        estimated_total_ms: None,
        progress_ratio: None,
        timing_sample_count: None,
        output_path,
        output_name,
        download_path: None,
        download_name: None,
        source_image_path: Some(operation.source_image_path().to_string()),
        output_dir: Some(operation.final_output_dir().to_string_lossy().to_string()),
        generation_mode: Some(operation.generation_mode()),
        polycount: Some(operation.polycount()),
        auto_segment: Some(operation.auto_segment()),
        instruction: operation.instruction().map(str::to_string),
        project_id: match operation {
            RuntimeOperation::Generate { .. } => None,
            RuntimeOperation::Segment { continuation }
            | RuntimeOperation::Refine { continuation } => Some(continuation.project_id.clone()),
        },
        parent_revision_id: match operation {
            RuntimeOperation::Refine { continuation } => continuation
                .refinement
                .as_ref()
                .map(|request| request.continuation_id.clone()),
            _ => None,
        },
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
        error: Some("recovery_failed".to_string()),
        runtime_status: runtime_status_label(),
    };
    if retain_intent {
        finish_job_retaining_intent(job_id, status, None);
    } else {
        finish_job(job_id, status, None);
    }
}

pub(super) fn failure_output(operation: &RuntimeOperation) -> (Option<String>, Option<String>) {
    match operation {
        RuntimeOperation::Segment { continuation } | RuntimeOperation::Refine { continuation } => (
            Some(
                continuation
                    .previous_output_path
                    .to_string_lossy()
                    .to_string(),
            ),
            continuation
                .previous_output_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string()),
        ),
        RuntimeOperation::Generate { .. } => (None, None),
    }
}
