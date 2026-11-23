use super::*;

#[test]
fn reconnect_setup_keeps_recent_conversation_without_creating_a_turn() {
    let state = Reader {
        history: vec![
            "User: earlier subject".to_string(),
            "Assistant: earlier answer".to_string(),
            "User: latest follow-up".to_string(),
        ],
        ..Reader::default()
    };
    let context = reconnect_setup_context(&state);
    assert!(context.contains("earlier subject"));
    assert!(context.ends_with("User: latest follow-up"));
    assert!(context.chars().count() <= 6000);
}

#[test]
fn one_large_entry_cannot_evict_the_prior_turn_context() {
    let state = Reader {
        history: vec![
            "User: current subject and constraints".to_string(),
            format!("Assistant: {}", "x".repeat(10_000)),
        ],
        ..Reader::default()
    };
    let context = reconnect_setup_context(&state);
    assert!(context.contains("current subject and constraints"));
    assert!(context.chars().count() <= 6000);
}

#[test]
fn reconnect_context_keeps_only_goal_and_structural_outcomes() {
    let mut state = Reader {
        last_cmd: "current committed task".into(),
        history: vec![
            "User: old task".into(),
            "Assistant: misleading old narration".into(),
            "User: current committed task".into(),
            "Observed (future_reader): current evidence".into(),
            "Assistant: untrusted draft".into(),
        ],
        turn_tools: vec!["received_but_not_delivered".into()],
        ..Reader::default()
    };
    state.turn_outcomes.record_delivered(
        "delivered_reader",
        &serde_json::json!({"ok": true, "content": "sensitive-result"}),
    );
    state.turn_outcomes.record_delivered(
        "failed_writer",
        &serde_json::json!({"ok": false, "error": "sensitive-error"}),
    );
    let context = active_reconnect_context(&state);
    assert!(context.contains("current committed task"));
    assert!(context.contains("delivered_reader=delivered_ok"));
    assert!(context.contains("failed_writer=delivered_failed"));
    assert!(!context.contains("received_but_not_delivered"));
    assert!(!context.contains("sensitive-result"));
    assert!(!context.contains("sensitive-error"));
    assert!(!context.contains("current evidence"));
    assert!(!context.contains("old task"));
    assert!(!context.contains("narration"));
    assert!(!context.contains("untrusted draft"));
}

#[test]
fn transport_replacement_records_uncertainty_and_cancels_pending_work() {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut state = Reader {
        pending: Pending {
            id: Some("pending-id".to_string()),
            tool: Some("future_operation".to_string()),
            turn_id: Some(1),
            cancelled: false,
            cancel: Some(cancel.clone()),
        },
        ..Reader::default()
    };

    assert!(cancel_pending_for_transport_replacement(&mut state));
    assert!(cancel.load(Ordering::SeqCst));
    assert!(cancel_pending_for_transport_replacement(&mut state));
    assert_eq!(state.pending.id.as_deref(), Some("pending-id"));
    assert_eq!(state.pending.tool.as_deref(), Some("future_operation"));
    assert!(state.reconciliation.required());
    let context = active_reconnect_context(&state);
    assert!(context.contains("future_operation=transport_interrupted_result_unknown"));
    assert!(context.contains("interrupted action is still settling"));
    assert!(!context.contains("completed"));
    assert_eq!(
        context
            .matches("future_operation=transport_interrupted_result_unknown")
            .count(),
        1
    );

    let mut already_cancelled = Reader {
        pending: Pending {
            id: Some("barge-in-id".to_string()),
            tool: Some("future_operation".to_string()),
            turn_id: Some(1),
            cancelled: true,
            cancel: Some(Arc::new(AtomicBool::new(true))),
        },
        ..Reader::default()
    };
    assert!(cancel_pending_for_transport_replacement(
        &mut already_cancelled
    ));
    assert!(already_cancelled.reconciliation.required());
    assert!(
        active_reconnect_context(&already_cancelled)
            .contains("transport_interrupted_result_unknown")
    );
}

