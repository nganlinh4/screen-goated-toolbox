//! Uncancellable browser compensation after an input or tab-lifecycle edge.

use serde_json::{Value, json};
use std::time::Instant;

pub(super) fn cdp_on_tab_cleanup_until(
    method: &str,
    params: Value,
    tab_id: i64,
    epoch: u64,
    deadline: Instant,
) -> anyhow::Result<Value> {
    super::capabilities::require(super::capabilities::CDP)?;
    super::capabilities::require(super::capabilities::CDP_EXPLICIT_TAB)?;
    let response = super::bridge::request_cleanup_until(
        json!({
            "id": super::bridge::next_request_id(),
            "type": "cdp",
            "method": method,
            "params": params,
            "tabId": tab_id,
            "requireActive": false,
        }),
        epoch,
        deadline,
    )?;
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(response.get("result").cloned().unwrap_or_else(|| json!({})))
    } else {
        Err(super::bridge::response_error(
            &response,
            "cdp cleanup error",
        ))
    }
}
