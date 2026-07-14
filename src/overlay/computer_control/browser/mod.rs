//! Deep browser control for the Computer Control agent: a thin Chrome MV3
//! extension (a CDP-over-WebSocket bridge) + this Rust side that hosts the socket
//! and implements high-level tools on raw CDP. Extension contract:
//! `src/overlay/computer_control/browser_ext/README.md`.
//!
//! The extension runs in the user's REAL logged-in session via `chrome.debugger`
//! (the only route left after Chrome 136 blocked `--remote-debugging-port` on the
//! default profile). All logic lives here in Rust; the extension just forwards CDP.

mod bridge;
mod bridge_listener;
mod bridge_rpc;
mod bridge_wait;
mod capabilities;
mod controller_io;
mod crypto;
mod errors;
mod frame_identity;
mod page;
mod pointer;
mod prefs;
mod readiness;
mod setup;
mod surface_binding;
mod tab_tools;
mod upload;

use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};
use std::time::Duration;

pub(super) use bridge::is_connected;
pub(super) use controller_io::{
    DOCUMENT_ID_JS, active_tab_id, click_selector_on_active_tab, click_selector_on_tab,
    eval_value_in_active_tab, fill_in_on_active_tab, fill_in_on_tab,
};
pub(super) use frame_identity::{
    active_document_identity, validate_active_document_identity, validate_document_identity_on_tab,
};
pub(super) use page::{extract_page, extract_page_on_tab, read_page, read_page_on_tab};
pub(super) use pointer::{
    cancelled_before_pointer_effect, click_on_document, drag_on_document, pointer_error_response,
};
pub(super) use prefs::{ever_connected, recently_connected, record_connection};
pub(super) use setup::{reset, setup, status};
pub(super) use tab_tools::{
    eval_js, eval_js_on_document, navigate, navigate_on_tab, read_console, read_console_on_tab,
    read_network, read_network_on_tab, wait_for, wait_for_on_tab,
};
pub(super) use upload::{upload_file, upload_file_on_document};

pub(super) fn ensure_started() {
    readiness::mark_bridge_start();
    bridge::ensure_started();
}

/// Hold the session's public ready boundary briefly for an extension that has
/// already paired here. First-time installs do not wait; a stale remembered peer
/// gets only a short probe. The global session stop interrupts either wait.
pub(super) fn await_startup_readiness(stop: &AtomicBool) -> bool {
    let policy = readiness::startup_wait(is_connected(), ever_connected(), recently_connected());
    let started = std::time::Instant::now();
    let outcome = if stop.load(Ordering::SeqCst) {
        Some(readiness::WaitOutcome::Cancelled)
    } else if policy.duration().is_zero() {
        None
    } else {
        Some(readiness::wait_for_connection(
            policy.duration(),
            Some(stop),
            is_connected,
        ))
    };
    super::telemetry::event(
        "browser_startup_readiness",
        "browser_bridge",
        super::telemetry::Privacy::Safe,
        json!({
            "expected": policy.reason(),
            "outcome": match outcome {
                None => "skipped",
                Some(readiness::WaitOutcome::Ready) => "ready",
                Some(readiness::WaitOutcome::Cancelled) => "cancelled",
                Some(readiness::WaitOutcome::TimedOut) => "timed_out",
            },
            "wait_ms": started.elapsed().as_millis(),
            "wait_budget_ms": policy.duration().as_millis(),
        }),
    );
    outcome != Some(readiness::WaitOutcome::Cancelled)
}

pub(super) fn cancellable_connection_preflight(
    cancel: &Arc<AtomicBool>,
) -> Result<readiness::ConnectionPreflight, Value> {
    match conn_guard_with_cancel(Some(cancel.as_ref())) {
        Some(error) => Err(error),
        None => Ok(readiness::enter_preflight(cancel)),
    }
}

pub(super) fn err(e: anyhow::Error) -> Value {
    errors::response(e)
}

fn not_connected() -> Value {
    json!({
        "ok": false,
        "code": "ERR_BROWSER_NOT_CONNECTED",
        "state": connection_state(),
        "error": "the browser extension isn't connected",
        "hint": "Call browser_setup and follow its do_yourself steps to load the unpacked extension yourself; it auto-pairs (no code to paste). Then poll browser_status until connected."
    })
}

fn connection_state() -> &'static str {
    if is_connected() {
        "connected"
    } else if bridge::pairing_window_open() {
        "pairing_window_open"
    } else if recently_connected() {
        "reconnecting"
    } else if ever_connected() {
        "disconnected_was_connected"
    } else {
        "not_setup"
    }
}

/// Run `Runtime.evaluate` in the TOP frame and return its by-value result.
pub(super) fn eval_value(expr: &str) -> anyhow::Result<Value> {
    eval_value_in(expr, None)
}