#[test]
fn complete_catalog_retry_is_bounded_to_two_identical_attempts() {
    let mut attempts = 0;
    let mut retry_notices = 0;
    let result = complete_catalog_with_one_retry(
        || {
            attempts += 1;
            Err::<(), _>(format!("failure-{attempts}"))
        },
        |_| retry_notices += 1,
    );

    assert_eq!(attempts, 2);
    assert_eq!(retry_notices, 1);
    assert_eq!(
        result,
        Err(("failure-1".to_string(), "failure-2".to_string()))
    );
}

#[test]
fn complete_catalog_retry_recovers_without_a_third_attempt() {
    let mut attempts = 0;
    let result = complete_catalog_with_one_retry(
        || {
            attempts += 1;
            if attempts == 1 { Err("first") } else { Ok(()) }
        },
        |_| {},
    );

    assert_eq!(result, Ok(()));
    assert_eq!(attempts, 2);
}

#[test]
fn startup_retry_preserves_the_exact_setup_and_recovers_once() {
    let setup = serde_json::json!({"setup": {"tools": [{"googleSearch": {}},
        {"functionDeclarations": [{"name": "future_tool"}]}]}});
    let stop = AtomicBool::new(false);
    let mut attempts = Vec::new();
    initial_setup_with_one_retry(&setup, &stop, |payload| {
        attempts.push(payload.clone());
        if attempts.len() == 1 {
            anyhow::bail!("temporary setup failure");
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(attempts, [setup.clone(), setup]);
}

#[test]
fn startup_retry_is_bounded_and_keeps_search_on_permanent_setup_errors() {
    let setup = serde_json::json!({"setup": {"tools": [{"googleSearch": {}}]}});
    let stop = AtomicBool::new(false);
    let mut attempts = 0;
    let result = initial_setup_with_one_retry(&setup, &stop, |payload| {
        assert_eq!(payload, &setup);
        attempts += 1;
        Err::<(), _>(anyhow::anyhow!("provider rejected setup"))
    });
    assert_eq!(attempts, 2);
    assert!(result.unwrap_err().to_string().contains("complete catalog"));
}

#[test]
fn startup_credentials_or_stop_do_not_admit_another_attempt() {
    let setup = serde_json::json!({"setup": {"tools": [{"googleSearch": {}}]}});
    let stop = AtomicBool::new(false);
    let mut attempts = 0;
    assert!(
        initial_setup_with_one_retry(&setup, &stop, |_| {
            attempts += 1;
            Err::<(), _>(anyhow::anyhow!("INVALID_API_KEY:gemini"))
        })
        .is_err()
    );
    assert_eq!(attempts, 1);
    attempts = 0;
    let result = initial_setup_with_one_retry(&setup, &stop, |_| {
        attempts += 1;
        Err::<(), _>(anyhow::Error::new(session::SessionStopped).context("setup cancelled"))
    });
    assert_eq!(
        session::failure_kind(&result.unwrap_err()),
        session::SessionFailureKind::Stopped
    );
    assert_eq!(attempts, 1);
    attempts = 0;
    assert!(
        initial_setup_with_one_retry(&setup, &stop, |_| {
            attempts += 1;
            stop.store(true, Ordering::SeqCst);
            Err::<(), _>(anyhow::anyhow!("connection closed"))
        })
        .is_err()
    );
    assert_eq!(attempts, 1);
    assert!(
        initial_setup_with_one_retry(&setup, &stop, |_| -> anyhow::Result<()> {
            panic!("stopped setup attempted a connection")
        })
        .is_err()
    );
}

#[test]
fn cancellation_during_successful_setup_drops_the_unpublished_connection() {
    struct Connection(Arc<AtomicBool>);
    impl Drop for Connection {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let stop = AtomicBool::new(false);
    let dropped = Arc::new(AtomicBool::new(false));
    let result = initial_setup_with_one_retry(&serde_json::json!({}), &stop, |_| {
        stop.store(true, Ordering::SeqCst);
        Ok(Connection(Arc::clone(&dropped)))
    });
    assert!(result.is_err());
    assert!(dropped.load(Ordering::SeqCst));
}
