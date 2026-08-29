use std::process::Command;

use serde_json::Value;

use super::{Continuation, JobStatus, RuntimeOperation, STATE};

fn normalize_progress_stage(value: &str) -> &'static str {
    match value {
        "queued" => "queued",
        "preparing" => "preparing",
        "generating" => "generating",
        "segmenting" => "segmenting",
        "refining" => "refining",
        "finalizing" => "finalizing",
        _ => "generating",
    }
}

fn progress_text(stage: &str) -> &'static str {
    match stage {
        "queued" => "Waiting to create.",
        "preparing" => "Preparing creation.",
        "segmenting" => "Separating model parts.",
        "refining" => "Creating a new version.",
        "finalizing" => "Finishing model.",
        _ => "Creating model.",
    }
}

fn normalize_phase(value: &str) -> Option<&'static str> {
    match value {
        "preparing" => Some("preparing"),
        "input" => Some("input"),
        "generation" => Some("generation"),
        "geometry" => Some("geometry"),
        "separation" => Some("separation"),
        "refinement" => Some("refinement"),
        _ => None,
    }
}

pub(super) fn update_progress(job_id: &str, value: &Value, runtime_status: &str) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    let Some(current) = state.jobs.get_mut(job_id) else {
        return;
    };
    if super::status_is_non_publishable(&current.stage) {
        return;
    }
    apply_progress(current, value, runtime_status);
}

fn apply_progress(current: &mut JobStatus, value: &Value, runtime_status: &str) {
    if let Some(stage) = value.get("stage").and_then(Value::as_str) {
        current.stage = normalize_progress_stage(stage).to_string();
        current.progress_text = progress_text(&current.stage).to_string();
    }
    if let Some(phase) = value.get("phase").and_then(Value::as_str) {
        current.phase = normalize_phase(phase).map(str::to_string);
    }
    if let Some(elapsed_ms) = crate::overlay::creation_progress::elapsed_ms(value) {
        current.elapsed_ms = Some(elapsed_ms);
    }
    if let Some(estimated_total_ms) = crate::overlay::creation_progress::estimated_total_ms(value) {
        current.estimated_total_ms = Some(estimated_total_ms);
    }
    if let Some(progress_ratio) = crate::overlay::creation_progress::ratio(value) {
        current.progress_ratio = Some(progress_ratio);
    }
    if let Some(timing_sample_count) = crate::overlay::creation_progress::timing_sample_count(value)
    {
        current.timing_sample_count = Some(timing_sample_count);
    }
    current.runtime_status = runtime_status.to_string();
}

pub(super) fn finish_job(job_id: &str, status: JobStatus, continuation: Option<Continuation>) {
    finish_job_with_intent(job_id, status, continuation, true);
}

pub(super) fn finish_job_retaining_intent(
    job_id: &str,
    status: JobStatus,
    continuation: Option<Continuation>,
) {
    finish_job_with_intent(job_id, status, continuation, false);
}

pub(super) fn finish_job_pending(
    job_id: &str,
    mut completed: JobStatus,
    continuation: Option<Continuation>,
) {
    if let Ok(mut state) = STATE.lock() {
        if state
            .jobs
            .get(job_id)
            .is_none_or(|item| super::status_is_non_publishable(&item.stage))
        {
            return;
        }
        let mut pending = completed.clone();
        pending.stage = "finalizing".to_string();
        pending.progress_text = "Finishing model.".to_string();
        pending.phase = Some("finalizing".to_string());
        pending.progress_ratio = pending.progress_ratio.map(|ratio| ratio.min(0.99));
        pending.output_path = None;
        pending.output_name = None;
        pending.can_segment = false;
        pending.can_refine = false;
        pending.supported_actions.clear();
        pending.available_actions.clear();
        completed.stage = "done".to_string();
        state.jobs.insert(job_id.to_string(), pending);
        state
            .pending_completions
            .insert(job_id.to_string(), (completed, continuation));
        state.pids.remove(job_id);
    }
    super::schedule_next();
}

