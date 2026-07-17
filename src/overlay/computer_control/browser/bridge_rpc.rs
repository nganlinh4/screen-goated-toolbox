//! Capability-gated non-CDP extension RPC envelopes.

use serde_json::{Value, json};
use std::time::Instant;

use super::{bridge, capabilities};

/// Send a non-CDP RPC envelope and return its `result`.
pub(super) fn rpc(type_: &str, mut extra: Value) -> anyhow::Result<Value> {
    rpc_with(type_, &mut extra)
}

pub(super) fn rpc_on_epoch(type_: &str, mut extra: Value, epoch: u64) -> anyhow::Result<Value> {
    rpc_with_epoch(type_, &mut extra, epoch, None)
}

pub(super) fn rpc_cleanup_until(
    type_: &str,
    mut extra: Value,
    epoch: u64,
    deadline: Instant,
) -> anyhow::Result<Value> {
    rpc_with_epoch(type_, &mut extra, epoch, Some(deadline))
}

fn rpc_with(type_: &str, extra: &mut Value) -> anyhow::Result<Value> {
    capabilities::require(rpc_capability(type_, extra))?;
    extra["id"] = json!(bridge::next_request_id());
    extra["type"] = json!(type_);
    let response = bridge::request(extra.take())?;
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(response.get("result").cloned().unwrap_or_else(|| json!({})))
    } else {
        Err(bridge::response_error(&response, "rpc error"))
    }
}

fn rpc_with_epoch(
    type_: &str,
    extra: &mut Value,
    epoch: u64,
    cleanup_deadline: Option<Instant>,
) -> anyhow::Result<Value> {
    capabilities::require(rpc_capability(type_, extra))?;
    extra["id"] = json!(bridge::next_request_id());
    extra["type"] = json!(type_);
    let response = match cleanup_deadline {
        Some(deadline) => bridge::request_cleanup_until(extra.take(), epoch, deadline)?,
        None => bridge::request_on_epoch(extra.take(), epoch)?,
    };
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(response.get("result").cloned().unwrap_or_else(|| json!({})))
    } else {
        Err(bridge::response_error(&response, "rpc error"))
    }
}

fn rpc_capability(type_: &str, extra: &Value) -> String {
    if type_ == "runtime" {
        return match extra.get("action").and_then(Value::as_str) {
            Some("reload") => capabilities::RUNTIME_RELOAD.to_string(),
            Some(action) => format!("runtime.{action}"),
            None => "runtime".to_string(),
        };
    }
    if type_ != "tabs" {
        return format!("rpc.{type_}");
    }
    match extra.get("action").and_then(Value::as_str) {
        Some("list") => capabilities::TABS_LIST.to_string(),
        Some("active") => capabilities::TABS_ACTIVE.to_string(),
        Some("activate") => capabilities::TABS_ACTIVATE.to_string(),
        Some("navigate") => capabilities::TABS_NAVIGATE.to_string(),
        Some("create") if extra.get("active").and_then(Value::as_bool) == Some(false) => {
            capabilities::TABS_CREATE_BACKGROUND.to_string()
        }
        Some("create") => capabilities::TABS_CREATE_FOREGROUND.to_string(),
        Some("remove") => capabilities::TABS_REMOVE.to_string(),
        Some(action) => format!("tabs.{action}"),
        None => "tabs".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_rpc_capabilities_follow_requested_effect() {
        assert_eq!(
            rpc_capability("tabs", &json!({"action": "create", "active": false})),
            capabilities::TABS_CREATE_BACKGROUND
        );
        assert_eq!(
            rpc_capability("tabs", &json!({"action": "create", "active": true})),
            capabilities::TABS_CREATE_FOREGROUND
        );
        assert_eq!(
            rpc_capability("tabs", &json!({"action": "remove"})),
            capabilities::TABS_REMOVE
        );
        assert_eq!(
            rpc_capability("tabs", &json!({"action": "navigate"})),
            capabilities::TABS_NAVIGATE
        );
    }

    #[test]
    fn runtime_rpc_capability_follows_requested_effect() {
        assert_eq!(
            rpc_capability("runtime", &json!({"action": "reload"})),
            capabilities::RUNTIME_RELOAD
        );
        assert_eq!(
            rpc_capability("runtime", &json!({"action": "future"})),
            "runtime.future"
        );
    }
}
