//! Session-trace copy of a completed receipt that the model never received.
//!
//! A cancelled or superseded job can still have finished (for example a
//! command that exited just as cancellation arrived). The content-free
//! `action_result_dropped` event says so; this sensitive companion keeps the
//! receipt itself in the per-session trace so the outcome can be inspected
//! later. It uses the same wire budget as a delivered tool response.

use serde_json::{Value, json};

use super::super::response_budget;
use super::super::telemetry::{self, ActionTrace, Privacy};

pub(super) fn record(action: ActionTrace, id: &str, name: &str, reason: &str, response: &Value) {
    telemetry::event_for_action(
        "action_result_dropped_receipt",
        "runtime",
        Privacy::Sensitive,
        action,
        fields(id, name, reason, response),
    );
}

fn fields(id: &str, name: &str, reason: &str, response: &Value) -> Value {
    let receipt = match response_budget::prepare(id, name, response.clone()) {
        Ok(prepared) => prepared.delivered,
        Err(_) => json!({"result_omitted": true, "reason": "receipt identity exceeds the budget"}),
    };
    json!({
        "tool_call_id": id,
        "tool": name,
        "reason": reason,
        "receipt": receipt,
    })
}

#[cfg(test)]
mod tests {
    use super::fields;
    use serde_json::json;

    #[test]
    fn a_dropped_receipt_keeps_its_exact_command_outcome() {
        let response = json!({
            "ok": true,
            "execution_ok": true,
            "exit_code": 3,
            "stdout": "가나다 done",
            "effect_status": "effect_may_have_occurred",
        });
        let recorded = fields(
            "call-1",
            "run_command",
            "cancelled_or_superseded",
            &response,
        );
        assert_eq!(recorded["tool_call_id"], "call-1");
        assert_eq!(recorded["tool"], "run_command");
        assert_eq!(recorded["reason"], "cancelled_or_superseded");
        assert_eq!(recorded["receipt"], response);
    }

    #[test]
    fn an_oversized_dropped_receipt_is_bounded_and_keeps_its_status() {
        let response = json!({"ok": false, "execution_ok": true, "stdout": "x".repeat(200_000)});
        let recorded = fields("call-2", "run_command", "stale_job_identity", &response);
        assert_eq!(recorded["receipt"]["result_omitted"], true);
        assert_eq!(recorded["receipt"]["execution_ok"], true);
        assert!(recorded.to_string().len() < 64 * 1024);
    }
}
