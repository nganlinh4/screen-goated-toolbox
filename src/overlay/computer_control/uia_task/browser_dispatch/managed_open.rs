use super::*;

pub(super) fn tab_route(controlled_tab_id: Option<i64>) -> TabRoute {
    controlled_tab_id.map_or(TabRoute::Current, TabRoute::Exact)
}

pub(super) fn default_lifetime() -> super::super::tab_ownership::TabLifetime {
    super::super::tab_ownership::TabLifetime::Persistent
}

pub(super) fn http_url(args: &Value) -> Result<&str, Value> {
    let raw = args.get("url").and_then(Value::as_str).unwrap_or("").trim();
    let valid = url::Url::parse(raw)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some());
    if valid {
        Ok(raw)
    } else {
        Err(json!({
            "ok": false,
            "code": "ERR_OPEN_URL_INVALID",
            "error": "open_url needs an absolute http(s) URL; use launch_app to open a local file",
            "retryable": true,
            "effect_verified": false,
            "effect_may_have_occurred": false,
            "executed": false,
        }))
    }
}

/// Keep a generic URL identity-bound when the browser bridge is already
/// usable. Without a bridge, returning `None` preserves the OS-shell fallback.
pub(super) fn dispatch(
    brain: &mut Brain,
    name: &str,
    args: &Value,
    cancel: &Arc<AtomicBool>,
) -> Option<Value> {
    if name != "open_url" || !super::super::super::browser::is_connected() {
        return None;
    }
    let url = match http_url(args) {
        Ok(url) => url,
        Err(error) => return Some(error),
    };
    if let Err(error) = super::super::super::browser::cancellable_connection_preflight(cancel) {
        return Some(error);
    }
    let lifetime = default_lifetime();
    let result = brain.open_browser_tab_lease(url, lifetime);
    Some(annotate_navigation(
        result,
        lifetime,
        "created_persistent_tab",
        false,
    ))
}
