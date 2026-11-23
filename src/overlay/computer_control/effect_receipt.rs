//! Structural effect evidence shared by execution, cancellation, delivery, and
//! postcondition reporting. Opaque payload data never supplies execution proof.

use serde_json::{Value, json};

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
        let Some(fields) = value.as_object() else {
            return Self::Unknown;
        };
        if fields.get("effect_scope").and_then(Value::as_str) == Some("batch") {
            return fields
                .get("receipts")
                .and_then(Value::as_array)
                .map_or(Self::MayHaveOccurred, |receipts| batch_status(receipts));
        }
        let mut evidence = Evidence::default();
        collect_object(fields, &mut evidence);
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

/// Aggregate independent action receipts without allowing one verified step to
/// hide an uncertain effect from another step.
pub(in crate::overlay::computer_control) fn batch_status(receipts: &[Value]) -> EffectStatus {
    let mut verified = false;
    for receipt in receipts {
        if receipt.get("effect_verified").and_then(Value::as_bool) == Some(true) {
            verified = true;
        } else if receipt
            .get("effect_may_have_occurred")
            .and_then(Value::as_bool)
            != Some(false)
        {
            return EffectStatus::MayHaveOccurred;
        }
    }
    if verified {
        EffectStatus::Verified
    } else {
        EffectStatus::ProvenNoEffect
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
    fn untrusted_payload_receipt_fields_do_not_change_host_certainty() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/phone-control/effect-receipt-boundary.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            for placement in fixture["placements"].as_array().unwrap() {
                let placement = placement.as_str().unwrap();
                let mut response = case["host"].clone();
                response[placement] = if placement == "items" {
                    json!([case["payload"].clone()])
                } else {
                    case["payload"].clone()
                };
                assert_eq!(
                    EffectStatus::after_dispatch(&response, case["mutating"].as_bool().unwrap())
                        .code(),
                    case["expected"].as_str().unwrap(),
                    "placement={placement};case={case}",
                );
            }
        }
    }

    #[test]
    fn shared_batch_certainty_cases_apply_to_the_host_batch() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/phone-control/batch-effect-certainty.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let receipts: Vec<Value> = case["statuses"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| match value.as_str() {
                    Some("verified") => {
                        json!({"effect_verified": true, "effect_may_have_occurred": true})
                    }
                    Some("proven_no_effect") => {
                        json!({"effect_verified": false, "effect_may_have_occurred": false})
                    }
                    Some("may_have_occurred") => {
                        json!({"effect_verified": false, "effect_may_have_occurred": true})
                    }
                    _ => json!({}),
                })
                .collect();
            let expected = case["expected"].as_str().unwrap();
            assert_eq!(batch_status(&receipts).code(), expected, "{case}");
            let batch = json!({"effect_scope": "batch", "receipts": receipts});
            assert_eq!(EffectStatus::from_value(&batch).code(), expected, "{case}");
        }
    }

    #[test]
    fn verified_batch_flag_cannot_hide_uncertain_step() {
        let response = json!({
            "effect_verified": true,
            "effect_scope": "batch", "receipts": [
                {"effect_verified": true, "effect_may_have_occurred": true},
                {"effect_verified": false, "effect_may_have_occurred": true}
            ]
        });
        assert_eq!(
            EffectStatus::from_value(&response),
            EffectStatus::MayHaveOccurred
        );
    }

    #[test]
    fn annotated_cancel_wrapper_keeps_host_certainty() {
        let mut response = json!({
            "cancelled": true,
            "executed": false,
            "action_result": {
                "ok": false,
                "effect_verified": false,
                "effect_may_have_occurred": false,
            },
        });
        EffectStatus::Verified.annotate(&mut response);
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
    fn incomplete_host_batch_cannot_claim_verification_or_no_effect() {
        for receipts in [Value::Null, json!({}), json!(false)] {
            let response = json!({"effect_scope":"batch", "receipts":receipts, "effect_verified":true, "executed":false});
            assert_eq!(
                EffectStatus::from_value(&response),
                EffectStatus::MayHaveOccurred
            );
        }
        assert_eq!(
            EffectStatus::from_value(&json!({"effect_scope":"batch","effect_verified":true})),
            EffectStatus::MayHaveOccurred
        );
    }

    #[test]
    fn payload_arrays_do_not_supply_effect_evidence() {
        let response = json!([{"effect_verified":true,"executed":false}]);
        assert_eq!(
            EffectStatus::after_dispatch(&response, true),
            EffectStatus::MayHaveOccurred
        );
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
