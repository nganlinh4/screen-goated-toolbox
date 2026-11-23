//! Ordering for the disclosure checkpoint and its last no-effect preflight.

use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn before_disclosure<T>(
    proposal: &Value,
    cancel: &AtomicBool,
    authorize: impl FnOnce(&Value) -> Result<Value, Value>,
    revalidate: impl FnOnce() -> Result<T, Value>,
    dispatch: impl FnOnce(T) -> Value,
) -> Value {
    if cancel.load(Ordering::Acquire) {
        return super::preflight_error("ERR_BROWSER_UPLOAD_CANCELLED", "upload cancelled");
    }
    let authorization = match authorize(proposal) {
        Ok(authorization) => authorization,
        Err(blocked) => return blocked,
    };
    if cancel.load(Ordering::Acquire) {
        return super::preflight_error("ERR_BROWSER_UPLOAD_CANCELLED", "upload cancelled");
    }
    let target = match revalidate() {
        Ok(target) => target,
        Err(error) => return error,
    };
    if cancel.load(Ordering::Acquire) {
        return super::preflight_error("ERR_BROWSER_UPLOAD_CANCELLED", "upload cancelled");
    }
    let mut result = dispatch(target);
    if let Some(object) = result.as_object_mut() {
        object.insert("resource_scope_authorization".to_string(), authorization);
    }
    result
}

#[cfg(test)]
mod tests;
