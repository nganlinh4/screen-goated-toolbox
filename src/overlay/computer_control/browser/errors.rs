//! Stable tool-result mapping for typed browser transport and capability errors.

use serde_json::{Value, json};

pub(super) fn response(error: anyhow::Error) -> Value {
    if let Some(effect_may_have_occurred) = super::bridge_wait::cancellation_effect(&error) {
        return json!({
            "ok": false,
            "code": "ERR_BROWSER_OPERATION_CANCELLED",
            "status": "aborted_by_user",
            "cancelled": true,
            "stage": "browser_request_wait",
            "effect_may_have_occurred": effect_may_have_occurred,
        });
    }
    if let Some(capability) = super::capabilities::unsupported_from(&error) {
        let update_staged = super::capabilities::update_staged();
        return json!({
            "ok": false,
            "code": "ERR_BROWSER_CAPABILITY_UNSUPPORTED",
            "error": error.to_string(),
            "capability": capability,
            "protocol_version": super::capabilities::protocol_version(),
            "capabilities": super::capabilities::list(),
            "update_staged": update_staged,
            "reload_required": update_staged,
            "instruction": if update_staged {
                "This connected extension remains usable for its advertised capabilities. Do not retry this unsupported command. The staged update takes effect only after the user manually reloads the extension in the browser extension manager."
            } else {
                "This connected extension does not expose the requested capability. Do not retry the same command."
            }
        });
    }
    json!({
        "ok": false,
        "code": "ERR_BROWSER_TOOL_FAILED",
        "error": error.to_string(),
    })
}
