use super::*;
use crate::overlay::creation_delivery::{CancellationOutcome, CancelledDelivery};
use crate::overlay::creation_process::Termination;

struct CancelCandidate {
    job_id: String,
    request: StartJobRequest,
    request_fingerprint: String,
    pid: Option<Termination>,
}

pub(in crate::overlay::image_to_svg) fn cancel_job(job_id: Option<&str>) -> Vec<JobStatus> {
    cancel_jobs(job_id, false).1
}

pub(in crate::overlay::image_to_svg) fn cancel_for_shutdown() -> bool {
    let Some(_admission) = ADMISSION.try_acquire() else {
        return false;
    };
    cancel_jobs(None, true).0
}

fn cancel_jobs(job_id: Option<&str>, shutdown: bool) -> (bool, Vec<JobStatus>) {
    cancel_jobs_with(
        job_id,
        shutdown,
        crate::overlay::creation_delivery::cancel_dispatch,
        Termination::terminate,
    )
}

fn cancel_jobs_with(
    job_id: Option<&str>,
    shutdown: bool,
    mut persist: impl FnMut(CancelledDelivery) -> Result<CancellationOutcome, String>,
    mut terminate: impl FnMut(Termination),
) -> (bool, Vec<JobStatus>) {
    let (candidates, mut durable) = {
        let state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        let targets = match job_id {
            Some(id) => vec![id.to_string()],
            None => state
                .jobs
                .iter()
                .filter(|(_, status)| {
                    status.stage == "queued"
                        || is_busy(&status.stage)
                        || shutdown && status.stage == "cancelling"
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>(),
        };
        let count = targets.len();
        let candidates = targets
            .into_iter()
            .filter_map(|id| {
                let status = state.jobs.get(&id)?;
                if status.stage != "queued"
                    && !is_busy(&status.stage)
                    && !(shutdown && status.stage == "cancelling")
                {
                    return None;
                }
                let (request, _) = state.requests.get(&id)?.clone();
                Some(CancelCandidate {
                    job_id: id.clone(),
                    request,
                    request_fingerprint: state.request_fingerprints.get(&id)?.clone(),
                    pid: state.pids.get(&id).copied().map(Termination::capture),
                })
            })
            .collect::<Vec<_>>();
        let complete = candidates.len() == count;
        (candidates, complete)
    };
    let mut pids_to_kill = std::collections::HashMap::new();
    for candidate in candidates {
        // Persistence decides the terminal outcome. Do not expose a provisional
        // cancellation to workers or restore a snapshot over concurrent progress.
        let outcome = {
            let output_name = candidate.request.output_name.clone();
            persist(CancelledDelivery {
                product: "svg",
                job_id: candidate.job_id.clone(),
                dispatch_id: candidate.request.dispatch_id.clone(),
                request_fingerprint: candidate.request_fingerprint.clone(),
                output_name,
            })
        };
        durable &= outcome.is_ok();
        let cancelled = outcome == Ok(CancellationOutcome::Cancelled);
        let mut kill = shutdown.then_some(candidate.pid).flatten();
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        let matching = state
            .requests
            .get(&candidate.job_id)
            .is_none_or(|(request, _)| request.dispatch_id == candidate.request.dispatch_id)
            && state
                .request_fingerprints
                .get(&candidate.job_id)
                .is_none_or(|value| value == &candidate.request_fingerprint);
        if matching
            && state
                .jobs
                .get(&candidate.job_id)
                .is_some_and(|status| !matches!(status.stage.as_str(), "done" | "cancelled"))
        {
            if cancelled {
                if let Some(status) = state.jobs.get_mut(&candidate.job_id) {
                    status.stage = "cancelled".to_string();
                    status.progress_text = "Cancelled".to_string();
                    // The queue renders the progress key in preference to the
                    // text, so a cancelled job that keeps its last worker key
                    // reads "Drawing vector paths" under a "Cancelled" heading.
                    status.progress_key = None;
                    status.phase = Some("cancelled".to_string());
                    status.error = None;
                }
                state.requests.remove(&candidate.job_id);
                state.request_fingerprints.remove(&candidate.job_id);
                state.deadlines.remove(&candidate.job_id);
                state.recovered_jobs.remove(&candidate.job_id);
                state.pending_completions.remove(&candidate.job_id);
                kill = state
                    .pids
                    .remove(&candidate.job_id)
                    .map(Termination::capture)
                    .or(kill);
                state.prune_terminal_jobs();
            } else if shutdown && outcome.is_err() {
                if let Some(status) = state.jobs.get_mut(&candidate.job_id)
                    && (status.stage == "queued"
                        || is_busy(&status.stage)
                        || status.stage == "cancelling")
                {
                    status.stage = "cancelling".to_string();
                    status.progress_text = "Cancelling".to_string();
                }
                kill = state
                    .pids
                    .remove(&candidate.job_id)
                    .map(Termination::capture)
                    .or(kill);
            }
        }
        drop(state);
        if let Some(pid) = kill {
            pids_to_kill.insert(pid.pid(), pid);
        }
        if cancelled {
            cleanup_request_staging(&candidate.request);
        }
    }
    if shutdown {
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        pids_to_kill.extend(
            state
                .pids
                .drain()
                .map(|(_, pid)| (pid, Termination::capture(pid))),
        );
    } else {
        schedule_next();
    }
    for pid in pids_to_kill.into_values() {
        terminate(pid);
    }
    (durable, job_statuses())
}

#[cfg(test)]
mod tests;
