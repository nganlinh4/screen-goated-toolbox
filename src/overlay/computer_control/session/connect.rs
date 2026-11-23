//! Cancellable Live connection setup for Computer Control sessions.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Result;

use super::{SessionStopped, Sock};

/// Connect through the shared Gemini Live transport. Debug builds may point
/// the WS base URL elsewhere via `CC_WS_BASE` (for testing v1alpha vs v1beta);
/// release builds always use the official endpoint, so an inherited
/// environment variable can never send the saved API key to another host.
///
/// DNS, TCP, TLS and the WebSocket upgrade all end by `deadline`, and the
/// attempt gives up as soon as the session or this attempt is stopped, so a
/// stalled peer cannot keep a stopped session or the reconnect slot busy.
pub(in crate::overlay::computer_control) fn connect_ws(
    api_key: &str,
    stop: &AtomicBool,
    attempt_stop: Option<&AtomicBool>,
    deadline: Instant,
) -> Result<Sock> {
    let base = live_ws_base(cfg!(debug_assertions), std::env::var("CC_WS_BASE").ok());
    connect_ws_to(&base, api_key, stop, attempt_stop, deadline)
}

fn live_ws_base(allow_override: bool, override_base: Option<String>) -> String {
    override_base
        .filter(|_| allow_override)
        .unwrap_or_else(|| crate::api::gemini_live::transport::GEMINI_LIVE_WS_BASE_URL.to_string())
}

fn connect_ws_to(
    base: &str,
    api_key: &str,
    stop: &AtomicBool,
    attempt_stop: Option<&AtomicBool>,
    deadline: Instant,
) -> Result<Sock> {
    let cancelled = || {
        stop.load(Ordering::SeqCst)
            || attempt_stop.is_some_and(|attempt| attempt.load(Ordering::SeqCst))
    };
    crate::api::gemini_live::transport::connect_websocket_until(base, api_key, deadline, &cancelled)
        .map_err(|error| {
            if cancelled() {
                anyhow::Error::new(SessionStopped)
            } else {
                error
            }
        })
}

#[cfg(test)]
mod tests {
    use super::super::{SessionFailureKind, failure_kind};
    use super::*;
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::time::Duration;

    /// A peer that accepts TCP and then never answers the TLS handshake.
    fn silent_peer() -> (String, std::thread::JoinHandle<()>, Arc<AtomicBool>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!(
            "wss://127.0.0.1:{}/live",
            listener.local_addr().unwrap().port()
        );
        let done = Arc::new(AtomicBool::new(false));
        let peer_done = Arc::clone(&done);
        let peer = std::thread::spawn(move || {
            let (_held, _) = listener.accept().unwrap();
            while !peer_done.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        (base, peer, done)
    }

    #[test]
    fn release_builds_ignore_the_endpoint_override() {
        let official = crate::api::gemini_live::transport::GEMINI_LIVE_WS_BASE_URL;
        let elsewhere = Some("wss://attacker.example/live".to_string());
        assert_eq!(live_ws_base(false, elsewhere.clone()), official);
        assert_eq!(live_ws_base(true, None), official);
        assert_eq!(live_ws_base(true, elsewhere), "wss://attacker.example/live");
    }

    #[test]
    fn session_stop_interrupts_a_stalled_connection_setup() {
        let (base, peer, done) = silent_peer();
        let stop = Arc::new(AtomicBool::new(false));
        let stopper = Arc::clone(&stop);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            stopper.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let error = connect_ws_to(
            &base,
            "key",
            &stop,
            None,
            Instant::now() + Duration::from_secs(20),
        )
        .unwrap_err();
        let elapsed = started.elapsed();
        done.store(true, Ordering::SeqCst);
        peer.join().unwrap();
        assert!(elapsed < Duration::from_secs(3), "stop took {elapsed:?}");
        assert_eq!(failure_kind(&error), SessionFailureKind::Stopped);
    }

    #[test]
    fn attempt_deadline_bounds_a_stalled_connection_setup() {
        let (base, peer, done) = silent_peer();
        let stop = AtomicBool::new(false);
        let attempt_stop = AtomicBool::new(false);
        let started = Instant::now();
        let error = connect_ws_to(
            &base,
            "key",
            &stop,
            Some(&attempt_stop),
            Instant::now() + Duration::from_millis(300),
        )
        .unwrap_err();
        let elapsed = started.elapsed();
        done.store(true, Ordering::SeqCst);
        peer.join().unwrap();
        assert!(
            elapsed < Duration::from_secs(3),
            "deadline took {elapsed:?}"
        );
        assert_eq!(failure_kind(&error), SessionFailureKind::Other);
    }
}
