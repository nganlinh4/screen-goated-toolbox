use std::time::Duration;

use serde_json::{Value, json};

pub(super) fn grant_if_owned(
    request: &mut super::super::bridge_wait::PendingReply,
    register: impl FnOnce(&str) -> bool,
    send: impl FnOnce(u64) -> anyhow::Result<()>,
) -> anyhow::Result<bool> {
    let cancel = request.cancel_token();
    super::super::super::executor::with_job_admission(cancel.as_deref(), || {
        let Some(remaining) = request.admission_remaining(std::time::Instant::now()) else {
            return Ok(false);
        };
        let Some(token) = request.token() else {
            return Ok(false);
        };
        if !register(token) {
            return Ok(false);
        }
        request.mark_admitted();
        send(remaining)?;
        Ok(true)
    })
    .transpose()
    .map(|result| result.unwrap_or(false))
}

pub(super) fn valid_effect_token(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|token| {
        !token.is_empty()
            && token.len() <= 128
            && token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    })
}

pub(super) fn send_grant(
    ws: &mut super::BridgeSocket,
    session: &mut super::super::bridge_auth::Session,
    id: u64,
    remaining_ms: u64,
) -> anyhow::Result<()> {
    // The cancellation handoff lock covers this small write. Bound it so a
    // broken loopback peer cannot indefinitely block a cancellation request.
    ws.get_mut()
        .set_write_timeout(Some(Duration::from_millis(250)))?;
    let result = session
        .seal(&json!({"type":"cdp_dispatch_grant", "requestId":id, "remainingMs":remaining_ms}))
        .and_then(|frame| super::send(ws, frame));
    let restored = ws.get_mut().set_write_timeout(None);
    result?;
    restored?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    #[test]
    fn tokens_are_bounded_and_safe() {
        assert_eq!(
            valid_effect_token(Some(&json!("abc-12_3"))),
            Some("abc-12_3")
        );
        assert!(valid_effect_token(Some(&json!("bad/token"))).is_none());
        assert!(valid_effect_token(Some(&json!("x".repeat(129)))).is_none());
    }

    #[test]
    fn cancellation_cannot_complete_between_admission_check_and_grant_handoff() {
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, _rx) = mpsc::channel();
        let mut pending = super::super::super::bridge_wait::PendingReply::new(
            tx,
            Some(Arc::clone(&cancel)),
            std::time::Instant::now() + Duration::from_secs(2),
            Some("token".to_string()),
        );
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (cancelled_tx, cancelled_rx) = mpsc::channel();
        std::thread::scope(|scope| {
            let request = &mut pending;
            let grant = scope.spawn(move || {
                grant_if_owned(
                    request,
                    |_| true,
                    |_| {
                        entered_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        Ok(())
                    },
                )
                .unwrap()
            });
            entered_rx.recv().unwrap();
            let cancel_worker = scope.spawn(|| {
                super::super::super::super::executor::request_job_cancel(&cancel);
                cancelled_tx.send(()).unwrap();
            });
            assert!(
                cancelled_rx
                    .recv_timeout(Duration::from_millis(50))
                    .is_err()
            );
            assert!(!cancel.load(Ordering::SeqCst));
            release_tx.send(()).unwrap();
            assert!(grant.join().unwrap());
            cancel_worker.join().unwrap();
            assert!(cancel.load(Ordering::SeqCst));
        });
        assert!(pending.was_admitted());
    }
}
