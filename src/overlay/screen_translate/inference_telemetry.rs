use std::time::Instant;

pub(super) struct AttemptTrace<'a> {
    trace_id: &'a str,
    sequence: usize,
    model_id: &'a str,
    api_model: &'a str,
    provider: &'a str,
    pending: usize,
    started: Instant,
    first_chunk_ms: Option<f64>,
    first_validated_ms: Option<f64>,
    transport_ms: Option<f64>,
    content: Option<serde_json::Value>,
}

impl<'a> AttemptTrace<'a> {
    pub(super) fn new(
        trace_id: &'a str,
        sequence: usize,
        model_id: &'a str,
        api_model: &'a str,
        provider: &'a str,
        pending: usize,
    ) -> Self {
        Self {
            trace_id,
            sequence,
            model_id,
            api_model,
            provider,
            pending,
            started: Instant::now(),
            first_chunk_ms: None,
            first_validated_ms: None,
            transport_ms: None,
            content: None,
        }
    }

    pub(super) fn observe_chunk(&mut self, chunk: &str) {
        if let Some(content) = &mut self.content {
            let serde_json::Value::String(mut current) = content["streamedResponse"].take() else {
                unreachable!("stream evidence is initialized as text");
            };
            if current.len() + chunk.len() <= 2 * 1024 * 1024 {
                current.push_str(chunk);
            } else {
                content["streamTruncated"] = true.into();
            }
            content["streamedResponse"] = current.into();
        }
        if self.first_chunk_ms.is_none() && !chunk.trim().is_empty() {
            self.first_chunk_ms = Some(self.elapsed_ms());
        }
    }

    pub(super) fn request(&mut self, prompt: &str, schema: &serde_json::Value, target: &str) {
        if super::diagnostics_model_attempts::enabled(self.trace_id) {
            self.content = Some(serde_json::json!({
                "prompt": prompt, "schema": schema,
                "instruction": format!("Translate into {target}. Return only the requested structured screen translation."),
                "streamedResponse": "", "streamTruncated": false
            }));
        }
    }

    pub(super) fn response(&mut self, response: &anyhow::Result<String>) {
        if let Some(content) = &mut self.content {
            match response {
                Ok(text) if text.len() <= 2 * 1024 * 1024 => {
                    content["response"] = text.clone().into()
                }
                Ok(_) => content["responseTruncated"] = true.into(),
                Err(error) => content["error"] = format!("{error:#}").into(),
            }
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
        let total_ms = self.elapsed_ms();
        crate::log_info!(
            "[ScreenTranslateModelPerf] trace={} attempt={} model_id={} api_model={} provider={} outcome={} pending={} accepted={} unresolved={} rejected={} first_chunk_ms={} first_validated_ms={} transport_ms={} total_ms={:.1}",
            self.trace_id,
            self.sequence,
            self.model_id,
            self.api_model,
            self.provider,
            outcome,
            self.pending,
            accepted,
            unresolved,
            rejected,
            optional_ms(self.first_chunk_ms),
            optional_ms(self.first_validated_ms),
            optional_ms(self.transport_ms),
            total_ms,
        );
        super::diagnostics_model_attempts::record(
            self.trace_id,
            super::diagnostics_model_attempts::ModelAttemptRecord {
                sequence: self.sequence,
                model_id: self.model_id.to_string(),
                api_model: self.api_model.to_string(),
                provider: self.provider.to_string(),
                outcome: outcome.to_string(),
                pending_region_count: self.pending,
                accepted_region_count: accepted,
                unresolved_region_count: unresolved,
                rejected_region_count: rejected,
                first_chunk_ms: self.first_chunk_ms,
                first_validated_ms: self.first_validated_ms,
                transport_ms: self.transport_ms,
                total_ms,
                content: self.content,
            },
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
        let mut trace = AttemptTrace::new("trace", 1, "model-id", "api-model", "provider", 2);
        trace.observe_chunk("  ");
        assert!(trace.first_chunk_ms.is_none());
        trace.observe_chunk("{");
        assert!(trace.first_chunk_ms.is_some());
    }

    #[test]
    #[cfg(debug_assertions)]
    fn enabled_evidence_preserves_request_and_response() {
        let id = "translation-content-evidence-test";
        super::super::diagnostics_model_attempts::begin_trace(id);
        let mut trace = AttemptTrace::new(id, 1, "model", "api-model", "provider", 1);
        trace.request(
            "Members: source text",
            &serde_json::json!({"type":"object"}),
            "target",
        );
        trace.observe_chunk("{\"translations\":");
        trace.observe_chunk("{\"0\":\"translated text\"}}");
        trace.response(&Ok("{\"translations\":{\"0\":\"translated text\"}}".into()));
        trace.finish("complete", 1, 0, 0);
        let records = super::super::diagnostics_model_attempts::take(id);
        let content = records[0].content.as_ref().unwrap();
        assert_eq!(content["prompt"], "Members: source text");
        assert_eq!(content["response"], content["streamedResponse"]);
    }
}
