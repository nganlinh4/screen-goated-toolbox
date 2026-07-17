//! Effect-aware wrappers for interrupted actions and unavailable grounding.

use serde_json::{Value, json};

use super::super::effect_receipt::EffectStatus;
use super::super::uia_task::FrameSource;

type ActionFrame = Option<(String, FrameSource)>;

pub(super) fn cancelled(stage: &str, effect_status: EffectStatus) -> (Value, ActionFrame) {
    let mut response = json!({
        "ok": false,
        "status": "aborted_by_user",
        "cancelled": true,
        "stage": stage,
        "postcondition": cancelled_postcondition(effect_status),
    });
    effect_status.annotate(&mut response);
    (response, None)
}

pub(super) fn cancelled_after_dispatch(
    action_result: Value,
    mutating: bool,
) -> (Value, ActionFrame) {
    let effect_status = EffectStatus::after_dispatch(&action_result, mutating);
    let (mut response, frame) = cancelled("after_dispatch", effect_status);
    response["action_result"] = action_result;
    (response, frame)
}

pub(super) fn unavailable_postcondition(effect_status: EffectStatus) -> Value {
    match effect_status {
        EffectStatus::Verified => json!({
            "ok": true,
            "status": "confirmed",
            "effect": "verified_by_receipt",
            "reason": "grounding_unavailable_after_verified_effect",
        }),
        EffectStatus::MayHaveOccurred => json!({
            "ok": false,
            "status": "unavailable",
            "confirmed": false,
            "effect": "may_have_occurred",
            "reason": "grounding_unavailable_after_dispatch",
            "retryable": false,
        }),
        EffectStatus::ProvenNoEffect => json!({
            "ok": false,
            "status": "not_run",
            "confirmed": true,
            "effect": "none",
            "reason": "effect_did_not_start",
            "retryable": true,
        }),
        EffectStatus::Unknown => json!({
            "ok": false,
            "status": "unavailable",
            "confirmed": false,
            "effect": "unknown",
            "reason": "grounding_unavailable",
            "retryable": false,
        }),
    }
}

pub(super) fn proven_no_effect_after_dispatch(
    action_result: Value,
    queue_ms: u128,
    dispatch_ms: u128,
) -> (Value, ActionFrame) {
    let execution_ok = action_result.get("ok").and_then(Value::as_bool);
    let effect_status = EffectStatus::ProvenNoEffect;
    let mut response = json!({
        "ok": execution_ok.unwrap_or(false),
        "execution_ok": execution_ok,
        "action_result": action_result,
        "grounding": {
            "performed": false,
            "reason": "dispatch_proved_no_effect",
        },
        "timing": {
            "queue_ms": queue_ms,
            "dispatch_and_settle_ms": dispatch_ms,
            "ground_ms": 0,
        },
        "postcondition": unavailable_postcondition(effect_status),
    });
    effect_status.annotate(&mut response);
    (response, None)
}

pub(super) fn complete_observation_after_dispatch(
    action_result: Value,
    queue_ms: u128,
    dispatch_ms: u128,
) -> (Value, ActionFrame) {
    let execution_ok = action_result.get("ok").and_then(Value::as_bool);
    let effect_status = EffectStatus::after_dispatch(&action_result, false);
    let mut response = json!({
        "ok": execution_ok.unwrap_or(false),
        "execution_ok": execution_ok,
        "action_result": action_result,
        "grounding": {
            "performed": false,
            "reason": "capability_result_is_complete",
        },
        "timing": {
            "queue_ms": queue_ms,
            "dispatch_and_settle_ms": dispatch_ms,
            "ground_ms": 0,
        },
        "postcondition": {
            "status": "not_applicable",
            "effect": "observation_or_query",
        },
    });
    effect_status.annotate(&mut response);
    (response, None)
}

pub(super) fn complete_structured_result_after_dispatch(
    action_result: Value,
    queue_ms: u128,
    dispatch_ms: u128,
    effect_status: EffectStatus,
) -> (Value, ActionFrame) {
    let execution_ok = action_result.get("ok").and_then(Value::as_bool);
    let exact_process = action_result.get("evidence_kind").and_then(Value::as_str)
        == Some("exact_process_invocation");
    let instruction = if exact_process {
        "Use the structured receipt for this check. Read any affected resource separately before claiming its semantic state."
    } else {
        "Use this capability's bounded structured result. Read affected state separately before claiming anything outside that receipt."
    };
    let postcondition = if effect_status.is_verified() {
        json!({
            "ok": true,
            "status": "confirmed",
            "effect": "verified_by_receipt",
            "reason": "structured_receipt_complete",
        })
    } else {
        json!({
            "status": "not_applicable",
            "confirmed": false,
            "effect": "structured_result_only",
            "reason": "desktop_cannot_verify_structured_result",
            "instruction": instruction,
        })
    };
    let mut response = json!({
        "ok": execution_ok.unwrap_or(false),
        "execution_ok": execution_ok,
        "action_result": action_result,
        "grounding": {
            "performed": false,
            "reason": "structured_result_is_nonvisual",
        },
        "timing": {
            "queue_ms": queue_ms,
            "dispatch_and_settle_ms": dispatch_ms,
            "ground_ms": 0,
        },
        "postcondition": postcondition,
    });
    effect_status.annotate(&mut response);
    (response, None)
}

