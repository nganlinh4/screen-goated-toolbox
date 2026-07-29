use std::io::Write as _;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use super::{
    STATE, StartJobRequest, finish, finish_retaining_intent, hide_command_window, job_cancelled,
    revalidate_request_sources, runtime_command, update_progress, validate_runtime_result,
};

pub(super) fn run_job(
    job_id: String,
    request: StartJobRequest,
    request_fingerprint: String,
    deadline_at_ms: u64,
    recovered: bool,
) {
    if job_cancelled(&job_id) {
        finish(&job_id, Err("Cancelled".to_string()));
        return;
    }
    if let Err(error) = revalidate_request_sources(&request) {
        finish(&job_id, Err(error));
        return;
    }
    if runtime_command().is_none() {
        let stop = Arc::new(AtomicBool::new(false));
        if crate::overlay::creation_runtime::download_runtime(stop, true).is_err() {
            finish(&job_id, Err("Creation engine is unavailable.".to_string()));
            return;
        }
    }
    if job_cancelled(&job_id) {
        finish(&job_id, Err("Cancelled".to_string()));
        return;
    }
    let Some(mut command) = runtime_command() else {
        finish(&job_id, Err("Creation engine is unavailable.".to_string()));
        return;
    };
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_command_window(&mut command);
    let child = {
        let Ok(mut state) = STATE.lock() else {
            return;
        };
        if state
            .jobs
            .get(&job_id)
            .is_none_or(|job| super::is_non_publishable(&job.stage))
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
        finish(&job_id, Err("Creation engine could not start.".to_string()));
        return;
    };
    let Some(mut stdin) = child.stdin.take() else {
        crate::overlay::creation_recovery::terminate_process_tree(child.id());
        finish(&job_id, Err("Creation engine could not start.".to_string()));
        return;
    };
    let Some(stdout) = child.stdout.take() else {
        crate::overlay::creation_recovery::terminate_process_tree(child.id());
        finish(&job_id, Err("Creation engine could not start.".to_string()));
        return;
    };
    let Ok(mut events) =
        crate::overlay::creation_process_supervisor::EventSupervisor::with_deadline(
            stdout,
            deadline_at_ms,
        )
    else {
        crate::overlay::creation_recovery::terminate_process_tree(child.id());
        finish(
            &job_id,
            Err("Creation exceeded its time limit.".to_string()),
        );
        return;
    };
    let (message, mut accepted) = if recovered {
        match recovery_message(
            &job_id,
            &request,
            &request_fingerprint,
            &mut stdin,
            child.id(),
            &mut events,
        ) {
            Ok(message) => message,
            Err(error) => {
                crate::overlay::creation_recovery::terminate_process_tree(child.id());
                let _ = child.wait();
                finish_retaining_intent(&job_id, Err(error));
                return;
            }
        }
    } else {
        (
            fresh_message(&job_id, &request, &request_fingerprint),
            false,
        )
    };
    if writeln!(stdin, "{message}").is_err() {
        crate::overlay::creation_recovery::terminate_process_tree(child.id());
        let _ = child.wait();
        let result = Err("Creation could not start.".to_string());
        if accepted {
            finish_retaining_intent(&job_id, result);
        } else {
            finish(&job_id, result);
        }
        return;
    }
    drop(stdin);

    let mut final_result = None;
    let mut proven_terminal = false;
    loop {
        match events.next(child.id(), || job_cancelled(&job_id)) {
            Ok(Some(value)) => {
                if matches!(
                    value.get("acceptanceState").and_then(Value::as_str),
                    Some("accepted" | "recovering" | "completed")
                ) {
                    accepted = true;
                }
                if value.get("event").and_then(Value::as_str) == Some("progress") {
                    update_progress(&job_id, &value);
                } else if value.get("ok").and_then(Value::as_bool) == Some(true) {
                    final_result = Some(
                        value
                            .get("result")
                            .cloned()
                            .ok_or_else(|| "Creation returned an invalid image.".to_string()),
                    );
                    proven_terminal = true;
                } else if value.get("ok").and_then(Value::as_bool) == Some(false) {
                    let recovery_not_found = value.get("error").and_then(Value::as_str)
                        == Some("creation.recovery_not_found");
                    final_result =
                        Some(Err(crate::overlay::creation_recovery::public_error(&value)));
                    proven_terminal = !recovery_not_found;
                }
            }
            Ok(None) => break,
            Err(error) => {
                final_result = Some(Err(error));
                break;
            }
        }
    }
    let _ = child.wait();
    let result = final_result
        .unwrap_or_else(|| Err("Creation ended before returning an image.".to_string()))
        .and_then(|value| validate_runtime_result(&request, value));
    if accepted && !proven_terminal && !job_cancelled(&job_id) {
        finish_retaining_intent(&job_id, result);
    } else {
        finish(&job_id, result);
    }
}

fn fresh_message(job_id: &str, request: &StartJobRequest, request_fingerprint: &str) -> Value {
    json!({
        "id": job_id,
        "cmd": "start_image_job",
        "args": {
            "dispatchId": &request.dispatch_id,
            "requestFingerprint": request_fingerprint,
            "imagePaths": &request.image_paths,
            "imagePath": &request.image_path,
            "sourceDescriptors": &request.source_descriptors,
            "outputDir": &request.output_dir,
            "outputName": &request.output_name,
            "prompt": &request.prompt,
        }
    })
}

fn recovery_message(
    job_id: &str,
    request: &StartJobRequest,
    fingerprint: &str,
    stdin: &mut impl std::io::Write,
    pid: u32,
    events: &mut crate::overlay::creation_process_supervisor::EventSupervisor,
) -> Result<(Value, bool), String> {
    let identity = crate::overlay::creation_recovery::Identity {
        tool: "image",
        operation: "create_image",
        dispatch_id: &request.dispatch_id,
        request_fingerprint: fingerprint,
    };
    let query_id = format!("{job_id}-query");
    writeln!(
        stdin,
        "{}",
        crate::overlay::creation_recovery::query_message(&query_id, &identity)
    )
    .map_err(|_| "Creation recovery could not start.".to_string())?;
    let response = events
        .next(pid, || job_cancelled(job_id))?
        .ok_or_else(|| "Creation recovery returned no status.".to_string())?;
    match crate::overlay::creation_recovery::parse_query_response(&response, &identity)? {
        crate::overlay::creation_recovery::State::None => {
            Ok((fresh_message(job_id, request, fingerprint), false))
        }
        _ => Ok((
            crate::overlay::creation_recovery::resume_message(
                job_id,
                &identity,
                private_request_value(request)?,
            ),
            true,
        )),
    }
}

fn private_request_value(request: &StartJobRequest) -> Result<Value, String> {
    let mut value = serde_json::to_value(request)
        .map_err(|_| "Creation recovery request is invalid.".to_string())?;
    if let Some(arguments) = value.as_object_mut() {
        arguments.remove("finalOutputDir");
    }
    Ok(value)
}
