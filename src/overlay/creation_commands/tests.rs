use super::*;
use serde_json::json;
use std::sync::mpsc::channel;
use std::time::Duration;

fn request(target: &Arc<Target>, id: &str) -> Request {
    Request {
        hwnd: crate::win_types::SendHwnd::default(),
        target: target.clone(),
        permit: target.reserve(32).unwrap(),
        envelope: Envelope {
            id: id.to_string(),
            cmd: "model_asset_url".into(),
            args: json!({"path":"result.glb"}),
        },
    }
}

#[test]
fn pending_replies_keep_capacity_reserved_until_the_window_drains_them() {
    let target = Arc::new(Target::new(|_, _, _| Ok(Value::Null), 0, MAX_REQUEST_BYTES));
    for index in 0..MAX_PENDING {
        process_with(
            request(&target, &index.to_string()),
            |_, _, _| Ok(json!(index)),
            |_| {},
        );
    }
    assert!(target.reserve(32).is_err());
    assert_eq!(target.take_replies().len(), MAX_PENDING);
    assert_eq!(target.pending.load(Ordering::Acquire), 0);
    assert!(target.reserve(32).is_ok());
}

#[test]
fn closed_target_discards_queued_commands_before_they_can_have_effects() {
    let target = Arc::new(Target::new(|_, _, _| Ok(Value::Null), 0, MAX_REQUEST_BYTES));
    let pending = request(&target, "queued");
    target.close();
    process_with(
        pending,
        |_, _, _| panic!("stale command executed"),
        |_| panic!("stale wake"),
    );
    assert_eq!(target.pending.load(Ordering::Acquire), 0);
    assert!(target.take_replies().is_empty());
}

#[test]
fn closing_during_work_discards_reply_and_does_not_notify_a_reused_window() {
    let target = Arc::new(Target::new(|_, _, _| Ok(Value::Null), 0, MAX_REQUEST_BYTES));
    let pending = request(&target, "in-flight");
    process_with(
        pending,
        |_, _, active| {
            target.close();
            assert!(!active.load(Ordering::Acquire));
            Ok(json!("late"))
        },
        |_| panic!("closed window notified"),
    );
    let replacement = Target::new(|_, _, _| Ok(Value::Null), 0, MAX_REQUEST_BYTES);
    assert!(replacement.take_replies().is_empty());
    assert_eq!(target.pending.load(Ordering::Acquire), 0);
}

#[test]
fn closing_releases_undelivered_reply_capacity() {
    let target = Arc::new(Target::new(|_, _, _| Ok(Value::Null), 0, MAX_REQUEST_BYTES));
    process_with(
        request(&target, "waiting"),
        |_, _, _| Ok(Value::Null),
        |_| {},
    );
    assert_eq!(target.pending.load(Ordering::Acquire), 1);
    target.close();
    assert_eq!(target.pending.load(Ordering::Acquire), 0);
    assert!(target.reserve(32).is_err());
}

#[test]
fn reply_budget_preserves_request_ids_with_small_errors() {
    let target = Arc::new(Target::new(|_, _, _| Ok(Value::Null), 0, MAX_REQUEST_BYTES));
    let large = "x".repeat(MAX_REPLY_BYTES / 2);
    for id in ["one", "two", "three"] {
        process_with(request(&target, id), |_, _, _| Ok(json!(large)), |_| {});
    }
    let replies = target.take_replies();
    assert!(replies.iter().map(String::len).sum::<usize>() <= MAX_REPLY_BYTES);
    assert!(replies[0].contains("\"id\":\"one\""));
    for (index, id) in [(1, "two"), (2, "three")] {
        assert!(replies[index].contains(&format!("\"id\":\"{id}\"")));
        assert!(replies[index].contains("operation_failed"));
    }
    assert_eq!(target.pending.load(Ordering::Acquire), 0);
}

#[test]
fn outstanding_requests_keep_space_for_every_escaped_identifier() {
    let target = Arc::new(Target::new(|_, _, _| Ok(Value::Null), 0, MAX_REQUEST_BYTES));
    let requests = (0..MAX_PENDING)
        .map(|index| request(&target, &format!("{index:02}{}", "\0".repeat(254))))
        .collect::<Vec<_>>();
    let large = "x".repeat(MAX_REPLY_BYTES - MAX_PENDING * ERROR_RESERVE);
    for pending in requests {
        let fallback = reply_script(&pending.envelope.id, Err("full".into()));
        assert!(fallback.len() <= ERROR_RESERVE);
        process_with(pending, |_, _, _| Ok(json!(large)), |_| {});
    }
    let replies = target.take_replies();
    assert_eq!(replies.len(), MAX_PENDING);
    assert!(replies.iter().map(String::len).sum::<usize>() <= MAX_REPLY_BYTES);
    for (index, script) in replies.iter().enumerate() {
        let id = serde_json::to_string(&format!("{index:02}{}", "\0".repeat(254))).unwrap();
        assert!(script.contains(&format!("\"id\":{id}")));
    }
    assert_eq!(target.pending.load(Ordering::Acquire), 0);
}

