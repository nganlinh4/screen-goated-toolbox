use super::super::super::protocol::ServerEvent;
use super::super::reader::{Pending, Reader, handle_event};
use super::*;
use serde_json::json;

fn queue(state: &mut Reader, id: &str) {
    state.immediate_tool_responses.push_back((
        id.to_owned(),
        "future_tool".into(),
        json!({"executed":false}),
    ));
}

#[test]
fn cancellation_prunes_only_matching_queued_responses_and_is_idempotent() {
    let mut state = Reader::default();
    for id in ["cancelled", "retained", "Cancelled", "cancelled"] {
        queue(&mut state, id);
    }
    let (sender, receiver) = mpsc::channel();
    for _ in 0..2 {
        handle_event(
            ServerEvent::ToolCancellation(vec!["cancelled".into(), "unknown".into()]),
            None,
            &sender,
            &mut state,
        );
        assert_eq!(
            state
                .immediate_tool_responses
                .iter()
                .map(|response| response.0.as_str())
                .collect::<Vec<_>>(),
            ["retained", "Cancelled"]
        );
    }
    handle_event(
        ServerEvent::ToolCancellation(Vec::new()),
        None,
        &sender,
        &mut state,
    );
    assert_eq!(state.immediate_tool_responses.len(), 2);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn cancellation_retires_queued_ids_without_cancelling_an_unmatched_owner() {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut state = Reader {
        pending: Pending {
            id: Some("owner".into()),
            tool: Some("future_tool".into()),
            cancel: Some(cancel.clone()),
            ..Pending::default()
        },
        ..Reader::default()
    };
    queue(&mut state, "later");
    let (sender, _) = mpsc::channel();
    handle_event(
        ServerEvent::ToolCancellation(vec!["later".into()]),
        None,
        &sender,
        &mut state,
    );
    assert!(state.immediate_tool_responses.is_empty());
    assert!(!cancel.load(Ordering::SeqCst));
    assert!(!state.pending.cancelled);
    assert_eq!(state.pending.id.as_deref(), Some("owner"));
    queue(&mut state, "cancelled-later");
    handle_event(
        ServerEvent::ToolCancellation(vec!["owner".into(), "cancelled-later".into()]),
        None,
        &sender,
        &mut state,
    );
    assert!(state.immediate_tool_responses.is_empty());
    assert!(cancel.load(Ordering::SeqCst));
    assert!(state.pending.cancelled);
    // Ownership remains until the executor's actual result settles.
    assert_eq!(state.pending.id.as_deref(), Some("owner"));
}

#[test]
fn terminal_drain_cancellation_prunes_the_late_not_run_response() {
    let mut state = Reader {
        terminal_drain: true,
        terminal_accepted: true,
        ..Reader::default()
    };
    let (sender, receiver) = mpsc::channel();
    for id in ["cancelled", "retained"] {
        handle_event(
            ServerEvent::ToolCall {
                id: id.into(),
                name: "future_tool".into(),
                args: json!({}),
            },
            None,
            &sender,
            &mut state,
        );
    }
    assert_eq!(state.immediate_tool_responses.len(), 2);
    handle_event(
        ServerEvent::ToolCancellation(vec!["cancelled".into()]),
        None,
        &sender,
        &mut state,
    );
    assert_eq!(state.immediate_tool_responses.len(), 1);
    assert_eq!(state.immediate_tool_responses[0].0, "retained");
    assert_eq!(
        state.immediate_tool_responses[0].2["error"]["code"],
        "turn_already_completed"
    );
    assert!(state.terminal_drain);
    assert!(!state.active);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn transport_replacement_clears_responses_even_without_an_active_job() {
    for terminal in [false, true] {
        let mut state = Reader {
            terminal_drain: terminal,
            ..Reader::default()
        };
        queue(&mut state, "old-transport");
        assert!(!cancel_pending_for_transport_replacement(&mut state));
        assert!(state.immediate_tool_responses.is_empty());
        assert_eq!(state.terminal_drain, terminal);
    }
}