pub(super) fn eval_value_on_tab(expr: &str, tab_id: i64) -> anyhow::Result<Value> {
    eval_value_in_exact_tab(expr, tab_id, None)
}

pub(super) fn eval_value_in_exact_tab(
    expr: &str,
    tab_id: i64,
    session_id: Option<&str>,
) -> anyhow::Result<Value> {
    let r = bridge::cdp_in_tab(
        "Runtime.evaluate",
        json!({ "expression": expr, "returnByValue": true, "awaitPromise": true }),
        session_id,
        tab_id,
    )?;
    if let Some(exc) = r.get("exceptionDetails") {
        anyhow::bail!("js exception: {}", js_exception_text(exc));
    }
    runtime_result_value(&r)
}

/// Like `eval_value`, but optionally inside a specific cross-origin FRAME's CDP
/// session (`None` = top frame). Lets the controller read + act inside an
/// out-of-process iframe the top document can't reach.
pub(super) fn eval_value_in(expr: &str, session_id: Option<&str>) -> anyhow::Result<Value> {
    let r = bridge::cdp_in(
        "Runtime.evaluate",
        json!({ "expression": expr, "returnByValue": true, "awaitPromise": true }),
        session_id,
    )?;
    if let Some(exc) = r.get("exceptionDetails") {
        anyhow::bail!("js exception: {}", js_exception_text(exc));
    }
    runtime_result_value(&r)
}

fn runtime_result_value(response: &Value) -> anyhow::Result<Value> {
    let remote = response
        .get("result")
        .ok_or_else(|| anyhow::anyhow!("Runtime.evaluate returned no result object"))?;
    if remote.get("type").and_then(Value::as_str) == Some("undefined") {
        anyhow::bail!(
            "js expression returned undefined; explicitly return a JSON-compatible value"
        );
    }
    remote.get("value").cloned().ok_or_else(|| {
        let kind = remote
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        anyhow::anyhow!("js expression returned a non-JSON-compatible {kind} value")
    })
}

/// A CDP `exceptionDetails` as a DEBUGGABLE message - real error name + message + stack
/// (`exception.description`), not just "Uncaught" - so a failing `browser_eval` can be fixed.
/// A thrown primitive (`throw "x"`) has only `exception.value`, so add where it threw.
fn js_exception_text(exc: &Value) -> String {
    let detail = exc
        .pointer("/exception/description")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            exc.pointer("/exception/value").map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string())
            })
        })
        .or_else(|| exc.get("text").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| "error".to_string());
    let detail: String = detail.chars().take(800).collect();
    let line = exc.get("lineNumber").and_then(Value::as_u64);
    let col = exc.get("columnNumber").and_then(Value::as_u64);
    match (detail.contains("\n    at "), line, col) {
        (false, Some(l), Some(c)) => format!("{detail} (at line {l}, col {c})"),
        _ => detail,
    }
}

/// CDP sessions of the cross-origin (out-of-process) iframes flat-attached to the
/// tab, learned from the auto-attach events. Best-effort: a dead/detached session
/// just errors when used and is skipped. Used to perceive + act inside frames the
/// top document can't reach (login / payment / embed widgets).
pub(super) fn child_frames() -> Vec<String> {
    child_frames_for_tab(None)
}

pub(super) fn child_frames_on_tab(tab_id: i64) -> Vec<String> {
    child_frames_for_tab(Some(tab_id))
}

