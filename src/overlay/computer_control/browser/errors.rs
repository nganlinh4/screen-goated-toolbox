//! Stable tool-result mapping for typed browser transport and capability errors.

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
pub(super) enum DebuggerAccess {
    Revoked,
    StorageUnavailable,
}
impl DebuggerAccess {
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::Revoked => "ERR_BROWSER_DEBUGGER_REVOKED",
            Self::StorageUnavailable => "ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE",
        }
    }
}
impl std::fmt::Display for DebuggerAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for DebuggerAccess {}

/// A request that reached a mutation dispatch may have taken effect even when
/// its response is unavailable or cannot be interpreted.
pub(super) fn uncertain_dispatch(error: anyhow::Error) -> Value {
    let mut result = response(error);
    if let Some(object) = result.as_object_mut() {
        object.insert("effect_may_have_occurred".to_string(), json!(true));
    }
    result
}

pub(super) fn response(error: anyhow::Error) -> Value {
    if let Some(access) = error.downcast_ref::<DebuggerAccess>() {
        return json!({"ok":false, "code":access.code(),
        "instruction": match access {
            DebuggerAccess::Revoked => "The user revoked control for this tab. Stop using it. Only the user may resume it from the SGT extension popup.",
            DebuggerAccess::StorageUnavailable => "Debugger revocation storage is unavailable. Browser control remains paused until the extension's storage is repaired and the extension is restarted.",
        }});
    }
    if let Some(key_error) = error.downcast_ref::<super::bridge::PairingKeyError>() {
        return json!({
            "ok": false, "code": key_error.code(), "executed": false,
            "effect_may_have_occurred": false,
            "instruction": "Pairing material is unavailable. Preserve the existing key file, resolve its storage or entropy error, and restart the application before trying browser_setup again.",
        });
    }
    if error
        .downcast_ref::<super::effect_barrier::EffectSettling>()
        .is_some()
    {
        return json!({
            "ok": false, "code": "ERR_BROWSER_EFFECT_SETTLING", "executed": false,
            "effect_may_have_occurred": false, "error": error.to_string(),
            "instruction": "Wait for the outstanding browser operation to finish or reconnect its extension. Do not treat an observation as reconciliation while the operation is unsettled. If terminal evidence is unavailable after the extension restarts, close the affected browser and restart Computer Control.",
        });
    }
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
        let self_reload_available = super::extension_update::available();
        return json!({
            "ok": false,
            "code": "ERR_BROWSER_CAPABILITY_UNSUPPORTED",
            "error": error.to_string(),
            "capability": capability,
            "protocol_version": super::capabilities::protocol_version(),
            "capabilities": super::capabilities::list(),
            "update_staged": update_staged,
            "reload_required": update_staged && !self_reload_available,
            "self_reload_available": self_reload_available,
            "recovery_tool": if self_reload_available { Value::String("browser_setup".to_string()) } else { Value::Null },
            "instruction": if self_reload_available {
                "This connected extension remains usable for its advertised capabilities. Do not retry this unsupported command. Call browser_setup once to activate the staged update, then follow its bounded browser_status retry contract."
            } else if update_staged {
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

#[cfg(test)]
mod tests {
    #[test]
    fn debugger_revocation_codes_survive_extension_error_decoding() {
        for code in [
            "ERR_BROWSER_DEBUGGER_REVOKED",
            "ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE",
        ] {
            let error = super::super::bridge::response_error(
                &serde_json::json!({"ok":false,"code":code}),
                "fallback",
            );
            let result = super::response(error);
            assert_eq!(result["code"], code);
            assert_eq!(result["ok"], false);
        }
    }
    #[test]
    fn pairing_failure_has_a_typed_code_without_secret_contents() {
        for error in [
            super::super::bridge::PairingKeyError::Unavailable,
            super::super::bridge::PairingKeyError::Invalid,
            super::super::bridge::PairingKeyError::Entropy,
        ] {
            let result = super::response(anyhow::Error::new(error));
            assert_eq!(result["code"], error.code());
            assert_eq!(result["executed"], false);
            assert_eq!(result["effect_may_have_occurred"], false);
        }
    }
    #[test]
    fn lost_mutation_response_is_effect_uncertain() {
        let result = super::uncertain_dispatch(anyhow::anyhow!("response lost"));
        assert_eq!(result["ok"], false);
        assert_eq!(result["effect_may_have_occurred"], true);
    }
}
