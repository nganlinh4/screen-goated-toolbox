//! Capability-checked CDP envelopes over the authenticated browser bridge.

use serde_json::{Value, json};

pub(super) fn cdp(method: &str, params: Value) -> anyhow::Result<Value> {
    cdp_in(method, params, None)
}

pub(super) fn cdp_on_tab(method: &str, params: Value, tab_id: i64) -> anyhow::Result<Value> {
    cdp_in_tab_with_policy(method, params, None, tab_id, false)
}

pub(super) fn cdp_in_tab(
    method: &str,
    params: Value,
    session_id: Option<&str>,
    tab_id: i64,
) -> anyhow::Result<Value> {
    cdp_in_tab_with_policy(method, params, session_id, tab_id, false)
}

pub(super) fn cdp_in_active_tab(
    method: &str,
    params: Value,
    session_id: Option<&str>,
    tab_id: i64,
) -> anyhow::Result<Value> {
    cdp_in_tab_with_policy(method, params, session_id, tab_id, true)
}

fn cdp_in_tab_with_policy(
    method: &str,
    params: Value,
    session_id: Option<&str>,
    tab_id: i64,
    require_active: bool,
) -> anyhow::Result<Value> {
    super::capabilities::require(super::capabilities::CDP)?;
    super::capabilities::require(super::capabilities::CDP_EXPLICIT_TAB)?;
    if require_active {
        super::capabilities::require(super::capabilities::CDP_REQUIRE_ACTIVE)?;
    }
    if session_id.is_some() {
        super::capabilities::require(super::capabilities::CDP_SESSION)?;
    }
    let mut env = json!({
        "id": super::bridge::next_request_id(), "type": "cdp", "method": method,
        "params": params, "tabId": tab_id, "requireActive": require_active,
    });
    if let Some(session_id) = session_id {
        env["sessionId"] = json!(session_id);
    }
    decode(super::bridge::request(env)?)
}

pub(super) fn cdp_in(
    method: &str,
    params: Value,
    session_id: Option<&str>,
) -> anyhow::Result<Value> {
    super::capabilities::require(super::capabilities::CDP)?;
    if session_id.is_some() {
        super::capabilities::require(super::capabilities::CDP_SESSION)?;
    }
    let mut env = json!({
        "id": super::bridge::next_request_id(), "type": "cdp",
        "method": method, "params": params
    });
    if let Some(session_id) = session_id {
        env["sessionId"] = json!(session_id);
    }
    decode(super::bridge::request(env)?)
}

fn decode(response: Value) -> anyhow::Result<Value> {
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(response.get("result").cloned().unwrap_or_else(|| json!({})))
    } else {
        Err(super::bridge::response_error(&response, "cdp error"))
    }
}
