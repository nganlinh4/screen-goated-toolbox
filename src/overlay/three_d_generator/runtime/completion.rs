use super::{Continuation, JobStatus, RuntimeState, STATE, status_is_non_publishable};
use crate::overlay::creation_delivery::DeliveryOutcome;

enum Settlement {
    Unchanged,
    Delivered,
    Failed(Option<Box<Continuation>>),
}

fn settle(
    state: &mut RuntimeState,
    job_id: &str,
    dispatch_id: &str,
    output_path: &str,
    outcome: DeliveryOutcome,
    failure_status: Option<JobStatus>,
) -> Settlement {
    let Some(operation) = state.operations.get(job_id) else {
        return Settlement::Unchanged;
    };
    if operation.dispatch_id() != dispatch_id
        || state
            .jobs
            .get(job_id)
            .is_none_or(|status| status_is_non_publishable(&status.stage))
        || state
            .pending_completions
            .get(job_id)
            .is_none_or(|(status, _)| status.output_path.as_deref() != Some(output_path))
    {
        return Settlement::Unchanged;
    }
    match outcome {
        DeliveryOutcome::Pending => Settlement::Unchanged,
        DeliveryOutcome::Missing | DeliveryOutcome::Cancelled => {
            let Some(status) = failure_status else {
                return Settlement::Unchanged;
            };
            state.jobs.insert(job_id.to_string(), status);
            let continuation = state
                .pending_completions
                .remove(job_id)
                .and_then(|(_, continuation)| continuation.map(Box::new));
            state.operations.remove(job_id);
            state.request_fingerprints.remove(job_id);
            state.deadlines.remove(job_id);
            state.recovered_jobs.remove(job_id);
            state.prune_terminal_jobs();
            Settlement::Failed(continuation)
        }
        DeliveryOutcome::Delivered => {
            let Some((status, continuation)) = state.pending_completions.remove(job_id) else {
                return Settlement::Unchanged;
            };
            state.jobs.insert(job_id.to_string(), status);
            if let Some(continuation) = continuation {
                state.continuations.insert(job_id.to_string(), continuation);
            }
            state.operations.remove(job_id);
            state.request_fingerprints.remove(job_id);
            state.deadlines.remove(job_id);
            state.recovered_jobs.remove(job_id);
            state.prune_terminal_jobs();
            Settlement::Delivered
        }
    }
}

pub(super) fn settle_reconciled_completions() {
    let candidates = match STATE.lock() {
        Ok(state) => state
            .pending_completions
            .iter()
            .filter_map(|(job_id, (status, _))| {
                let operation = state.operations.get(job_id)?;
                Some((
                    job_id.clone(),
                    operation.clone(),
                    status.output_path.clone()?,
                ))
            })
            .collect::<Vec<_>>(),
        Err(_) => return,
    };
    for (job_id, operation, output_path) in candidates {
        let dispatch_id = operation.dispatch_id();
        let Ok(outcome) =
            crate::overlay::creation_delivery::outcome("3d", &job_id, dispatch_id, &output_path)
        else {
            continue;
        };
        let failure_status = matches!(
            outcome,
            DeliveryOutcome::Missing | DeliveryOutcome::Cancelled
        )
        .then(|| super::failure::failed_status(&job_id, &operation));
        let settlement = match STATE.lock() {
            Ok(mut state) => settle(
                &mut state,
                &job_id,
                dispatch_id,
                &output_path,
                outcome,
                failure_status,
            ),
            Err(_) => return,
        };
        match settlement {
            Settlement::Unchanged => {}
            Settlement::Delivered => super::schedule_next(),
            Settlement::Failed(continuation) => {
                if let Some(continuation) = continuation {
                    let _ = crate::overlay::creation_source_snapshot::release_continuation(
                        std::slice::from_ref(&continuation.source_descriptor),
                        &job_id,
                    );
                }
                super::schedule_next();
            }
        }
    }
}

#[cfg(test)]
mod tests;
