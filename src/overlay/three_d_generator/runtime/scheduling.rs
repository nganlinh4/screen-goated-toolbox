//! Admission of queued jobs into the bounded set of running runtime operations.
use super::{MAX_PARALLEL_JOBS, RuntimeOperation, RuntimeState, STATE, run_runtime_operation};

/// Everything a worker thread needs to run one claimed job.
struct Dispatch {
    job_id: String,
    operation: RuntimeOperation,
    request_fingerprint: String,
    deadline_at_ms: u64,
    recovered: bool,
}

impl RuntimeState {
    /// Claims the oldest queued job when a parallel slot is free, moving it to
    /// its first running stage so later passes neither re-dispatch it nor
    /// ignore it for capacity.
    fn claim_next_dispatch(&mut self) -> Option<Dispatch> {
        if self.running_count() >= MAX_PARALLEL_JOBS {
            return None;
        }
        let job_id = self.next_queued_job()?;
        let operation = self.operations.get(&job_id)?.clone();
        let request_fingerprint = self.request_fingerprints.get(&job_id)?.clone();
        let deadline_at_ms = *self.deadlines.get(&job_id)?;
        let recovered = self.recovered_jobs.remove(&job_id);
        if let Some(status) = self.jobs.get_mut(&job_id) {
            let (stage, text, phase) = match operation {
                RuntimeOperation::Segment { .. } => {
                    ("segmenting", "Separating model parts.", "separation")
                }
                RuntimeOperation::Refine { .. } => {
                    ("refining", "Creating a new version.", "refinement")
                }
                RuntimeOperation::Generate { .. } => {
                    ("preparing", "Preparing creation.", "preparing")
                }
            };
            status.stage = stage.to_string();
            status.progress_text = text.to_string();
            status.phase = Some(phase.to_string());
        }
        Some(Dispatch {
            job_id,
            operation,
            request_fingerprint,
            deadline_at_ms,
            recovered,
        })
    }
}

pub(super) fn schedule_next() {
    loop {
        // The closure releases STATE before the worker thread starts.
        let Some(next) = STATE
            .lock()
            .ok()
            .and_then(|mut state| state.claim_next_dispatch())
        else {
            return;
        };
        std::thread::spawn(move || {
            run_runtime_operation(
                next.job_id,
                next.operation,
                next.request_fingerprint,
                next.deadline_at_ms,
                next.recovered,
            )
        });
    }
}

#[cfg(test)]
mod tests;
