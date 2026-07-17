use super::*;

#[test]
fn idle_receiver_stops_even_when_a_sender_is_still_alive() {
    let (tx, rx) = mpsc::channel::<()>();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let worker = std::thread::spawn(move || receive_until_stopped(&rx, &worker_stop));

    stop.store(true, Ordering::SeqCst);
    assert_eq!(worker.join().unwrap(), None);
    drop(tx);
}

#[test]
fn disconnected_receiver_stops_without_a_session_signal() {
    let (tx, rx) = mpsc::channel::<()>();
    let stop = AtomicBool::new(false);
    drop(tx);

    assert_eq!(receive_until_stopped(&rx, &stop), None);
}

#[test]
fn final_failure_telemetry_keeps_the_nested_dispatch_code() {
    let wrapped = json!({
        "ok": false,
        "action_result": {"ok": false, "code": "ERR_STALE_SURFACE"},
    });
    assert_eq!(action_failure_code(&wrapped), "ERR_STALE_SURFACE");
    assert_eq!(
        action_failure_code(&json!({"ok": false})),
        "ERR_ACTION_EXECUTION_FAILED"
    );
}

#[test]
fn done_is_a_bounded_terminal_signal_without_a_second_semantic_judge() {
    let response = accepted_done_response(&"finished ".repeat(80));
    assert_eq!(response["ok"], true);
    assert_eq!(response["completion_status"], "model_declared");
    assert!(response.get("verdict").is_none());
    assert_eq!(response["summary"].as_str().unwrap().chars().count(), 320);
}

#[test]
fn turn_retirement_is_one_way_and_never_returns_a_model_result() {
    let (job_tx, job_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let (cleanup_tx, cleanup_rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let worker =
        std::thread::spawn(move || executor_loop(None, job_rx, done_tx, cleanup_tx, worker_stop));
    job_tx
        .send(Job {
            id: super::super::RETIRE_TURN.to_string(),
            name: super::super::RETIRE_TURN.to_string(),
            args: json!({}),
            task: String::new(),
            user_text: String::new(),
            inherit_evidence: false,
            action: telemetry::ActionTrace {
                action_id: 0,
                turn_id: 7,
            },
            source_frame: None,
            queued_at: std::time::Instant::now(),
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .unwrap();

    assert_eq!(
        cleanup_rx.recv_timeout(std::time::Duration::from_secs(5)),
        Ok(7)
    );
    assert!(done_rx.try_recv().is_err());
    stop.store(true, Ordering::SeqCst);
    drop(job_tx);
    worker.join().unwrap();
}
