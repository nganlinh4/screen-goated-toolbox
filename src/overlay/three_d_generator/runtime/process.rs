use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use serde_json::Value;

use super::super::depth_model;
use super::{
    Continuation, JobStatus, RuntimeOperation, STATE, job_status, prepare_runtime, runtime_command,
    runtime_status_label,
};
use crate::overlay::creation_runtime;

fn command_for_operation(operation: &RuntimeOperation) -> Option<Command> {
    let mut command = runtime_command()?;
    match operation {
        RuntimeOperation::Generate {
            request,
            output_dir,
        } => {
            command
                .arg("--job")
                .arg("--image")
                .arg(&request.image_path)
                .arg("--output-dir")
                .arg(output_dir)
                .arg("--polycount")
                .arg(request.polycount.to_string());
            if request.auto_segment {
                command.arg("--auto-segment");
            }
        }
        RuntimeOperation::Segment { continuation } => {
            command
                .arg("--segment-job")
                .arg("--task-id")
                .arg(&continuation.task_id)
                .arg("--profile-dir")
                .arg(&continuation.profile_dir)
                .arg("--image")
                .arg(&continuation.image_path)
                .arg("--output-dir")
                .arg(&continuation.output_dir)
                .arg("--previous-output")
                .arg(&continuation.previous_output_path);
        }
    }
    command.arg("--headless");
    Some(command)
}

fn update_progress(job_id: &str, value: &Value, runtime_status: &str) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    let Some(current) = state.jobs.get_mut(job_id) else {
        return;
    };
    if current.stage == "cancelled" {
        return;
    }
    if let Some(stage) = value.get("stage").and_then(Value::as_str) {
        current.stage = stage.to_string();
    }
    if let Some(progress_text) = value.get("progressText").and_then(Value::as_str) {
        current.progress_text = progress_text.to_string();
    }
    if let Some(phase) = value.get("phase").and_then(Value::as_str) {
        current.phase = Some(phase.to_string());
    }
    if let Some(workspace_state) = value.get("workspaceState").and_then(Value::as_str) {
        current.workspace_state = Some(workspace_state.to_string());
    }
    if let Some(elapsed_ms) = value.get("elapsedMs").and_then(Value::as_u64) {
        current.elapsed_ms = Some(elapsed_ms);
    }
    if let Some(estimated_total_ms) = value.get("estimatedTotalMs").and_then(Value::as_u64) {
        current.estimated_total_ms = Some(estimated_total_ms);
    }
    if let Some(progress_ratio) = value.get("progressRatio").and_then(Value::as_f64) {
        current.progress_ratio = Some(progress_ratio);
    }
    if let Some(timing_sample_count) = value.get("timingSampleCount").and_then(Value::as_u64) {
        current.timing_sample_count = Some(timing_sample_count);
    }
    if let Some(preview_path) = value.get("previewPath").and_then(Value::as_str) {
        current.preview_path = Some(preview_path.to_string());
    }
    if let Some(output_path) = value.get("outputPath").and_then(Value::as_str) {
        current.output_path = Some(output_path.to_string());
    }
    if let Some(output_name) = value.get("outputName").and_then(Value::as_str) {
        current.output_name = Some(output_name.to_string());
    }
    if let Some(is_segmented) = value.get("isSegmented").and_then(Value::as_bool) {
        current.is_segmented = is_segmented;
    }
    if let Some(can_segment) = value.get("canSegment").and_then(Value::as_bool) {
        current.can_segment = can_segment;
    }
    current.runtime_status = runtime_status.to_string();
}

fn update_preview(job_id: &str, preview_path: String) {
    if let Ok(mut state) = STATE.lock()
        && let Some(current) = state.jobs.get_mut(job_id)
        && current.stage != "cancelled"
    {
        current.preview_path = Some(preview_path);
    }
}

fn finish_job(job_id: &str, status: JobStatus, continuation: Option<Continuation>) {
    let completed = (status.stage == "done").then(|| status.clone());
    if let Ok(mut state) = STATE.lock() {
        if state
            .jobs
            .get(job_id)
            .is_none_or(|item| item.stage == "cancelled")
        {
            state.pids.remove(job_id);
            return;
        }
        state.jobs.insert(job_id.to_string(), status);
        if let Some(continuation) = continuation {
            state.continuations.insert(job_id.to_string(), continuation);
        }
        state.pids.remove(job_id);
    }
    if let Some(status) = completed
        && let (Some(source_path), Some(output_path)) = (
            status.source_image_path.as_deref(),
            status.output_path.as_deref(),
        )
        && let Err(error) = crate::overlay::generation_history::record(
            "3d",
            source_path,
            output_path,
            serde_json::json!({ "isSegmented": status.is_segmented }),
        )
    {
        crate::log_info!("[3D Generator] Could not record result history: {error}");
    }
}