fn child_frames_for_tab(tab_id: Option<i64>) -> Vec<String> {
    use std::collections::HashSet;
    let detached: HashSet<String> =
        bridge::recent_events_on_tab("Target.detachedFromTarget", 50, tab_id)
            .iter()
            .filter_map(|e| {
                e.get("params")
                    .and_then(|p| p.get("sessionId"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for e in bridge::recent_events_on_tab("Target.attachedToTarget", 50, tab_id) {
        let p = e.get("params");
        let ttype = p
            .and_then(|p| p.get("targetInfo"))
            .and_then(|t| t.get("type"))
            .and_then(Value::as_str);
        let Some(sid) = p.and_then(|p| p.get("sessionId")).and_then(Value::as_str) else {
            continue;
        };
        if ttype == Some("iframe") && !detached.contains(sid) && seen.insert(sid.to_string()) {
            out.push(sid.to_string());
        }
    }
    out
}

// ── Page tools (all guard on connection) ─────────────────────────────────────

macro_rules! require_conn {
    () => {
        if let Some(v) = conn_guard() {
            return v;
        }
    };
}

/// Connection gate for page tools. An existing install gets one full reconnect
/// cycle before recency distinguishes a sleeping worker from a stale install;
/// a never-installed extension returns setup guidance immediately.
/// `Some(error)` = bail, `None` = proceed.
fn conn_guard() -> Option<Value> {
    let cancel = readiness::current_cancel();
    conn_guard_with_cancel(cancel.as_deref())
}

fn conn_guard_with_cancel(cancel: Option<&AtomicBool>) -> Option<Value> {
    if cancel.is_some_and(|token| token.load(Ordering::SeqCst)) {
        return Some(json!({
            "ok": false,
            "status": "aborted_by_user",
            "cancelled": true,
            "stage": "browser_connection_wait",
            "effect_may_have_occurred": false,
        }));
    }
    if is_connected() {
        return None;
    }
    if ever_connected() {
        let wait = if readiness::preflight_active() {
            Duration::ZERO
        } else {
            readiness::existing_install_wait(
                recently_connected(),
                readiness::bridge_startup_plausible(),
            )
        };
        match readiness::wait_for_connection(wait, cancel, is_connected) {
            readiness::WaitOutcome::Ready => return None,
            readiness::WaitOutcome::Cancelled => {
                return Some(json!({
                    "ok": false,
                    "status": "aborted_by_user",
                    "cancelled": true,
                    "stage": "browser_connection_wait",
                    "effect_may_have_occurred": false,
                }));
            }
            readiness::WaitOutcome::TimedOut => {}
        }
        return Some(if recently_connected() {
            json!({
                "ok": false,
                "code": "ERR_BROWSER_RECONNECTING",
                "state": connection_state(),
                "error": "the browser extension is reconnecting",
                "hint": "Browser control IS installed here - its background service worker just went idle and reconnects on its own. Do NOT run browser_setup and do NOT tell the user to set anything up; wait a moment and retry this call."
            })
        } else {
            json!({
                "ok": false,
                "code": "ERR_BROWSER_DISCONNECTED",
                "state": connection_state(),
                "error": "browser control isn't responding",
                "hint": "It was set up here before but hasn't connected for a while - the extension may have been removed or the browser closed. If the user still wants deep browser control, run browser_setup; otherwise just proceed without it (use the on-screen tools)."
            })
        });
    }
    Some(not_connected())
}

// ── Vision-located input over the trusted browser pipeline (CDP) ──────────────
//
// A vision model can find a target the DOM can't address by selector — a card on
// a <canvas>, a control inside a cross-origin iframe. We screenshot the VIEWPORT
// via CDP, let vision locate the target in that crisp page image, then act there
// with `Input.dispatchMouseEvent`. Because the screenshot IS the viewport, a
// 0-1000 normalized point maps linearly to CSS px — the exact space CDP input
// uses — with no window-chrome or DPR math. And CDP events are TRUSTED, so they
// drive canvas/WebGL pointer handlers (and HTML5 drag-and-drop) that ignore the
// synthetic OS mouse events SendInput produces.

/// Whether a vision-located click/drag should go through the browser's trusted
/// input pipeline. Ownership requires the extension's active browser tab/window
/// to bind to the exact foreground HWND/PID generation.
pub(super) fn input_active() -> bool {
    is_connected() && surface_binding::active().is_ok()
}

pub(super) fn owns_foreground_window(hwnd: u64) -> bool {
    surface_binding::owns_foreground_window(hwnd)
}

/// Capture the controlled tab's viewport as a JPEG, with its CSS width/height so a
/// 0-1000 normalized vision hit can be scaled to the CSS px CDP input expects.
pub(super) fn shot_on_tab(tab_id: i64) -> anyhow::Result<(Vec<u8>, f64, f64)> {
    shot_impl(Some(tab_id))
}

fn vision_cdp(tab_id: Option<i64>, method: &str, params: Value) -> anyhow::Result<Value> {
    match tab_id {
        Some(tab_id) => bridge::cdp_on_tab(method, params, tab_id),
        None => bridge::cdp(method, params),
    }
}

fn shot_impl(tab_id: Option<i64>) -> anyhow::Result<(Vec<u8>, f64, f64)> {
    let size = match tab_id {
        Some(tab_id) => {
            eval_value_on_tab("({w: window.innerWidth, h: window.innerHeight})", tab_id)
        }
        None => eval_value("({w: window.innerWidth, h: window.innerHeight})"),
    }?;
    let cw = size.get("w").and_then(Value::as_f64).unwrap_or(0.0);
    let ch = size.get("h").and_then(Value::as_f64).unwrap_or(0.0);
    if cw < 1.0 || ch < 1.0 {
        anyhow::bail!("browser viewport has no size");
    }
    let _ = vision_cdp(tab_id, "Page.enable", json!({})); // idempotent; some builds need it
    let r = vision_cdp(
        tab_id,
        "Page.captureScreenshot",
        json!({"format": "jpeg", "quality": 65}),
    )?;
    let b64 = r
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("no screenshot data"))?;
    use base64::Engine as _;
    let jpeg = base64::engine::general_purpose::STANDARD.decode(b64)?;
    Ok((jpeg, cw, ch))
}

/// Open `url` in a NEW tab of the current window (keeps the existing page).
pub(super) fn open_tab(url: &str) -> Value {
    require_conn!();
    match bridge::rpc("tabs", json!({"action": "create", "url": url})) {
        Ok(v) => json!({"ok": true, "tab": v}),
        Err(e) => err(e),
    }
}

pub(super) struct TemporaryBrowserTab {
    pub id: i64,
    pub foreground: bool,
}

pub(super) fn open_temporary_tab(url: &str) -> anyhow::Result<TemporaryBrowserTab> {
    let can_close = capabilities::supports(capabilities::TABS_REMOVE)
        || capabilities::supports(capabilities::CDP_EXPLICIT_TAB);
    if !can_close {
        return Err(capabilities::unsupported(capabilities::TABS_REMOVE));
    }
    let Some(foreground) = temporary_tab_foreground(
        capabilities::supports(capabilities::TABS_CREATE_BACKGROUND),
        capabilities::supports(capabilities::TABS_CREATE_FOREGROUND),
    ) else {
        return Err(capabilities::unsupported(
            capabilities::TABS_CREATE_BACKGROUND,
        ));
    };
    let tab = bridge::rpc(
        "tabs",
        json!({"action": "create", "url": url, "active": foreground}),
    )?;
    let id = tab
        .get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow::anyhow!("browser did not return a temporary tab id"))?;
    Ok(TemporaryBrowserTab { id, foreground })
}

fn temporary_tab_foreground(can_background: bool, can_foreground: bool) -> Option<bool> {
    if can_background {
        Some(false)
    } else if can_foreground {
        Some(true)
    } else {
        None
    }
}

pub(super) fn close_tab(tab_id: i64) -> anyhow::Result<()> {
    if capabilities::supports(capabilities::TABS_REMOVE) {
        bridge::rpc("tabs", json!({"action": "remove", "tabId": tab_id}))?;
    } else if capabilities::supports(capabilities::CDP_EXPLICIT_TAB) {
        bridge::cdp_on_tab("Page.close", json!({}), tab_id)?;
    } else {
        return Err(capabilities::unsupported(capabilities::TABS_REMOVE));
    }
    Ok(())
}

pub(super) fn get_tabs() -> Value {
    require_conn!();
    match bridge::rpc("tabs", json!({"action": "list"})) {
        Ok(v) => json!({"ok": true, "tabs": v}),
        Err(e) => err(e),
    }
}

pub(super) fn switch_tab(tab_id: i64) -> Value {
    require_conn!();
    if let Err(e) = bridge::rpc("tabs", json!({"action": "activate", "tabId": tab_id})) {
        return err(e);
    }
    // Confirm WHERE we landed (title + url) so the agent doesn't keep reading/acting on the wrong tab.
    let (title, url) = tab_title_url(tab_id);
    json!({
        "ok": true,
        "switched": tab_id,
        "target_tab_id": tab_id,
        "title": title,
        "url": url,
    })
}

/// A tab's title + url from the live tab list (best-effort; nulls on miss).
fn tab_title_url(tab_id: i64) -> (Value, Value) {
    if let Ok(v) = bridge::rpc("tabs", json!({"action": "list"}))
        && let Some(t) = v.as_array().and_then(|tabs| {
            tabs.iter()
                .find(|t| t.get("id").and_then(Value::as_i64) == Some(tab_id))
        })
    {
        return (
            t.get("title").cloned().unwrap_or(Value::Null),
            t.get("url").cloned().unwrap_or(Value::Null),
        );
    }
    (Value::Null, Value::Null)
}

#[cfg(test)]
mod tests {
    use super::{runtime_result_value, temporary_tab_foreground};
    use serde_json::json;

    #[test]
    fn runtime_eval_distinguishes_json_null_from_missing_values() {
        assert_eq!(
            runtime_result_value(&json!({
                "result": {"type": "object", "subtype": "null", "value": null}
            }))
            .unwrap(),
            serde_json::Value::Null
        );

        let undefined = runtime_result_value(&json!({"result": {"type": "undefined"}}))
            .unwrap_err()
            .to_string();
        assert!(undefined.contains("explicitly return"));

        let unserializable = runtime_result_value(&json!({
            "result": {"type": "number", "unserializableValue": "NaN"}
        }))
        .unwrap_err()
        .to_string();
        assert!(unserializable.contains("non-JSON-compatible number"));
    }

    #[test]
    fn temporary_tabs_degrade_only_to_an_available_foreground_create() {
        assert_eq!(temporary_tab_foreground(true, true), Some(false));
        assert_eq!(temporary_tab_foreground(false, true), Some(true));
        assert_eq!(temporary_tab_foreground(false, false), None);
    }
}
