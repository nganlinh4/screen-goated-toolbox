//! Retry replacement-transport reseeding only for an actual socket write failure.

use std::sync::{
    Arc, LazyLock,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const CONNECT_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_POLL: Duration = Duration::from_millis(50);
static CONNECTOR_BUSY: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));

struct ConnectorLease(Arc<AtomicBool>);

impl Drop for ConnectorLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(super) struct ReconnectRequest<'a> {
    pub key: &'a str,
    pub target: Option<&'a str>,
    pub stop: &'a Arc<AtomicBool>,
    pub trigger: &'a str,
}

pub(super) fn recover_write(
    error: anyhow::Error,
    socket: &mut super::Sock,
    reconnects: &mut u32,
    state: &mut super::Reader,
    request: ReconnectRequest<'_>,
) -> anyhow::Result<bool> {
    if !super::session::is_socket_write_failure(&error) {
        return Err(error);
    }
    super::overlay::push_log(format!("(transport write failed: {error} - reconnecting)"));
    super::reconnect_session(
        socket,
        request.key,
        request.target,
        reconnects,
        state,
        request.stop,
        request.trigger,
    )
}

pub(super) fn retry_reseed<T>(mut attempt: impl FnMut() -> anyhow::Result<T>) -> anyhow::Result<T> {
    loop {
        match attempt() {
            Err(error) if super::session::is_socket_write_failure(&error) => {
                super::overlay::push_log("(reconnect write failed - retrying)".to_string());
            }
            result => return result,
        }
    }
}

pub(super) fn note_audio_gap<T>(result: anyhow::Result<T>) -> anyhow::Result<T> {
    if result.is_err() {
        super::overlay::push_log(
            "(audio transmission interrupted; some speech may need to be repeated)".to_string(),
        );
    }
    result
}

/// DNS/TLS can block independently of socket timeouts. Only this worker owns
/// its socket, and it must recheck stop before sending setup after connect.
pub(super) fn connect_replacement(
    key: &str,
    context: &str,
    stop: &Arc<AtomicBool>,
) -> anyhow::Result<super::Sock> {
    if stop.load(Ordering::SeqCst) {
        anyhow::bail!(super::session::SessionStopped);
    }
    let key = key.to_owned();
    let context = context.to_owned();
    let worker_stop = Arc::clone(stop);
    run_cancelable_connector(
        stop,
        &CONNECTOR_BUSY,
        CONNECT_ATTEMPT_TIMEOUT,
        move |attempt_stop, deadline| {
            super::uia_task::reconnect(
                &key,
                (!context.is_empty()).then_some(context.as_str()),
                &worker_stop,
                &attempt_stop,
                deadline,
            )
        },
    )
}

