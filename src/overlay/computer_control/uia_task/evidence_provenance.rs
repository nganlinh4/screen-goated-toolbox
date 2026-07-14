//! Trust origin for facts supplied to the independent completion verifier.

use std::borrow::Cow;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EvidenceProvenance {
    /// Immutable surface that supplied the first model-visible frame of a job.
    JobSource,
    /// First exact surface observed for a provider during the job.
    ProviderSource,
    /// Output returned by a capability implementation. Its `ok` field proves
    /// execution/delivery, not every semantic claim nested in the output.
    CapabilityResult,
    /// Natural-language interpretation produced by an auxiliary model.
    ModelInference,
    /// Output computed by code supplied by the acting model. The execution is
    /// real, but the code can return constants unrelated to provider state.
    ModelAuthoredComputation,
    /// An effect was executed after model-derived localization. Effect metadata
    /// is usable; inferred target labels are advisory.
    ModelMediatedEffect,
    /// Exact surface metadata observed while grounding after a capability.
    GroundedSurface,
}

impl EvidenceProvenance {
    pub(super) fn for_dispatch(tool: &str) -> Self {
        if tool == "run_command" {
            Self::ModelAuthoredComputation
        } else {
            Self::CapabilityResult
        }
    }

    pub(super) fn needs_request_lineage(self) -> bool {
        self == Self::ModelAuthoredComputation
    }

    /// Higher-ranked entries may displace lower-ranked advice, never vice versa.
    pub(super) fn retention_rank(self) -> u8 {
        match self {
            Self::JobSource | Self::ProviderSource => 4,
            Self::CapabilityResult => 3,
            Self::GroundedSurface => 2,
            Self::ModelMediatedEffect => 1,
            Self::ModelInference | Self::ModelAuthoredComputation => 0,
        }
    }

    /// Category floors keep source identity, several direct facts, and the two
    /// latest grounded postconditions simultaneously available within the cap.
    pub(super) fn retention_floor(self) -> usize {
        match self {
            Self::JobSource | Self::ProviderSource => 1,
            Self::CapabilityResult => 4,
            Self::GroundedSurface => 2,
            Self::ModelInference | Self::ModelAuthoredComputation | Self::ModelMediatedEffect => 0,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::JobSource => "job_source",
            Self::ProviderSource => "provider_source",
            Self::CapabilityResult => "capability_result",
            Self::ModelInference => "model_inference",
            Self::ModelAuthoredComputation => "model_authored_computation",
            Self::ModelMediatedEffect => "model_mediated_effect",
            Self::GroundedSurface => "grounded_surface",
        }
    }

    pub(super) fn verifier_result_with_request<'a>(
        self,
        request: Option<&Value>,
        result: &'a Value,
    ) -> Cow<'a, Value> {
        if self != Self::ModelAuthoredComputation {
            return Cow::Borrowed(result);
        }
        Cow::Owned(json!({
            "request_lineage": request_lineage(request),
            "execution_receipt": structural_receipt(result),
            "model_authored_output_withheld": true,
        }))
    }
}

fn request_lineage(request: Option<&Value>) -> Value {
    let encoded = request.map(Value::to_string).unwrap_or_default();
    let digest = Sha256::digest(encoded.as_bytes());
    let sha256: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    let fields = request
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    json!({
        "model_authored": true,
        "sha256": sha256,
        "byte_count": encoded.len(),
        "fields": fields,
    })
}

fn structural_receipt(result: &Value) -> Value {
    const FIELDS: &[&str] = &[
        "ok",
        "code",
        "status",
        "executed",
        "dispatch_ok",
        "effect_verified",
        "effect_may_have_occurred",
        "cancelled",
        "timed_out",
        "stale",
        "exit_code",
        "terminated",
        "reaped",
        "dry_run",
        "target_tab_id",
        "target_pinned",
    ];
    let mut receipt = serde_json::Map::new();
    if let Some(result) = result.as_object() {
        for field in FIELDS {
            if let Some(value) = result.get(*field) {
                receipt.insert((*field).to_string(), value.clone());
            }
        }
    }
    Value::Object(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_authored_output_is_withheld_from_completion_evidence() {
        let request = json!({"command": "Write-Output invented-marker"});
        let output = json!({
            "ok": true,
            "exit_code": 0,
            "target_tab_id": 41,
            "stdout": "invented-marker",
        });
        let safe = EvidenceProvenance::ModelAuthoredComputation
            .verifier_result_with_request(Some(&request), &output);
        assert_eq!(safe["execution_receipt"]["ok"], true);
        assert_eq!(safe["execution_receipt"]["exit_code"], 0);
        assert_eq!(safe["execution_receipt"]["target_tab_id"], 41);
        assert_eq!(safe["request_lineage"]["model_authored"], true);
        assert_eq!(safe["request_lineage"]["fields"], json!(["command"]));
        assert_eq!(safe["model_authored_output_withheld"], true);
        assert!(!safe.to_string().contains("invented-marker"));
        assert!(matches!(
            EvidenceProvenance::CapabilityResult.verifier_result_with_request(None, &output),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn dispatch_classifies_shell_output_as_model_authored() {
        assert_eq!(
            EvidenceProvenance::for_dispatch("run_command"),
            EvidenceProvenance::ModelAuthoredComputation
        );
        assert_eq!(
            EvidenceProvenance::for_dispatch("future_capability"),
            EvidenceProvenance::CapabilityResult
        );
    }
}
