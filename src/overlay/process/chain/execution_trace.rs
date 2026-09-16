use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static NEXT_EXECUTION: AtomicU64 = AtomicU64::new(1);

pub(super) struct ExecutionTrace {
    id: u64,
    started: Instant,
}

impl ExecutionTrace {
    pub(super) fn start(block: usize, model: &str) -> Self {
        let trace = Self {
            id: NEXT_EXECUTION.fetch_add(1, Ordering::Relaxed),
            started: Instant::now(),
        };
        crate::log_info!(
            "[PresetExecution] execution={} block={} start_model={:?}",
            trace.id,
            block,
            model
        );
        trace
    }

    pub(super) fn fallback(&self, from: &str, to: &str, reason: &str) {
        let reason: String = reason
            .chars()
            .take(320)
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .collect();
        crate::log_info!(
            "[PresetExecution] execution={} elapsed_ms={} fallback_from={:?} fallback_to={:?} reason={:?}",
            self.id,
            self.started.elapsed().as_millis(),
            from,
            to,
            reason,
        );
    }

    pub(super) fn finish(&self, model: &str, result: &anyhow::Result<String>) {
        crate::log_info!(
            "[PresetExecution] execution={} total_ms={} final_model={:?} status={}",
            self.id,
            self.started.elapsed().as_millis(),
            model,
            if result.is_ok() { "ok" } else { "error" },
        );
    }
}