fn run_cancelable_connector<T: Send + 'static>(
    stop: &AtomicBool,
    busy: &Arc<AtomicBool>,
    timeout: Duration,
    connect: impl FnOnce(Arc<AtomicBool>, Instant) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if stop.load(Ordering::SeqCst) {
            anyhow::bail!(super::session::SessionStopped);
        }
        if Instant::now() >= deadline {
            anyhow::bail!("reconnect connector deadline elapsed while waiting for previous worker");
        }
        if busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            break;
        }
        std::thread::sleep(CONNECT_POLL.min(deadline.saturating_duration_since(Instant::now())));
    }
    let lease = ConnectorLease(Arc::clone(busy));
    if stop.load(Ordering::SeqCst) {
        anyhow::bail!(super::session::SessionStopped);
    }
    let attempt_stop = Arc::new(AtomicBool::new(false));
    let worker_attempt = Arc::clone(&attempt_stop);
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("cc-reconnect".into())
        .spawn(super::super::session_retention::inherit(move || {
            // Keep the slot owned until a late result and its socket have been dropped.
            let _lease = lease;
            let _ = sender.send(connect(worker_attempt, deadline));
        }))?;
    loop {
        if stop.load(Ordering::SeqCst) {
            super::control::request_stop(&attempt_stop);
            anyhow::bail!(super::session::SessionStopped);
        }
        if Instant::now() >= deadline {
            super::control::request_stop(&attempt_stop);
            anyhow::bail!("reconnect connector deadline elapsed");
        }
        match receiver
            .recv_timeout(CONNECT_POLL.min(deadline.saturating_duration_since(Instant::now())))
        {
            Ok(result) => {
                if stop.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    super::control::request_stop(&attempt_stop);
                    if stop.load(Ordering::SeqCst) {
                        anyhow::bail!(super::session::SessionStopped);
                    }
                    anyhow::bail!("reconnect connector deadline elapsed");
                }
                return result;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!("reconnect worker failed")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::computer_control::session::SocketWriteFailure;

    #[test]
    fn retries_only_reseed_writes_and_keeps_budget_owned_by_attempt() {
        let mut count = 0;
        let value = retry_reseed(|| {
            count += 1;
            if count < 3 {
                Err(anyhow::Error::new(SocketWriteFailure))
            } else {
                Ok(false)
            }
        })
        .unwrap();
        assert!(!value);
        assert_eq!(count, 3);
    }

    #[test]
    fn validation_errors_do_not_trigger_reconnect_loop() {
        let mut count = 0;
        let result = retry_reseed(|| {
            count += 1;
            Err::<(), _>(anyhow::anyhow!("invalid setup"))
        });
        assert!(result.is_err());
        assert_eq!(count, 1);
    }

    #[test]
    fn stopped_during_blocked_connect_cannot_send_late_setup() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (blocked_tx, blocked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (effect_tx, effect_rx) = mpsc::channel();
        let caller_stop = Arc::clone(&stop);
        let busy = Arc::new(AtomicBool::new(false));
        let caller_busy = Arc::clone(&busy);
        let caller = std::thread::spawn(move || {
            run_cancelable_connector(
                &caller_stop,
                &caller_busy,
                Duration::from_secs(2),
                move |attempt_stop, deadline| {
                    blocked_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    super::super::control::reconnect_attempt_effect(
                        &worker_stop,
                        Some(&attempt_stop),
                        Some(deadline),
                        || {
                            effect_tx.send(()).unwrap();
                            Ok(())
                        },
                    )
                },
            )
        });
        blocked_rx.recv().unwrap();
        super::super::control::request_stop(&stop);
        let error = caller.join().unwrap().unwrap_err();
        assert_eq!(
            super::super::session::failure_kind(&error),
            super::super::session::SessionFailureKind::Stopped
        );
        release_tx.send(()).unwrap();
        assert!(
            effect_rx
                .recv_timeout(std::time::Duration::from_millis(200))
                .is_err()
        );
        assert!(stop.load(Ordering::SeqCst));
    }

    #[test]
    fn timed_out_connector_keeps_slot_until_worker_exits_and_cannot_send_late() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let busy = Arc::new(AtomicBool::new(false));
        let caller_stop = Arc::clone(&stop);
        let caller_busy = Arc::clone(&busy);
        let worker_stop = Arc::clone(&stop);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (effect_tx, effect_rx) = mpsc::channel();
        let first = std::thread::spawn(move || {
            run_cancelable_connector(
                &caller_stop,
                &caller_busy,
                Duration::from_millis(100),
                move |attempt_stop, deadline| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    super::super::control::reconnect_attempt_effect(
                        &worker_stop,
                        Some(&attempt_stop),
                        Some(deadline),
                        || {
                            effect_tx.send(()).unwrap();
                            Ok(())
                        },
                    )
                },
            )
        });
        entered_rx.recv().unwrap();
        let error = first.join().unwrap().unwrap_err();
        assert_eq!(
            super::super::session::failure_kind(&error),
            super::super::session::SessionFailureKind::Other
        );
        assert!(busy.load(Ordering::Acquire));
        let started = Arc::new(AtomicBool::new(false));
        let worker_started = Arc::clone(&started);
        assert!(
            run_cancelable_connector(&stop, &busy, Duration::from_millis(75), move |_, _| {
                worker_started.store(true, Ordering::SeqCst);
                Ok(())
            })
            .is_err()
        );
        assert!(!started.load(Ordering::SeqCst));
        release_tx.send(()).unwrap();
        assert!(effect_rx.recv_timeout(Duration::from_secs(1)).is_err());
        let deadline = Instant::now() + Duration::from_secs(1);
        while busy.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(!busy.load(Ordering::Acquire));
        assert_eq!(
            run_cancelable_connector(&stop, &busy, Duration::from_secs(1), |_, _| Ok(7)).unwrap(),
            7
        );
    }
}