#[test]
fn submission_limits_preserve_capacity_for_valid_requests() {
    let entry = Entry::new(Target::new(|_, _, _| Ok(Value::Null), 0, 64 * 1024), |_| {}).unwrap();
    assert!(entry.target.reserve(64 * 1024 + 1).is_err());
    assert!(
        entry
            .submit(
                HWND::default(),
                Envelope {
                    id: "x".repeat(257),
                    cmd: "start_job".into(),
                    args: Value::Null,
                },
                32
            )
            .is_err()
    );
    assert_eq!(entry.target.pending.load(Ordering::Acquire), 0);
    assert!(entry.target.reserve(64 * 1024).is_ok());
}

#[test]
fn blocked_window_submission_is_independent_and_close_invalidates_its_queue() {
    let (started, wait_started) = channel();
    let (release, wait_release) = channel();
    let (finished, wait_finished) = channel();
    let caller = std::thread::current().id();
    let entry = Entry::new(
        Target::new(|_, _, _| Ok(Value::Null), 0, MAX_REQUEST_BYTES),
        move |pending| {
            let id = pending.envelope.id.clone();
            process_with(
                pending,
                |_, _, _| {
                    assert_ne!(std::thread::current().id(), caller);
                    assert_eq!(id, "first", "closed queued submission executed");
                    started.send(()).unwrap();
                    wait_release.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok(Value::Null)
                },
                |_| panic!("closed window notified"),
            );
            finished.send(id).unwrap();
        },
    )
    .unwrap();
    let envelope = |id: &str| Envelope {
        id: id.into(),
        cmd: "start_job".into(),
        args: Value::Null,
    };
    entry
        .submit(HWND::default(), envelope("first"), 32)
        .unwrap();
    wait_started.recv_timeout(Duration::from_secs(5)).unwrap();
    entry
        .submit(HWND::default(), envelope("queued"), 32)
        .unwrap();

    let (other_done, wait_other_done) = channel();
    let other = Entry::new(
        Target::new(|_, _, _| Ok(Value::Null), 0, MAX_REQUEST_BYTES),
        move |pending| {
            process_with(
                pending,
                |_, _, _| Ok(Value::Null),
                |_| {
                    other_done.send(()).unwrap();
                },
            );
        },
    )
    .unwrap();
    other
        .submit(HWND::default(), envelope("independent"), 32)
        .unwrap();
    wait_other_done
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(other.target.take_replies().len(), 1);

    entry.target.close();
    release.send(()).unwrap();
    for expected in ["first", "queued"] {
        assert_eq!(
            wait_finished.recv_timeout(Duration::from_secs(5)).unwrap(),
            expected
        );
    }
    assert!(entry.target.take_replies().is_empty());
    assert_eq!(entry.target.pending.load(Ordering::Acquire), 0);
}

#[test]
fn replies_carry_only_the_stable_code_and_a_full_queue_is_coded() {
    let target = Arc::new(Target::new(|_, _, _| Ok(Value::Null), 0, MAX_REQUEST_BYTES));
    process_with(
        request(&target, "coded"),
        |_, _, _| {
            Err(CreationError::new(
                ErrorCode::NameTaken,
                "A file named private.glb already exists.",
            ))
        },
        |_| {},
    );
    process_with(
        request(&target, "uncoded"),
        |_, _, _| Err("C:/private/detail".into()),
        |_| {},
    );
    let replies = target.take_replies();
    assert!(replies[0].contains("\"error\":\"name_taken\""));
    assert!(replies[1].contains("\"error\":\"operation_failed\""));
    assert!(replies.iter().all(|reply| !reply.contains("private")));

    let held = (0..MAX_PENDING)
        .map(|_| target.reserve(32).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        target.reserve(32).err().unwrap().code(),
        ErrorCode::QueueFull
    );
    drop(held);
    target.close();
    assert_eq!(
        target.reserve(32).err().unwrap().code(),
        ErrorCode::OperationFailed
    );
}
