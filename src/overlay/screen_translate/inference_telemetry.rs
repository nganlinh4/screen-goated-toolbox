use std::time::Instant;

pub(super) struct AttemptTrace<'a> {
    trace_id: &'a str,
    sequence: usize,
    model: &'a str,
    provider: &'a str,
    pending: usize,
    started: Instant,
    first_chunk_ms: Option<f64>,
    first_validated_ms: Option<f64>,
    transport_ms: Option<f64>,
}

impl<'a> AttemptTrace<'a> {
    pub(super) fn new(
        trace_id: &'a str,
        sequence: usize,
        model: &'a str,
        provider: &'a str,
        pending: usize,
    ) -> Self {
        Self {
            trace_id,
            sequence,
            model,
            provider,
            pending,
            started: Instant::now(),
            first_chunk_ms: None,
            first_validated_ms: None,
            transport_ms: None,
        }
    }

    pub(super) fn observe_chunk(&mut self, chunk: &str) {
        if self.first_chunk_ms.is_none() && !chunk.trim().is_empty() {
            self.first_chunk_ms = Some(self.elapsed_ms());
        }
    }

    pub(super) fn observe_validated_region(&mut self) {
        if self.first_validated_ms.is_none() {
            self.first_validated_ms = Some(self.elapsed_ms());
        }
    }

    pub(super) fn transport_complete(&mut self) {
        self.transport_ms = Some(self.elapsed_ms());
    }

    pub(super) fn finish(self, outcome: &str, accepted: usize, unresolved: usize, rejected: usize) {
        crate::log_info!(
            "[ScreenTranslateModelPerf] trace={} attempt={} model={} provider={} outcome={} pending={} accepted={} unresolved={} rejected={} first_chunk_ms={} first_validated_ms={} transport_ms={} total_ms={:.1}",
            self.trace_id,
            self.sequence,
            self.model,
            self.provider,
            outcome,
            self.pending,
            accepted,
            unresolved,
            rejected,
            optional_ms(self.first_chunk_ms),
            optional_ms(self.first_validated_ms),
            optional_ms(self.transport_ms),
            self.elapsed_ms(),
        );
    }

    fn elapsed_ms(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }
}

fn optional_ms(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_string(), |value| format!("{value:.1}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_transport_chunks_do_not_claim_first_output() {
        let mut trace = AttemptTrace::new("trace", 1, "model", "provider", 2);
        trace.observe_chunk("  ");
        assert!(trace.first_chunk_ms.is_none());
        trace.observe_chunk("{");
        assert!(trace.first_chunk_ms.is_some());
    }
}