pub(super) fn run_runtime_operation(job_id: String, operation: RuntimeOperation) {
    if runtime_command().is_none() {
        update_progress(
            &job_id,
            &serde_json::json!({
                "stage": "preparing",
                "phase": "engine_setup",
                "progressText": "Preparing the 3D engine"
            }),
            "installing",
        );
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        if let Err(error) = creation_runtime::download_runtime(stop, true) {
            finish_job(
                &job_id,
                JobStatus {
                    job_id: Some(job_id.clone()),
                    stage: "failed".to_string(),
                    progress_text: "The 3D engine could not be prepared.".to_string(),
                    phase: Some("failed".to_string()),
                    workspace_state: None,
                    elapsed_ms: None,
                    estimated_total_ms: None,
                    progress_ratio: None,
                    timing_sample_count: None,
                    output_path: None,
                    output_name: None,
                    preview_path: None,
                    source_image_path: Some(operation.source_image_path().to_string()),
                    is_segmented: false,
                    can_segment: false,
                    error: Some(error.to_string()),
                    runtime_status: "missing".to_string(),
                },
                None,
            );
            return;
        }
    }
    let runtime_status = runtime_status_label();
    let source_image_path = operation.source_image_path().to_string();
    if matches!(&operation, RuntimeOperation::Generate { .. }) {
        update_progress(
            &job_id,
            &serde_json::json!({
                "stage": "preparing",
                "phase": "depth_preview",
                "progressText": "Preparing image depth"
            }),
            &runtime_status,
        );
        let preview_job_id = job_id.clone();
        let preview_source = source_image_path.clone();
        std::thread::spawn(move || {
            let stop = std::sync::Arc::new(AtomicBool::new(false));
            let result = depth_model::download_depth_model(stop, true)
                .and_then(|()| depth_model::create_depth_preview(&preview_source));
            match result {
                Ok(path) => update_preview(&preview_job_id, path.to_string_lossy().to_string()),
                Err(error) => crate::log_info!(
                    "[3D Generator] Optional depth preview failed for {}: {error}",
                    preview_source
                ),
            }
        });
    }
    let mut command = match command_for_operation(&operation) {
        Some(command) => command,
        None => {
            finish_job(
                &job_id,
                JobStatus {
                    job_id: Some(job_id.clone()),
                    stage: "failed".to_string(),
                    progress_text: "The 3D engine is not installed.".to_string(),
                    phase: None,
                    workspace_state: None,
                    elapsed_ms: None,
                    estimated_total_ms: None,
                    progress_ratio: None,
                    timing_sample_count: None,
                    output_path: None,
                    output_name: None,
                    preview_path: None,
                    source_image_path: Some(source_image_path),
                    is_segmented: false,
                    can_segment: false,
                    error: Some("runtime_missing".to_string()),
                    runtime_status,
                },
                None,
            );
            return;
        }
    };
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .creation_flags_windows();

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            finish_job(
                &job_id,
                JobStatus {
                    job_id: Some(job_id.clone()),
                    stage: "failed".to_string(),
                    progress_text: "The 3D engine could not start.".to_string(),
                    phase: None,
                    workspace_state: None,
                    elapsed_ms: None,
                    estimated_total_ms: None,
                    progress_ratio: None,
                    timing_sample_count: None,
                    output_path: None,
                    output_name: None,
                    preview_path: None,
                    source_image_path: Some(source_image_path),
                    is_segmented: false,
                    can_segment: false,
                    error: Some(err.to_string()),
                    runtime_status,
                },
                None,
            );
            return;
        }
    };
    let keep_running = if let Ok(mut state) = STATE.lock() {
        if state
            .jobs
            .get(&job_id)
            .is_some_and(|status| status.stage != "cancelled")
        {
            state.pids.insert(job_id.clone(), child.id());
            true
        } else {
            false
        }
    } else {
        false
    };
    if !keep_running {
        let _ = child.kill();
        let _ = child.wait();
        return;
    }

    let stderr = child.stderr.take();
    let stderr_tail = std::sync::Arc::new(Mutex::new(String::new()));
    if let Some(stderr) = stderr {
        let stderr_tail = stderr_tail.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut tail) = stderr_tail.lock() {
                    tail.push_str(&line);
                    tail.push('\n');
                    if tail.len() > 8000 {
                        let keep_from = tail.len().saturating_sub(6000);
                        *tail = tail[keep_from..].to_string();
                    }
                }
            }
        });
    }

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if value.get("event").and_then(Value::as_str) == Some("progress") {
                update_progress(&job_id, &value, &runtime_status);
                continue;
            }

            if value.get("ok").and_then(Value::as_bool) == Some(true) {
                let result = value.get("result").cloned().unwrap_or(Value::Null);
                let output_path = result
                    .get("outputPath")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let output_name = result
                    .get("outputName")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| {
                        output_path.as_deref().and_then(|path| {
                            Path::new(path)
                                .file_name()
                                .map(|name| name.to_string_lossy().to_string())
                        })
                    });
                let preview_path = result
                    .get("previewPath")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| job_status(Some(&job_id)).preview_path);
                let is_segmented = result
                    .get("isSegmented")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if is_segmented
                    && let Some(profile_dir) = result.get("profileDir").and_then(Value::as_str)
                    && let Ok(mut state) = STATE.lock()
                {
                    state.invalidate_profile_continuations(profile_dir);
                }
                let can_segment = result
                    .get("canSegment")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let continuation = if can_segment {
                    let task_id = result.get("taskId").and_then(Value::as_str);
                    let profile_dir = result.get("profileDir").and_then(Value::as_str);
                    let output_dir = result.get("outputDir").and_then(Value::as_str);
                    match (task_id, profile_dir, output_dir, output_path.as_deref()) {
                        (Some(task_id), Some(profile_dir), Some(output_dir), Some(output_path)) => {
                            Some(Continuation {
                                task_id: task_id.to_string(),
                                profile_dir: profile_dir.to_string(),
                                image_path: source_image_path.clone(),
                                output_dir: PathBuf::from(output_dir),
                                previous_output_path: PathBuf::from(output_path),
                                preview_path: preview_path.clone(),
                            })
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                finish_job(
                    &job_id,
                    JobStatus {
                        job_id: Some(job_id.clone()),
                        stage: "done".to_string(),
                        progress_text: if is_segmented {
                            "Parts ready".to_string()
                        } else {
                            "Model ready".to_string()
                        },
                        phase: Some("complete".to_string()),
                        workspace_state: None,
                        elapsed_ms: None,
                        estimated_total_ms: None,
                        progress_ratio: Some(1.0),
                        timing_sample_count: None,
                        output_path,
                        output_name,
                        preview_path,
                        source_image_path: Some(source_image_path.clone()),
                        is_segmented,
                        can_segment: can_segment && continuation.is_some(),
                        error: None,
                        runtime_status: runtime_status.clone(),
                    },
                    continuation,
                );
                let _ = child.wait();
                let _ = prepare_runtime();
                return;
            }

            if value.get("ok").and_then(Value::as_bool) == Some(false) {
                let error = value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("Creation failed")
                    .to_string();
                let current = job_status(Some(&job_id));
                finish_job(
                    &job_id,
                    JobStatus {
                        job_id: Some(job_id.clone()),
                        stage: "failed".to_string(),
                        progress_text: "Creation was interrupted.".to_string(),
                        phase: Some("failed".to_string()),
                        workspace_state: None,
                        elapsed_ms: None,
                        estimated_total_ms: None,
                        progress_ratio: None,
                        timing_sample_count: None,
                        output_path: current.output_path,
                        output_name: current.output_name,
                        preview_path: current.preview_path,
                        source_image_path: Some(source_image_path.clone()),
                        is_segmented: false,
                        can_segment: false,
                        error: Some(error),
                        runtime_status: runtime_status.clone(),
                    },
                    None,
                );
                let _ = child.wait();
                let _ = prepare_runtime();
                return;
            }
        }
    }

    let process_status = child.wait().ok();
    let stderr = stderr_tail
        .lock()
        .map(|tail| tail.trim().to_string())
        .unwrap_or_default();
    let message = if stderr.is_empty() {
        format!("3D engine exited unexpectedly: {process_status:?}")
    } else {
        format!("3D engine exited unexpectedly: {process_status:?}. {stderr}")
    };
    let current = job_status(Some(&job_id));
    finish_job(
        &job_id,
        JobStatus {
            job_id: Some(job_id.clone()),
            stage: "failed".to_string(),
            progress_text: "Creation was interrupted.".to_string(),
            phase: Some("failed".to_string()),
            workspace_state: None,
            elapsed_ms: None,
            estimated_total_ms: None,
            progress_ratio: None,
            timing_sample_count: None,
            output_path: current.output_path,
            output_name: current.output_name,
            preview_path: current.preview_path,
            source_image_path: Some(source_image_path),
            is_segmented: false,
            can_segment: false,
            error: Some(message),
            runtime_status,
        },
        None,
    );
    let _ = prepare_runtime();
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
