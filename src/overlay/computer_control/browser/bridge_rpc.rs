//! Capability-gated non-CDP extension RPC envelopes.

use serde_json::{Value, json};

use super::{bridge, capabilities};

/// Send a non-CDP RPC envelope and return its `result`.
pub(super) fn rpc(type_: &str, mut extra: Value) -> anyhow::Result<Value> {
    capabilities::require(rpc_capability(type_, &extra))?;
    extra["id"] = json!(bridge::next_request_id());
    extra["type"] = json!(type_);
    let response = bridge::request(extra)?;
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(response.get("result").cloned().unwrap_or_else(|| json!({})))
    } else {
        Err(bridge::response_error(&response, "rpc error"))
    }
}

fn rpc_capability(type_: &str, extra: &Value) -> String {
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
}
