use super::*;

#[test]
fn artifact_record_keeps_originating_action_context() {
    let trace = ActionTrace {
        action_id: 17,
        turn_id: 4,
    };
    let record = artifact_record(
        "click",
        9,
        Some(trace),
        json!({"kind": "target", "turn_id": 999, "action_id": 999}),
    );

    assert_eq!(record["session_id"], session_id());
    assert_eq!(record["turn_id"], 4);
    assert_eq!(record["action_id"], 17);
    assert_eq!(record["record_id"], 9);
    assert_eq!(record["kind"], "target");
}

#[test]
fn only_content_free_events_may_reach_the_global_trace() {
    assert!(Privacy::Safe.may_write_global());
    assert!(!Privacy::Sensitive.may_write_global());
    assert!(!Privacy::UserText.may_write_global());
}

#[test]
fn serialized_write_stamp_replaces_a_producer_sample() {
    let mut record = json!({"event": "test", "ts_ms": 900, "mono_ms": 800});

    stamp_for_serialized_write(&mut record, 101, 42);

    assert_eq!(record["ts_ms"], 101);
    assert_eq!(record["mono_ms"], 42);
}

#[test]
fn unit_test_events_do_not_initialize_the_customer_trace_writer() {
    write_jsonl(json!({"event": "unit_test"}), Privacy::Safe);
    assert!(WRITE_LOCK.get().is_none());
}
