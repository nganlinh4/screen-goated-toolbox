//! Tool-call identity hygiene for the live reader: a bounded tombstone set of
//! provider-cancelled call ids, and the single capped entry point for not-run
//! responses that wait while another action owns the executor.

use std::collections::{HashSet, VecDeque};

use serde_json::Value;

use super::super::overlay;
use super::super::telemetry;
use super::reader::Reader;

/// Cancelled ids remembered for the whole run (oldest evicted first). Provider
/// ids are random per call, so a bounded window covers every realistic replay.
pub(super) const MAX_CANCELLED_TOOL_IDS: usize = 256;

/// Not-run responses held while one action owns the executor. A well-behaved
/// model issues a handful of parallel calls; more than this is a provider
/// protocol violation that the runtime answers by replacing the transport.
pub(super) const MAX_QUEUED_TOOL_RESPONSES: usize = 64;

/// Bounded FIFO set of ids named by `toolCallCancellation`.
#[derive(Default)]
pub(super) struct CancelledToolIds {
    order: VecDeque<String>,
    ids: HashSet<String>,
}

impl CancelledToolIds {
    pub(super) fn record(&mut self, ids: &[String]) {
        for id in ids {
            if self.ids.contains(id) {
                continue;
            }
            if self.order.len() >= MAX_CANCELLED_TOOL_IDS
                && let Some(evicted) = self.order.pop_front()
            {
                self.ids.remove(&evicted);
            }
            self.ids.insert(id.clone());
            self.order.push_back(id.clone());
        }
    }

    pub(super) fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
}

/// Queue a not-run response, or reject the call when the queue is full. A
/// rejected call never runs; the caller's loop replaces the transport, which
/// discards every id of the misbehaving connection.
pub(super) fn enqueue(state: &mut Reader, id: &str, name: &str, response: Value) -> bool {
    let queued = state.immediate_tool_responses.len();
    if queued >= MAX_QUEUED_TOOL_RESPONSES {
        state.tool_response_queue_overflowed = true;
        overlay::push_log(format!(
            "[!] provider sent more than {MAX_QUEUED_TOOL_RESPONSES} concurrent tool calls; {name} did not run and the session will reconnect"
        ));
        telemetry::typed_error(
            "ERR_TOOL_RESPONSE_QUEUE_FULL",
            "turn_policy",
            "rejected a tool call because the not-run response queue is full",
            serde_json::json!({
                "tool": name,
                "queued": queued,
                "max_queued": MAX_QUEUED_TOOL_RESPONSES,
            }),
        );
        return false;
    }
    state
        .immediate_tool_responses
        .push_back((id.to_string(), name.to_string(), response));
    true
}

/// True once per overflow; the socket loop then replaces the transport.
pub(super) fn take_queue_overflow(state: &mut Reader) -> bool {
    std::mem::take(&mut state.tool_response_queue_overflowed)
}

/// A call whose id the provider already cancelled must neither run nor receive
/// a response, however late it is re-sent. Returns true when it was dropped.
pub(super) fn drop_cancelled_reuse(state: &Reader, id: &str, name: &str) -> bool {
    if !state.cancelled_tool_ids.contains(id) {
        return false;
    }
    overlay::push_log(format!(
        "[~] ignored {name}: its call id was already cancelled"
    ));
    telemetry::typed_error(
        "ERR_TOOL_CALL_ID_CANCELLED",
        "turn_policy",
        "dropped a tool call that reused a provider-cancelled call id",
        serde_json::json!({"tool": name, "tool_call_id": id}),
    );
    true
}

#[cfg(test)]
#[path = "tool_call_ids_tests.rs"]
mod tests;
