//! Bounded, content-free tool outcome ledger used to reseed a replaced Live session.

use std::collections::VecDeque;

use serde_json::Value;

const MAX_OUTCOMES: usize = 10;
const MAX_TOOL_NAME_CHARS: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutcomeStatus {
    DeliveredOk,
    DeliveredFailed,
    DeliveredBlocked,
    DeliveredCancelled,
    DeliveredUnknown,
    InterruptedResultUnknown,
    TransportInterruptedUnknown,
}

impl OutcomeStatus {
    fn label(self) -> &'static str {
        match self {
            Self::DeliveredOk => "delivered_ok",
            Self::DeliveredFailed => "delivered_failed",
            Self::DeliveredBlocked => "delivered_blocked",
            Self::DeliveredCancelled => "delivered_cancelled",
            Self::DeliveredUnknown => "delivered_result_unknown",
            Self::InterruptedResultUnknown => "interrupted_result_unknown",
            Self::TransportInterruptedUnknown => "transport_interrupted_result_unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToolOutcome {
    tool: String,
    status: OutcomeStatus,
}

/// Contains only a compact tool identifier and structural delivery state. Tool
/// arguments, observations, error text, and response bodies never enter it.
#[derive(Debug, Default)]
pub(super) struct ToolOutcomeLedger {
    entries: VecDeque<ToolOutcome>,
}

impl ToolOutcomeLedger {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Record a result only after its tool response was accepted by the current
    /// transport. `ok` describes the delivered tool result, not task completion.
    pub(super) fn record_delivered(&mut self, tool: &str, response: &Value) {
        let cancelled = response
            .get("cancelled")
            .and_then(Value::as_bool)
            .or_else(|| {
                response
                    .pointer("/action_result/cancelled")
                    .and_then(Value::as_bool)
            })
            == Some(true);
        let ok = response.get("ok").and_then(Value::as_bool);
        let ok = ok.or_else(|| {
            response
                .pointer("/action_result/ok")
                .and_then(Value::as_bool)
        });
        let blocked = response
            .get("status")
            .and_then(Value::as_str)
            .or_else(|| {
                response
                    .pointer("/action_result/status")
                    .and_then(Value::as_str)
            })
            .is_some_and(|status| status.starts_with("blocked_"));
        let status = if cancelled {
            OutcomeStatus::DeliveredCancelled
        } else if blocked {
            OutcomeStatus::DeliveredBlocked
        } else {
            match ok {
                Some(true) => OutcomeStatus::DeliveredOk,
                Some(false) => OutcomeStatus::DeliveredFailed,
                None => OutcomeStatus::DeliveredUnknown,
            }
        };
        self.push(tool, status);
    }

    /// The worker may have crossed an effect boundary before transport
    /// replacement. Cancellation proves only that further work was requested to
    /// stop, so reconnect must inspect current state rather than claim success or
    /// safely retry.
    pub(super) fn record_transport_interruption(&mut self, tool: &str) {
        self.push(tool, OutcomeStatus::TransportInterruptedUnknown);
    }

    pub(super) fn record_interrupted_effect(&mut self, tool: &str) {
        self.push(tool, OutcomeStatus::InterruptedResultUnknown);
    }

    fn reconnect_lines(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| format!("{}={}", entry.tool, entry.status.label()))
            .collect()
    }

    /// Keep the newest complete entries that fit. Never slice an identifier or
    /// status into ambiguous reconnect text.
    pub(super) fn reconnect_summary(&self, max_chars: usize) -> String {
        let mut selected = Vec::new();
        let mut used = 0;
        for line in self.reconnect_lines().into_iter().rev() {
            let added = line.chars().count() + usize::from(!selected.is_empty()) * 2;
            if used + added > max_chars {
                break;
            }
            used += added;
            selected.push(line);
        }
        selected.reverse();
        selected.join(", ")
    }

    #[cfg(test)]
    pub(super) fn has_transport_uncertainty(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.status == OutcomeStatus::TransportInterruptedUnknown)
    }

    pub(super) fn clear_interruption_uncertainty(&mut self) {
        self.entries.retain(|entry| {
            !matches!(
                entry.status,
                OutcomeStatus::InterruptedResultUnknown
                    | OutcomeStatus::TransportInterruptedUnknown
            )
        });
    }

    fn push(&mut self, tool: &str, status: OutcomeStatus) {
        self.entries.push_back(ToolOutcome {
            tool: structural_tool_name(tool),
            status,
        });
        while self.entries.len() > MAX_OUTCOMES {
            self.entries.pop_front();
        }
    }
}

fn structural_tool_name(name: &str) -> String {
    let mut result = String::new();
    for ch in name.chars().take(MAX_TOOL_NAME_CHARS) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':') {
            result.push(ch);
        } else {
            result.push('_');
        }
    }
    if result.is_empty() {
        "unknown_tool".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ledger_distinguishes_delivery_states_without_retaining_content() {
        let mut ledger = ToolOutcomeLedger::default();
        ledger.record_delivered(
            "future.read",
            &json!({"ok": true, "reading": "private-observation"}),
        );
        ledger.record_delivered(
            "future.write",
            &json!({"ok": false, "error": "private-error"}),
        );
        ledger.record_delivered(
            "future.guard",
            &json!({"ok": false, "status": "blocked_structural_invariant"}),
        );
        ledger.record_delivered("future.cancel", &json!({"ok": false, "cancelled": true}));
        ledger.record_transport_interruption("future pending");

        let recap = ledger.reconnect_summary(1000);
        assert!(recap.contains("future.read=delivered_ok"));
        assert!(recap.contains("future.write=delivered_failed"));
        assert!(recap.contains("future.guard=delivered_blocked"));
        assert!(recap.contains("future.cancel=delivered_cancelled"));
        assert!(recap.contains("future_pending=transport_interrupted_result_unknown"));
        assert!(!recap.contains("private-observation"));
        assert!(!recap.contains("private-error"));
    }

    #[test]
    fn ledger_is_bounded_to_recent_structural_outcomes() {
        let mut ledger = ToolOutcomeLedger::default();
        for index in 0..(MAX_OUTCOMES + 3) {
            ledger.record_delivered(&format!("operation_{index}"), &json!({"ok": true}));
        }
        let lines = ledger.reconnect_summary(1000);
        assert_eq!(lines.matches("operation_").count(), MAX_OUTCOMES);
        assert!(!lines.contains("operation_0="));
        assert!(lines.contains("operation_12="));
    }

    #[test]
    fn reconnect_summary_prefers_recent_complete_entries() {
        let mut ledger = ToolOutcomeLedger::default();
        ledger.record_delivered("first_operation", &json!({"ok": true}));
        ledger.record_delivered("second_operation", &json!({"ok": false}));
        let summary = ledger.reconnect_summary(50);
        assert_eq!(summary, "second_operation=delivered_failed");
    }

    #[test]
    fn resolved_transport_uncertainty_is_removed_without_touching_other_outcomes() {
        let mut ledger = ToolOutcomeLedger::default();
        ledger.record_delivered("reader", &json!({"ok": true}));
        ledger.record_transport_interruption("writer");
        assert!(ledger.has_transport_uncertainty());

        ledger.clear_interruption_uncertainty();

        assert!(!ledger.has_transport_uncertainty());
        assert_eq!(ledger.reconnect_summary(1000), "reader=delivered_ok");
    }
}
