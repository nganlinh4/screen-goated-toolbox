//! Authoritative child-session snapshots from the attached browser extension.

use serde_json::json;

use super::{bridge, capabilities};

const MAX_SESSIONS: usize = 128;
const MAX_SESSION_ID_BYTES: usize = 1024;

#[derive(Debug, PartialEq, Eq)]
pub(in crate::overlay::computer_control) struct FrameInventory {
    pub connection_epoch: u64,
    pub registry_id: u64,
    pub generation: u64,
    pub sessions: Vec<String>,
}

#[derive(Debug)]
pub(super) struct Incomplete(pub String);

impl std::fmt::Display for Incomplete {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "browser frame coverage is incomplete: {}",
            self.0
        )
    }
}

impl std::error::Error for Incomplete {}

/// The page has more frames than one observation covers. Coverage stays
/// all-or-nothing, so the browser controller refuses the page as a whole.
#[derive(Debug)]
pub(super) struct TooManyFrames(pub String);

impl std::fmt::Display for TooManyFrames {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "browser frame coverage is unavailable: {}",
            self.0
        )
    }
}

impl std::error::Error for TooManyFrames {}

pub(super) fn snapshot(tab_id: i64, require_active: bool) -> anyhow::Result<FrameInventory> {
    capabilities::require(capabilities::FRAME_INVENTORY)?;
    let epoch = bridge::connection_epoch();
    let result = super::bridge_rpc::rpc_on_epoch(
        "frames",
        json!({"tabId": tab_id, "requireActive": require_active}),
        epoch,
    )
    .map_err(classify_rpc_error)?;
    parse(&result, epoch).map_err(|error| Incomplete(error.to_string()).into())
}

fn classify_rpc_error(error: anyhow::Error) -> anyhow::Error {
    if error
        .downcast_ref::<super::effect_barrier::EffectSettling>()
        .is_some()
        || error
            .downcast_ref::<super::errors::DebuggerAccess>()
            .is_some()
        || error.downcast_ref::<TooManyFrames>().is_some()
    {
        error
    } else {
        Incomplete(error.to_string()).into()
    }
}

fn parse(value: &serde_json::Value, connection_epoch: u64) -> anyhow::Result<FrameInventory> {
    anyhow::ensure!(connection_epoch > 0, "missing connection identity");
    let registry_id = value
        .get("registryId")
        .and_then(serde_json::Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| anyhow::anyhow!("missing registry identity"))?;
    let generation = value
        .get("generation")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("missing registry generation"))?;
    let values = value
        .get("sessions")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("missing frame sessions"))?;
    anyhow::ensure!(values.len() <= MAX_SESSIONS, "too many frame sessions");
    let mut sessions = Vec::with_capacity(values.len());
    for value in values {
        let session = value
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= MAX_SESSION_ID_BYTES)
            .ok_or_else(|| anyhow::anyhow!("invalid frame session"))?;
        anyhow::ensure!(
            !sessions.iter().any(|s| s == session),
            "duplicate frame session"
        );
        sessions.push(session.to_string());
    }
    Ok(FrameInventory {
        connection_epoch,
        registry_id,
        generation,
        sessions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_inventory_rejects_malformed_or_duplicate_sessions() {
        assert!(
            parse(
                &json!({"registryId":1,"generation":2,"sessions":["a","b"]}),
                1
            )
            .is_ok()
        );
        assert!(parse(&json!({"registryId":0,"generation":2,"sessions":[]}), 1).is_err());
        assert!(
            parse(
                &json!({"registryId":1,"generation":2,"sessions":["a","a"]}),
                1
            )
            .is_err()
        );
        assert!(parse(&json!({"registryId":1,"generation":2,"sessions":[""]}), 1).is_err());
        assert!(parse(&json!({"registryId":1,"generation":2,"sessions":[]}), 0).is_err());
    }

    #[test]
    fn inventory_failures_and_old_extensions_have_typed_errors() {
        let incomplete: anyhow::Error = Incomplete("attachment failed".to_string()).into();
        let result = super::super::frame_coverage_error(&incomplete).unwrap();
        assert_eq!(result["code"], "ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE");
        let old_extension = capabilities::unsupported(capabilities::FRAME_INVENTORY);
        let result = super::super::frame_coverage_error(&old_extension).unwrap();
        assert_eq!(result["code"], "ERR_BROWSER_CAPABILITY_UNSUPPORTED");
        assert_eq!(result["capability"], capabilities::FRAME_INVENTORY);
        let old_extension = capabilities::unsupported(capabilities::CDP_DISPATCH_ADMISSION);
        let result = super::super::frame_coverage_error(&old_extension).unwrap();
        assert_eq!(result["code"], "ERR_BROWSER_CAPABILITY_UNSUPPORTED");
        assert_eq!(result["capability"], capabilities::CDP_DISPATCH_ADMISSION);
    }

    #[test]
    fn a_page_beyond_the_frame_bound_is_refused_without_a_retry_hint() {
        let reply = json!({
            "id": 4, "ok": false, "code": "ERR_BROWSER_TOO_MANY_FRAMES",
            "error": "page has more than 128 frames",
        });
        let error = classify_rpc_error(bridge::response_error(&reply, "rpc error"));
        let result = super::super::frame_coverage_error(&error).unwrap();
        assert_eq!(result["code"], "ERR_BROWSER_TOO_MANY_FRAMES");
        assert_eq!(result["retryable"], false);
        assert_eq!(result["effect_may_have_occurred"], false);
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("more than 128 frames")
        );
        assert!(result["next_step"].as_str().unwrap().contains("screen"));

        let other =
            json!({"ok": false, "code": "ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE", "error": "x"});
        let error = classify_rpc_error(bridge::response_error(&other, "rpc error"));
        assert_eq!(
            super::super::frame_coverage_error(&error).unwrap()["code"],
            "ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE"
        );
    }
}
