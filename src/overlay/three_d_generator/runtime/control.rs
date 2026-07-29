use std::path::PathBuf;

use super::{
    JobStatus, RuntimeOperation, STATE, default_output_dir, idle_status, job_status, schedule_next,
    status_is_busy,
};

struct CancelCandidate {
    job_id: String,
    previous_status: JobStatus,
    operation: RuntimeOperation,
    request_fingerprint: String,
    pid: Option<u32>,
}

pub(in crate::overlay::three_d_generator) fn remap_result_path(previous: &str, current: &str) {
    let current_name = PathBuf::from(current)
        .file_name()
        .map(|name| name.to_string_lossy().to_string());
    if let Ok(mut state) = STATE.lock() {
        for status in state.jobs.values_mut() {
            if status
                .output_path
                .as_deref()
                .is_some_and(|path| path.eq_ignore_ascii_case(previous))
            {
                status.output_path = Some(current.to_string());
                status.output_name = current_name.clone();
            }
        }
        for continuation in state.continuations.values_mut() {
            if continuation
                .previous_output_path
                .to_string_lossy()
                .eq_ignore_ascii_case(previous)
            {
                continuation.previous_output_path = PathBuf::from(current);
            }
        }
    }
}

pub(in crate::overlay::three_d_generator) fn forget_result_path(path: &str) {
    if let Ok(mut state) = STATE.lock() {
        for status in state.jobs.values_mut() {
            if status
                .output_path
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(path))
            {
                status.output_path = None;
                status.output_name = None;
                status.can_segment = false;
            }
        }
        let removed = state
            .continuations
            .iter()
            .filter(|(_, continuation)| {
                continuation
                    .previous_output_path
                    .to_string_lossy()
                    .eq_ignore_ascii_case(path)
            })
            .map(|(job_id, _)| job_id.clone())
            .collect::<Vec<_>>();
        for job_id in removed {
            if let Some(continuation) = state.continuations.remove(&job_id) {
                let _ = crate::overlay::creation_source_snapshot::release_continuation(
                    std::slice::from_ref(&continuation.source_descriptor),
                    &job_id,
                );
            }
        }
    }
}

pub(in crate::overlay::three_d_generator) fn is_known_result_path(path: &std::path::Path) -> bool {
    let known_in_session = STATE.lock().is_ok_and(|state| {
        state.jobs.values().any(|status| {
            status
                .output_path
                .as_deref()
                .and_then(|value| std::fs::canonicalize(value).ok())
                .as_deref()
                == Some(path)
        })
    });
    known_in_session
        || crate::overlay::generation_history::list("3d").is_ok_and(|entries| {
            entries.iter().any(|entry| {
                std::fs::canonicalize(&entry.output_path).ok().as_deref() == Some(path)
            })
        })
}

pub(in crate::overlay::three_d_generator) fn cancel_job(job_id: Option<&str>) -> JobStatus {
    cancel_jobs(job_id, false).1
}

pub(in crate::overlay::three_d_generator) fn cancel_for_shutdown() -> bool {
    cancel_jobs(None, true).0
}

fn cancel_jobs(job_id: Option<&str>, shutdown: bool) -> (bool, JobStatus) {
    let (candidates, mut durable) = {
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        let targets: Vec<String> = match job_id {
            Some(job_id) => vec![job_id.to_string()],
            None => state
                .jobs
                .iter()
                .filter(|(_, status)| {
                    status.stage == "queued"
                        || status_is_busy(&status.stage)
                        || shutdown && status.stage == "cancelling"
                })
                .map(|(job_id, _)| job_id.clone())
                .collect(),
        };
        let target_count = targets.len();
        let mut candidates = Vec::new();
        for target in targets {
            let Some(previous_status) = state.jobs.get(&target).cloned() else {
                continue;
            };
            if previous_status.stage != "queued"
                && !status_is_busy(&previous_status.stage)
                && !(shutdown && previous_status.stage == "cancelling")
            {
                continue;
            }
            let Some(operation) = state.operations.get(&target).cloned() else {
                continue;
            };
            let Some(request_fingerprint) = state.request_fingerprints.get(&target).cloned() else {
                continue;
            };
            if previous_status.stage != "cancelling"
                && let Some(status) = state.jobs.get_mut(&target)
            {
                status.stage = "cancelling".to_string();
                status.progress_text = "Cancelling.".to_string();
                status.error = None;
            }
            candidates.push(CancelCandidate {
                job_id: target.clone(),
                previous_status,
                operation,
                request_fingerprint,
                pid: state.pids.get(&target).copied(),
            });
        }
        let complete = candidates.len() == target_count;
        (candidates, complete)
    };
    let mut pids_to_kill = std::collections::HashSet::new();
    for candidate in candidates {
        let cancelled = crate::overlay::creation_delivery::cancel_dispatch(
            crate::overlay::creation_delivery::CancelledDelivery {
                product: "3d",
                job_id: candidate.job_id.clone(),
                dispatch_id: candidate.operation.dispatch_id().to_string(),
                request_fingerprint: candidate.request_fingerprint,
                output_name: candidate.operation.output_name().to_string(),
            },
        )
        .is_ok();
        durable &= cancelled;
        let mut kill = shutdown.then_some(candidate.pid).flatten();
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        if state
            .jobs
            .get(&candidate.job_id)
            .is_some_and(|status| status.stage == "cancelling")
        {
            if cancelled {
                if let Some(status) = state.jobs.get_mut(&candidate.job_id) {
                    status.stage = "cancelled".to_string();
                    status.progress_text = "Cancelled.".to_string();
                    status.error = None;
                }
                state.operations.remove(&candidate.job_id);
                state.request_fingerprints.remove(&candidate.job_id);
                state.deadlines.remove(&candidate.job_id);
                state.recovered_jobs.remove(&candidate.job_id);
                state.pending_completions.remove(&candidate.job_id);
                kill = state.pids.remove(&candidate.job_id).or(kill);
            } else if shutdown {
                kill = state.pids.remove(&candidate.job_id).or(kill);
            } else {
                state
                    .jobs
                    .insert(candidate.job_id.clone(), candidate.previous_status);
            }
        }
        drop(state);
        if let Some(pid) = kill {
            pids_to_kill.insert(pid);
        }
    }
    if shutdown {
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        pids_to_kill.extend(state.pids.drain().map(|(_, pid)| pid));
    } else {
        schedule_next();
    }
    for pid in pids_to_kill {
        crate::overlay::creation_recovery::terminate_process_tree(pid);
    }
    let status = STATE
        .lock()
        .ok()
        .and_then(|state| {
            job_id
                .and_then(|job_id| state.jobs.get(job_id).cloned())
                .or_else(|| state.latest_status())
        })
        .unwrap_or_else(idle_status);
    (durable, status)
}

pub(in crate::overlay::three_d_generator) fn open_output(
    kind: &str,
    requested_path: Option<&str>,
) -> Result<(), String> {
    let path = requested_path
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| job_status(None).output_path.map(PathBuf::from))
        .unwrap_or_else(default_output_dir);
    let target = if kind == "folder" {
        if path.is_file() {
            path.parent()
                .map(PathBuf::from)
                .unwrap_or_else(default_output_dir)
        } else {
            path
        }
    } else {
        path
    };
    open::that(&target).map_err(|_| "The selected result could not be opened.".to_string())
}