fn cancelled_postcondition(effect_status: EffectStatus) -> Value {
    match effect_status {
        EffectStatus::Verified => json!({
            "ok": true,
            "status": "confirmed",
            "effect": "verified_by_receipt",
            "reason": "cancelled_after_effect",
        }),
        EffectStatus::MayHaveOccurred => json!({
            "ok": false,
            "status": "inconclusive",
            "confirmed": false,
            "effect": "may_have_occurred",
            "reason": "cancelled_after_dispatch",
            "retryable": false,
        }),
        EffectStatus::ProvenNoEffect => json!({
            "ok": false,
            "status": "not_run",
            "confirmed": true,
            "effect": "none",
            "reason": "cancelled_before_effect",
            "retryable": true,
        }),
        EffectStatus::Unknown => json!({
            "ok": false,
            "status": "inconclusive",
            "confirmed": false,
            "effect": "unknown",
            "reason": "cancelled",
            "retryable": false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_after_verified_mutation_keeps_receipt() {
        let (response, _) = cancelled_after_dispatch(
            json!({
                "ok": true,
                "effect_verified": true,
                "effect_may_have_occurred": true,
            }),
            true,
        );
        assert_eq!(response["cancelled"], true);
        assert_eq!(response["executed"], true);
        assert_eq!(response["effect_verified"], true);
        assert_eq!(response["effect_status"], "verified");
        assert_eq!(response["action_result"]["effect_verified"], true);
        assert_eq!(response["postcondition"]["status"], "confirmed");
    }

    #[test]
    fn unavailable_grounding_does_not_flatten_ambiguous_effect() {
        let response = unavailable_postcondition(EffectStatus::MayHaveOccurred);
        assert_eq!(response["status"], "unavailable");
        assert_eq!(response["effect"], "may_have_occurred");
        assert_eq!(response["retryable"], false);
    }

    #[test]
    fn proven_pre_effect_failure_needs_no_post_action_capture() {
        let (response, frame) = proven_no_effect_after_dispatch(
            json!({
                "ok": false,
                "code": "ERR_OPAQUE_PREFLIGHT",
                "effect_may_have_occurred": false,
            }),
            7,
            11,
        );
        assert!(frame.is_none());
        assert_eq!(response["effect_status"], "proven_no_effect");
        assert_eq!(response["grounding"]["performed"], false);
        assert_eq!(response["timing"]["ground_ms"], 0);
    }

    #[test]
    fn complete_observation_needs_no_duplicate_screen_capture() {
        let (response, frame) = complete_observation_after_dispatch(
            json!({"ok": true, "content": "authoritative result"}),
            3,
            17,
        );
        assert!(frame.is_none());
        assert_eq!(response["ok"], true);
        assert_eq!(response["grounding"]["performed"], false);
        assert_eq!(response["postcondition"]["status"], "not_applicable");
        assert_eq!(response["timing"]["ground_ms"], 0);
    }

    #[test]
    fn structured_receipts_skip_irrelevant_desktop_grounding() {
        let (response, frame) = complete_structured_result_after_dispatch(
            json!({
                "ok": false,
                "evidence_kind": "exact_process_invocation",
                "process_completed": true,
                "exit_code": 1,
            }),
            2,
            19,
            EffectStatus::MayHaveOccurred,
        );
        assert!(frame.is_none());
        assert_eq!(response["grounding"]["performed"], false);
        assert_eq!(response["postcondition"]["status"], "not_applicable");
        assert_eq!(response["effect_status"], "may_have_occurred");
    }

    #[test]
    fn free_form_process_receipt_does_not_claim_authoritative_resource_state() {
        let (response, frame) = complete_structured_result_after_dispatch(
            json!({
                "ok": true,
                "process_completed": true,
                "exit_code": 0,
            }),
            2,
            19,
            EffectStatus::MayHaveOccurred,
        );
        assert!(frame.is_none());
        assert_eq!(response["grounding"]["performed"], false);
        assert!(
            response["postcondition"]["instruction"]
                .as_str()
                .is_some_and(
                    |instruction| instruction.contains("bounded structured result")
                        && instruction.contains("Read affected state separately")
                )
        );
    }
}
