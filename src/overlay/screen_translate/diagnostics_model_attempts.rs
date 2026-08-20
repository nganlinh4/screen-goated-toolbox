use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModelAttemptRecord {
    pub sequence: usize,
    pub model_id: String,
    pub api_model: String,
    pub provider: String,
    pub outcome: String,
    pub pending_region_count: usize,
    pub accepted_region_count: usize,
    pub unresolved_region_count: usize,
    pub rejected_region_count: usize,
    pub first_chunk_ms: Option<f64>,
    pub first_validated_ms: Option<f64>,
    pub transport_ms: Option<f64>,
    pub total_ms: f64,
}

#[cfg(debug_assertions)]
mod implementation {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};

    use super::ModelAttemptRecord;

    static ATTEMPTS: LazyLock<Mutex<HashMap<String, Vec<ModelAttemptRecord>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    pub(crate) fn begin_trace(trace_id: &str) {
        ATTEMPTS
            .lock()
            .unwrap()
            .insert(trace_id.to_string(), Vec::new());
    }

    pub(crate) fn record(trace_id: &str, attempt: ModelAttemptRecord) {
        if let Some(attempts) = ATTEMPTS.lock().unwrap().get_mut(trace_id) {
            attempts.push(attempt);
        }
    }

    pub(crate) fn take(trace_id: &str) -> Vec<ModelAttemptRecord> {
        ATTEMPTS
            .lock()
            .unwrap()
            .remove(trace_id)
            .unwrap_or_default()
    }
}

#[cfg(not(debug_assertions))]
mod implementation {
    use super::ModelAttemptRecord;

    pub(crate) fn begin_trace(_trace_id: &str) {}
    pub(crate) fn record(_trace_id: &str, _attempt: ModelAttemptRecord) {}
    pub(crate) fn take(_trace_id: &str) -> Vec<ModelAttemptRecord> {
        Vec::new()
    }
}

pub(super) use implementation::{begin_trace, record, take};

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;

    #[test]
    fn records_only_dispatched_models_in_attempt_order() {
        let trace_id = "model-attempt-evidence-test";
        begin_trace(trace_id);
        record(
            trace_id,
            ModelAttemptRecord {
                sequence: 1,
                model_id: "configured-id".to_string(),
                api_model: "provider/model-primary".to_string(),
                provider: "primary-provider".to_string(),
                outcome: "failed".to_string(),
                pending_region_count: 4,
                accepted_region_count: 3,
                unresolved_region_count: 1,
                rejected_region_count: 0,
                first_chunk_ms: Some(10.0),
                first_validated_ms: Some(12.0),
                transport_ms: Some(20.0),
                total_ms: 21.0,
            },
        );
        let attempts = take(trace_id);
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].api_model, "provider/model-primary");
        assert_eq!(attempts[0].provider, "primary-provider");
        assert_eq!(attempts[0].accepted_region_count, 3);
        assert!(take(trace_id).is_empty());
    }
}
