use super::*;

#[test]
fn cancellation_retains_its_kind_through_startup_and_retry_context() {
    let error = anyhow::Error::new(SessionStopped)
        .context("stopped while activating the installed integration catalog")
        .context("initial setup failed");
    assert_eq!(failure_kind(&error), SessionFailureKind::Stopped);
    assert_eq!(
        failure_kind(&anyhow::Error::new(SessionStopped).context("quota exceeded")),
        SessionFailureKind::Stopped
    );
}

#[test]
fn rendered_words_cannot_classify_cancellation_or_provider_limits() {
    for message in [
        "stopped",
        "stopped while activating the installed integration catalog",
        "tool catalog size exceeded",
        "connection deadline exceeded",
        "RESOURCE_EXHAUSTED",
        "quota exceeded",
    ] {
        assert_eq!(
            failure_kind(&anyhow::anyhow!(message)),
            SessionFailureKind::Other,
            "{message}"
        );
    }
}

#[test]
fn connection_deadline_is_a_failure_and_cancellation_still_wins() {
    let stop = AtomicBool::new(false);
    let attempt_stop = AtomicBool::new(false);
    assert!(check_connection_attempt(&stop, Some(&attempt_stop), None).is_ok());
    let deadline = Instant::now();
    let error = check_connection_attempt(&stop, Some(&attempt_stop), Some(deadline)).unwrap_err();
    assert_eq!(failure_kind(&error), SessionFailureKind::Other);
    attempt_stop.store(true, Ordering::SeqCst);
    let error = check_connection_attempt(&stop, Some(&attempt_stop), Some(deadline)).unwrap_err();
    assert_eq!(failure_kind(&error), SessionFailureKind::Stopped);
    attempt_stop.store(false, Ordering::SeqCst);
    stop.store(true, Ordering::SeqCst);
    let error = check_connection_attempt(&stop, None, None).unwrap_err();
    assert_eq!(failure_kind(&error), SessionFailureKind::Stopped);
}

#[test]
fn structured_provider_limits_survive_context_and_message_truncation() {
    for status in [
        serde_json::json!({"code":429,"message":"try later"}),
        serde_json::json!({"status":"RESOURCE_EXHAUSTED","message":"limit"}),
        serde_json::json!({"status":"RESOURCE_EXHAUSTED","message":"x".repeat(3000)}),
    ] {
        let raw = serde_json::json!({"error":status,"setupComplete":{}}).to_string();
        let error = super::super::setup_frame_ready(&raw)
            .unwrap_err()
            .context("initial setup failed twice");
        assert_eq!(failure_kind(&error), SessionFailureKind::ResourceLimit);
    }
}

#[test]
fn unrelated_provider_status_and_spoofed_detail_remain_generic() {
    for status in [
        serde_json::json!({"code":400,"status":"INVALID_ARGUMENT","message":"limit exceeded"}),
        serde_json::json!({"code":504,"status":"DEADLINE_EXCEEDED","message":"quota"}),
        serde_json::json!({"code":"429","message":"429 RESOURCE_EXHAUSTED"}),
        serde_json::json!({"details":{"code":429,"status":"RESOURCE_EXHAUSTED"}}),
        serde_json::json!("RESOURCE_EXHAUSTED"),
    ] {
        let raw = serde_json::json!({"error":status}).to_string();
        let error = super::super::setup_frame_ready(&raw).unwrap_err();
        assert_eq!(failure_kind(&error), SessionFailureKind::Other);
    }
}

#[test]
fn only_http_429_is_resource_limit_evidence() {
    for (status, expected) in [
        (429, SessionFailureKind::ResourceLimit),
        (400, SessionFailureKind::Other),
        (403, SessionFailureKind::Other),
        (503, SessionFailureKind::Other),
    ] {
        let response = tungstenite::http::Response::builder()
            .status(status)
            .body(Some(b"quota exceeded".to_vec()))
            .unwrap();
        let error =
            anyhow::Error::new(tungstenite::Error::Http(Box::new(response))).context("connect");
        assert_eq!(failure_kind(&error), expected);
    }
}
