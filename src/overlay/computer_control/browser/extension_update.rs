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
    capabilities::update_staged()
        && capabilities::supports(capabilities::RUNTIME_RELOAD)
        && capabilities::supports(capabilities::EFFECT_ADMISSION)
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

    let epoch = bridge::connection_epoch();
    let token = super::crypto::random_hex(16)?;
    let receipt = super::reload_receipt::ReloadReceipt::register(token.clone(), epoch)?;
    let deadline =
        Instant::now() + super::readiness::bounded_request_timeout(Duration::from_secs(15));
    let result = bridge::rpc("runtime", json!({"action": "reload", "effectToken": token}));
    if let Err(error) = result
        && super::bridge_wait::dispatch_effect(&error) == Some(false)
    {
        return Err(error);
    }
    let new_epoch = receipt.wait(deadline).map_err(|_| anyhow::anyhow!(
        "browser extension reload has not been confirmed; wait for it to reconnect or reload it on the extensions page"
    ))?;
    anyhow::ensure!(
        bridge::connection_epoch() == new_epoch
            && bridge::is_connected()
            && capabilities::protocol_version() >= target_protocol
            && super::effect_barrier::ready(),
        "browser extension reconnected without confirming the staged version; retry setup"
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
