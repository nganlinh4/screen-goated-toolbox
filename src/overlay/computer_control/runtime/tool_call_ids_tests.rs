use std::sync::mpsc;

use serde_json::json;

use super::super::super::protocol::ServerEvent;
use super::super::reader::{Pending, Reader, handle_event};
use super::*;

fn call(id: &str) -> ServerEvent {
    ServerEvent::ToolCall {
        id: id.into(),
        name: "future_tool".into(),
        args: json!({}),
    }
}

#[test]
fn a_reused_cancelled_call_id_never_runs_or_receives_a_response() {
    let mut state = Reader::default();
    let (sender, receiver) = mpsc::channel();
    handle_event(
        ServerEvent::ToolCancellation(vec!["cancelled".into()]),
        None,
        &sender,
        &mut state,
    );
    handle_event(call("cancelled"), None, &sender, &mut state);
    assert!(receiver.try_recv().is_err());
    assert!(state.immediate_tool_responses.is_empty());
    assert!(state.pending.id.is_none());

    // A fresh id still reaches the executor through the same path.
    handle_event(call("fresh"), None, &sender, &mut state);
    assert_eq!(receiver.try_recv().unwrap().id, "fresh");
}

#[test]
fn a_reused_cancelled_call_id_gets_no_terminal_drain_response() {
    let mut state = Reader {
        terminal_drain: true,
        terminal_accepted: true,
        ..Reader::default()
    };
    let (sender, receiver) = mpsc::channel();
    handle_event(
        ServerEvent::ToolCancellation(vec!["cancelled".into()]),
        None,
        &sender,
        &mut state,
    );
    handle_event(call("cancelled"), None, &sender, &mut state);
    assert!(state.immediate_tool_responses.is_empty());
    handle_event(call("fresh"), None, &sender, &mut state);
    assert_eq!(state.immediate_tool_responses.len(), 1);
    assert_eq!(state.immediate_tool_responses[0].0, "fresh");
    assert!(receiver.try_recv().is_err());
}

#[test]
fn cancelled_ids_are_bounded_and_evict_the_oldest_first() {
    let mut ids = CancelledToolIds::default();
    let all: Vec<String> = (0..=MAX_CANCELLED_TOOL_IDS)
        .map(|index| format!("id-{index}"))
        .collect();
    ids.record(&all[..2]);
    // Re-recording an id is idempotent and does not consume capacity.
    ids.record(&all[..2]);
    ids.record(&all[2..]);
    assert_eq!(ids.order.len(), MAX_CANCELLED_TOOL_IDS);
    assert_eq!(ids.ids.len(), MAX_CANCELLED_TOOL_IDS);
    assert!(!ids.contains("id-0"));
    assert!(ids.contains("id-1"));
    assert!(ids.contains(&format!("id-{MAX_CANCELLED_TOOL_IDS}")));
}

#[test]
fn a_full_response_queue_rejects_the_excess_call_and_requests_replacement() {
    let mut state = Reader {
        pending: Pending {
            id: Some("owner".into()),
            tool: Some("future_tool".into()),
            ..Pending::default()
        },
        ..Reader::default()
    };
    let (sender, receiver) = mpsc::channel();
    for index in 0..=MAX_QUEUED_TOOL_RESPONSES {
        handle_event(call(&format!("later-{index}")), None, &sender, &mut state);
    }
    assert_eq!(
        state.immediate_tool_responses.len(),
        MAX_QUEUED_TOOL_RESPONSES
    );
    let excess = format!("later-{MAX_QUEUED_TOOL_RESPONSES}");
    assert!(
        state
            .immediate_tool_responses
            .iter()
            .all(|(id, _, response)| *id != excess && response["executed"] == false)
    );
    assert!(take_queue_overflow(&mut state));
    assert!(!take_queue_overflow(&mut state));
    assert!(receiver.try_recv().is_err());
    assert_eq!(state.pending.id.as_deref(), Some("owner"));
}
