//! Bounded, capability-negotiated activation of staged extension code.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;

use super::super::telemetry::{self, Privacy};
use super::{bridge, capabilities};

const RETRY_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReloadRequest {
    Requested,
    AlreadyRequested,
}

#[derive(Clone, Copy, Debug)]
struct Attempt {
    loaded_protocol: u64,
    target_protocol: u64,
    at: Instant,
}

fn last_attempt() -> &'static Mutex<Option<Attempt>> {
    static LAST: OnceLock<Mutex<Option<Attempt>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

pub(super) fn available() -> bool {
    capabilities::update_staged() && capabilities::supports(capabilities::RUNTIME_RELOAD)
}

pub(super) fn request_if_staged() -> anyhow::Result<Option<ReloadRequest>> {
    if !available() {
        return Ok(None);
    }

    let loaded_protocol = capabilities::protocol_version();
    let target_protocol = capabilities::CURRENT_PROTOCOL;
    let now = Instant::now();
    let mut last = last_attempt().lock().unwrap();
    if recently_requested(*last, loaded_protocol, target_protocol, now) {
        return Ok(Some(ReloadRequest::AlreadyRequested));
    }

    *last = Some(Attempt {
        loaded_protocol,
        target_protocol,
        at: now,
    });
    drop(last);

    let result = bridge::rpc("runtime", json!({"action": "reload"}))?;
    anyhow::ensure!(
        result.get("reloading").and_then(serde_json::Value::as_bool) == Some(true),
        "browser extension did not acknowledge its staged reload"
    );
    telemetry::event(
        "browser_extension_self_reload_requested",
        "browser_bridge",
        Privacy::Safe,
        json!({
            "loaded_protocol": loaded_protocol,
            "target_protocol": target_protocol,
        }),
    );
    Ok(Some(ReloadRequest::Requested))
}

fn recently_requested(
    attempt: Option<Attempt>,
    loaded_protocol: u64,
    target_protocol: u64,
    now: Instant,
) -> bool {
    attempt.is_some_and(|attempt| {
        attempt.loaded_protocol == loaded_protocol
            && attempt.target_protocol == target_protocol
            && now.saturating_duration_since(attempt.at) < RETRY_COOLDOWN
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_retry_is_bounded_per_version_transition() {
        let now = Instant::now();
        let attempt = Attempt {
            loaded_protocol: 7,
            target_protocol: 8,
            at: now,
        };
        assert!(recently_requested(Some(attempt), 7, 8, now));
        assert!(!recently_requested(Some(attempt), 6, 8, now));
        assert!(!recently_requested(Some(attempt), 7, 9, now));
        assert!(!recently_requested(
            Some(attempt),
            7,
            8,
            now + RETRY_COOLDOWN
        ));
    }
}
