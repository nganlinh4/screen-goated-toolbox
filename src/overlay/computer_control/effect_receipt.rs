//! Structural effect evidence shared by execution, cancellation, delivery, and
//! postcondition reporting. Stronger nested receipts always beat outer wrappers.

use serde_json::{Value, json};

const MAX_RECEIPT_DEPTH: usize = 12;
const MAX_RECEIPT_NODES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::overlay::computer_control) enum EffectStatus {
    Verified,
    MayHaveOccurred,
    ProvenNoEffect,
    Unknown,
}

#[derive(Default)]
struct Evidence {
    verified: bool,
    may_have_occurred: bool,
    proven_no_effect: bool,
}

impl EffectStatus {
    pub(in crate::overlay::computer_control) fn from_value(value: &Value) -> Self {
        let mut evidence = Evidence::default();
        let mut remaining = MAX_RECEIPT_NODES;
        collect(value, 0, &mut remaining, &mut evidence);
        if evidence.verified {
            Self::Verified
        } else if evidence.may_have_occurred {
            Self::MayHaveOccurred
        } else if evidence.proven_no_effect {
            Self::ProvenNoEffect
        } else {
            Self::Unknown
        }
    }

    /// Dispatch returned, so an otherwise silent result is ambiguous rather
    /// than proof that no effect happened.
    pub(in crate::overlay::computer_control) fn after_dispatch(
        value: &Value,
        mutating: bool,
    ) -> Self {
        match (Self::from_value(value), mutating) {
            (Self::Unknown, true) => Self::MayHaveOccurred,
            (status, _) => status,
        }
    }

    pub(in crate::overlay::computer_control) fn is_verified(self) -> bool {
        self == Self::Verified
    }

    pub(in crate::overlay::computer_control) fn is_maybe(self) -> bool {
        self == Self::MayHaveOccurred
    }

    pub(in crate::overlay::computer_control) fn is_proven_no_effect(self) -> bool {
        self == Self::ProvenNoEffect
    }

    pub(in crate::overlay::computer_control) fn may_have_occurred(self) -> Option<bool> {
        match self {
            Self::Verified | Self::MayHaveOccurred => Some(true),
            Self::ProvenNoEffect => Some(false),
            Self::Unknown => None,
        }
    }

    pub(in crate::overlay::computer_control) fn executed(self) -> Option<bool> {
        match self {
            Self::Verified => Some(true),
            Self::ProvenNoEffect => Some(false),
            Self::MayHaveOccurred | Self::Unknown => None,
        }
    }

    pub(in crate::overlay::computer_control) fn code(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::MayHaveOccurred => "may_have_occurred",
            Self::ProvenNoEffect => "proven_no_effect",
            Self::Unknown => "unknown",
        }
    }

    pub(in crate::overlay::computer_control) fn annotate(self, value: &mut Value) {
        value["effect_status"] = json!(self.code());
        value["effect_may_have_occurred"] =
            self.may_have_occurred().map_or(Value::Null, Value::Bool);
        value["effect_verified"] = json!(self.is_verified());
        value["executed"] = self.executed().map_or(Value::Null, Value::Bool);
    }
}

fn collect(value: &Value, depth: usize, remaining: &mut usize, evidence: &mut Evidence) {
    if depth >= MAX_RECEIPT_DEPTH || *remaining == 0 || evidence.verified {
        return;
    }
    *remaining -= 1;
    match value {
        Value::Object(fields) => {
            collect_object(fields, evidence);
            for child in fields.values() {
                collect(child, depth + 1, remaining, evidence);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect(child, depth + 1, remaining, evidence);
            }
        }
        _ => {}
    }
}

fn collect_object(fields: &serde_json::Map<String, Value>, evidence: &mut Evidence) {
    if fields.get("effect_verified").and_then(Value::as_bool) == Some(true) {
        evidence.verified = true;
    }
    match fields
        .get("effect_may_have_occurred")
        .and_then(Value::as_bool)
    {
        Some(true) => evidence.may_have_occurred = true,
        Some(false) => evidence.proven_no_effect = true,
        None => {}
    }
    match fields.get("dispatch_ok").and_then(Value::as_bool) {
        Some(true) => evidence.may_have_occurred = true,
        Some(false) => evidence.proven_no_effect = true,
        None => {}
    }
    match fields.get("executed").and_then(Value::as_bool) {
        Some(true) => evidence.may_have_occurred = true,
        Some(false) => evidence.proven_no_effect = true,
        None => {}
    }
    if let Some(injection) = fields.get("input_injection").and_then(Value::as_object) {
        let inserted = injection
            .get("inserted")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if injection.get("fully_inserted").and_then(Value::as_bool) == Some(true) || inserted > 0 {
            evidence.may_have_occurred = true;
        } else if injection.get("requested").and_then(Value::as_u64) > Some(0) {
            evidence.proven_no_effect = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_verified_receipt_beats_cancel_wrapper() {
        let mut response = json!({
            "cancelled": true,
            "executed": false,
            "action_result": {
                "ok": false,
                "effect_verified": true,
                "effect_may_have_occurred": true,
            },
        });
        let status = EffectStatus::from_value(&response);
        assert_eq!(status, EffectStatus::Verified);
        status.annotate(&mut response);
        assert_eq!(response["executed"], true);
        assert_eq!(response["effect_status"], "verified");
    }

    #[test]
    fn partial_injection_is_ambiguous_not_verified() {
        let status = EffectStatus::from_value(&json!({
            "input_injection": {"requested": 3, "inserted": 1, "fully_inserted": false}
        }));
        assert_eq!(status, EffectStatus::MayHaveOccurred);
        assert_eq!(status.executed(), None);
    }

    #[test]
    fn explicit_zero_dispatch_is_proven_no_effect() {
        let status = EffectStatus::from_value(&json!({
            "dispatch_ok": false,
            "effect_may_have_occurred": false,
        }));
        assert_eq!(status, EffectStatus::ProvenNoEffect);
        assert_eq!(status.executed(), Some(false));
    }

    #[test]
    fn unknown_annotation_stays_unknown() {
        let mut response = json!({"cancelled": true});
        EffectStatus::Unknown.annotate(&mut response);
        assert_eq!(response["effect_may_have_occurred"], Value::Null);
        assert_eq!(EffectStatus::from_value(&response), EffectStatus::Unknown);
    }

    #[test]
    fn silent_mutation_dispatch_is_ambiguous_not_a_no_op() {
        let receipt = json!({"ok": false, "error": "readback unavailable"});
        assert_eq!(
            EffectStatus::after_dispatch(&receipt, true),
            EffectStatus::MayHaveOccurred
        );
        assert_eq!(
            EffectStatus::after_dispatch(&receipt, false),
            EffectStatus::Unknown
        );
    }
}