fn finish_job_with_intent(
    job_id: &str,
    status: JobStatus,
    continuation: Option<Continuation>,
    clear_terminal: bool,
) {
    let terminal = matches!(status.stage.as_str(), "done" | "failed" | "cancelled");
    if let Ok(mut state) = STATE.lock() {
        if state.jobs.get(job_id).is_none_or(|item| {
            item.stage == "cancelled" || (item.stage == "cancelling" && status.stage != "done")
        }) {
            return;
        }
        state.jobs.insert(job_id.to_string(), status);
        if let Some(continuation) = continuation {
            state.continuations.insert(job_id.to_string(), continuation);
        }
        state.pids.remove(job_id);
        state.operations.remove(job_id);
        state.request_fingerprints.remove(job_id);
        state.deadlines.remove(job_id);
        state.recovered_jobs.remove(job_id);
        state.prune_terminal_jobs();
    }
    if terminal && clear_terminal {
        crate::overlay::creation_intent_journal::clear("3d", job_id);
    }
    super::schedule_next();
}

pub(super) fn run_runtime_operation(
    job_id: String,
    operation: RuntimeOperation,
    request_fingerprint: String,
    deadline_at_ms: u64,
    recovered: bool,
) {
    super::recovery::run_stdio_operation(
        job_id,
        operation,
        request_fingerprint,
        deadline_at_ms,
        recovered,
    );
}

pub(super) trait CommandNoWindowExt {
    fn creation_flags_windows(&mut self) -> &mut Self;
}

impl CommandNoWindowExt for Command {
    fn creation_flags_windows(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(0x08000000);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{apply_progress, normalize_progress_stage, progress_text};
    use crate::overlay::three_d_generator::runtime::JobStatus;

    #[test]
    fn inbound_runtime_state_is_reduced_to_the_public_contract() {
        assert_eq!(normalize_progress_stage("generating"), "generating");
        assert_eq!(normalize_progress_stage("unexpected-detail"), "generating");
        assert_eq!(normalize_progress_stage("done"), "generating");
        assert_eq!(progress_text("generating"), "Creating model.");
    }

    #[test]
    fn progress_cannot_publish_unvalidated_result_fields() {
        let mut status = JobStatus {
            job_id: Some("job".to_string()),
            stage: "generating".to_string(),
            progress_text: "Creating model.".to_string(),
            phase: None,
            elapsed_ms: None,
            estimated_total_ms: None,
            progress_ratio: None,
            timing_sample_count: None,
            output_path: None,
            output_name: None,
            download_path: None,
            download_name: None,
            source_image_path: Some("source.png".to_string()),
            output_dir: Some("output".to_string()),
            generation_mode: None,
            polycount: None,
            auto_segment: None,
            instruction: None,
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
            runtime_status: "installed".to_string(),
        };
        apply_progress(
            &mut status,
            &json!({
                "stage": "done",
                "phase": "complete",
                "elapsedMs": u64::MAX,
                "estimatedTotalMs": u64::MAX,
                "progressRatio": 9.0,
                "timingSampleCount": u64::MAX,
                "outputPath": "unvalidated.glb",
                "outputName": "unvalidated.glb",
                "isSegmented": true,
                "canSegment": true,
            }),
            "installed",
        );
        assert_eq!(status.stage, "generating");
        assert_eq!(status.phase, None);
        assert_eq!(status.elapsed_ms, Some(7_200_000));
        assert_eq!(status.estimated_total_ms, Some(7_200_000));
        assert_eq!(status.progress_ratio, Some(1.0));
        assert_eq!(status.timing_sample_count, Some(100_000));
        assert_eq!(status.output_path, None);
        assert_eq!(status.output_name, None);
        assert!(!status.is_segmented);
        assert!(!status.can_segment);
    }
}
