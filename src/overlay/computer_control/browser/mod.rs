//! Deep browser control for the Computer Control agent: a thin Chrome MV3
//! extension (a CDP-over-WebSocket bridge) + this Rust side that hosts the socket
//! and implements high-level tools on raw CDP. See `temp-browser-extension-design.md`.
//!
//! The extension runs in the user's REAL logged-in session via `chrome.debugger`
//! (the only route left after Chrome 136 blocked `--remote-debugging-port` on the
//! default profile). All logic lives here in Rust; the extension just forwards CDP.

mod bridge;
mod crypto;
mod page;
mod prefs;
mod setup;

use serde_json::{Value, json};
use std::time::Duration;

pub(super) use bridge::{ensure_started, is_connected};
pub(super) use page::read_page_on_tab;
pub(super) use page::{extract_page, read_page};
pub(super) use prefs::{
    ever_connected, offer_due, recently_connected, record_connection, record_decline,
};
pub(super) use setup::{reset, setup, status};

fn err(e: anyhow::Error) -> Value {
    json!({"ok": false, "code": "ERR_BROWSER_TOOL_FAILED", "error": e.to_string()})
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
    let r = bridge::cdp_on_tab(
        "Runtime.evaluate",
        json!({ "expression": expr, "returnByValue": true, "awaitPromise": true }),
        tab_id,
    )?;
    if let Some(exc) = r.get("exceptionDetails") {
        anyhow::bail!("js exception: {}", js_exception_text(exc));
    }
    Ok(r.get("result")
        .and_then(|x| x.get("value"))
        .cloned()
        .unwrap_or(Value::Null))
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
    Ok(r.get("result")
        .and_then(|x| x.get("value"))
        .cloned()
        .unwrap_or(Value::Null))
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
    use std::collections::HashSet;
    let detached: HashSet<String> = bridge::recent_events("Target.detachedFromTarget", 50)
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
    for e in bridge::recent_events("Target.attachedToTarget", 50) {
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

/// Connection gate for the page tools. Three cases when not currently connected:
///  - recently connected → the installed extension is just napping/restarting (it
///    auto-reconnects on the fixed port); wait briefly, then ask the model to retry.
///  - connected before but not for a while → it may have been removed or the browser
///    closed; let the model re-run setup if the user still wants it.
///  - never connected → not set up; point at browser_setup.
///
/// `Some(error)` = bail, `None` = proceed.
fn conn_guard() -> Option<Value> {
    if is_connected() {
        return None;
    }
    if ever_connected() {
        // Set up before → it auto-reconnects on the fixed port. Wait briefly for the
        // service worker / restarted browser to come back before deciding it's gone.
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(100));
            if is_connected() {
                return None;
            }
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

pub(super) fn eval_js(code: &str) -> Value {
    require_conn!();
    match eval_value(code) {
        Ok(v) => json!({"ok": true, "result": v}),
        Err(e) => err(e),
    }
}

pub(super) fn click_selector(selector: &str) -> Value {
    require_conn!();
    let js = format!(
        r#"(() => {{ const e = document.querySelector({sel});
            if (!e) return null; e.scrollIntoView({{block:'center', inline:'center'}});
            const r = e.getBoundingClientRect();
            return {{ x: r.left + r.width/2, y: r.top + r.height/2 }}; }})()"#,
        sel = json!(selector)
    );
    let pt = match eval_value(&js) {
        Ok(Value::Null) => {
            return json!({"ok": false, "error": format!("no element matches {selector}")});
        }
        Ok(v) => v,
        Err(e) => return err(e),
    };
    let (x, y) = (
        pt.get("x").and_then(Value::as_f64).unwrap_or(0.0),
        pt.get("y").and_then(Value::as_f64).unwrap_or(0.0),
    );
    if let Err(e) = dispatch_click(x, y) {
        return err(e);
    }
    json!({"ok": true, "clicked": [x.round(), y.round()]})
}

fn dispatch_click(x: f64, y: f64) -> anyhow::Result<()> {
    click(x, y, false)
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
/// input pipeline: deep control is connected AND a Chromium browser is foreground.
pub(super) fn input_active() -> bool {
    is_connected() && super::uia::foreground_is_chromium()
}

/// Capture the controlled tab's viewport as a JPEG, with its CSS width/height so a
/// 0-1000 normalized vision hit can be scaled to the CSS px CDP input expects.
pub(super) fn shot() -> anyhow::Result<(Vec<u8>, f64, f64)> {
    let size = eval_value("({w: window.innerWidth, h: window.innerHeight})")?;
    let cw = size.get("w").and_then(Value::as_f64).unwrap_or(0.0);
    let ch = size.get("h").and_then(Value::as_f64).unwrap_or(0.0);
    if cw < 1.0 || ch < 1.0 {
        anyhow::bail!("browser viewport has no size");
    }
    let _ = bridge::cdp("Page.enable", json!({})); // idempotent; some builds need it
    let r = bridge::cdp(
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

/// Trusted left/right click at a CSS-px point.
pub(super) fn click(x: f64, y: f64, right: bool) -> anyhow::Result<()> {
    let (button, mask) = if right { ("right", 2) } else { ("left", 1) };
    bridge::cdp(
        "Input.dispatchMouseEvent",
        json!({"type":"mouseMoved","x":x,"y":y}),
    )?;
    bridge::cdp(
        "Input.dispatchMouseEvent",
        json!({"type":"mousePressed","x":x,"y":y,"button":button,"buttons":mask,"clickCount":1}),
    )?;
    bridge::cdp(
        "Input.dispatchMouseEvent",
        json!({"type":"mouseReleased","x":x,"y":y,"button":button,"buttons":0,"clickCount":1}),
    )?;
    Ok(())
}

/// Trusted press-glide-release drag between two CSS-px points, shaped like a real
/// human drag so pointer/canvas games and HTML5 drag-and-drop reliably latch it:
///   1. press, then HOLD - lets the page's pointerdown/grab settle before motion
///      (a press immediately followed by movement reads as a stray click, which is
///      why fast drags intermittently "spring back" instead of picking the item up);
///   2. a STEADY (linear) glide of many held mouseMoved events - steady motion
///      crosses velocity/threshold-based drag detection cleanly;
///   3. dwell on the drop target before release - so dragover/drop fires there.
pub(super) fn drag(fx: f64, fy: f64, tx: f64, ty: f64) -> anyhow::Result<()> {
    const GRAB_HOLD_MS: u64 = 110; // settle the grab before moving
    const DROP_HOLD_MS: u64 = 110; // settle on the target before releasing
    const STEPS: i32 = 28;
    bridge::cdp(
        "Input.dispatchMouseEvent",
        json!({"type":"mouseMoved","x":fx,"y":fy}),
    )?;
    bridge::cdp(
        "Input.dispatchMouseEvent",
        json!({"type":"mousePressed","x":fx,"y":fy,"button":"left","buttons":1,"clickCount":1}),
    )?;
    std::thread::sleep(Duration::from_millis(GRAB_HOLD_MS));
    for i in 1..=STEPS {
        let t = f64::from(i) / f64::from(STEPS);
        let (x, y) = (fx + (tx - fx) * t, fy + (ty - fy) * t);
        bridge::cdp(
            "Input.dispatchMouseEvent",
            json!({"type":"mouseMoved","x":x,"y":y,"button":"left","buttons":1}),
        )?;
        std::thread::sleep(Duration::from_millis(14));
    }
    std::thread::sleep(Duration::from_millis(DROP_HOLD_MS));
    bridge::cdp(
        "Input.dispatchMouseEvent",
        json!({"type":"mouseReleased","x":tx,"y":ty,"button":"left","buttons":0,"clickCount":1}),
    )?;
    Ok(())
}

/// Focus the element (in its frame - `session` None = top, Some = a cross-origin
/// iframe) and type `text` via a trusted `Input.insertText` so input/change fire.
pub(super) fn fill_in(selector: &str, text: &str, session: Option<&str>) -> Value {
    require_conn!();
    let js = format!(
        r#"(() => {{ const e = document.querySelector({sel}); if (!e) return false;
            e.focus(); if (e.select) e.select(); return true; }})()"#,
        sel = json!(selector)
    );
    match eval_value_in(&js, session) {
        Ok(Value::Bool(true)) => {}
        Ok(_) => return json!({"ok": false, "error": format!("no element matches {selector}")}),
        Err(e) => return err(e),
    }
    // Trusted insert so input/change fire (vs setting .value, which doesn't).
    match bridge::cdp("Input.insertText", json!({"text": text})) {
        Ok(_) => json!({"ok": true, "filled": selector}),
        Err(e) => err(e),
    }
}

pub(super) fn wait_for(selector: &str, timeout_ms: u64) -> Value {
    require_conn!();
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms.min(30_000));
    let js = format!(
        "(() => !!document.querySelector({sel}))()",
        sel = json!(selector)
    );
    loop {
        match eval_value(&js) {
            Ok(Value::Bool(true)) => return json!({"ok": true, "found": selector}),
            Ok(_) => {}
            Err(e) => return err(e),
        }
        if std::time::Instant::now() > deadline {
            return json!({"ok": false, "error": format!("'{selector}' not found within {timeout_ms}ms")});
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub(super) fn navigate(url: &str) -> Value {
    require_conn!();
    match bridge::cdp("Page.navigate", json!({"url": url})) {
        Ok(_) => json!({"ok": true, "navigated": url}),
        Err(e) => err(e),
    }
}

/// Open `url` in a NEW tab of the current window (keeps the existing page).
pub(super) fn open_tab(url: &str) -> Value {
    require_conn!();
    match bridge::rpc("tabs", json!({"action": "create", "url": url})) {
        Ok(v) => json!({"ok": true, "tab": v}),
        Err(e) => err(e),
    }
}

pub(super) fn open_background_tab(url: &str) -> anyhow::Result<i64> {
    if bridge::protocol_version() < 2 {
        anyhow::bail!(
            "browser extension update is staged but must be reloaded before background tabs are safe"
        );
    }
    let tab = bridge::rpc(
        "tabs",
        json!({"action": "create", "url": url, "active": false}),
    )?;
    tab.get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow::anyhow!("browser did not return a temporary tab id"))
}

pub(super) fn close_tab(tab_id: i64) -> anyhow::Result<()> {
    bridge::rpc("tabs", json!({"action": "remove", "tabId": tab_id}))?;
    Ok(())
}

pub(super) fn upload_file(selector: &str, path: &str) -> Value {
    require_conn!();
    let doc = match bridge::cdp("DOM.getDocument", json!({"depth": 0})) {
        Ok(v) => v,
        Err(e) => return err(e),
    };
    let root = doc
        .get("root")
        .and_then(|r| r.get("nodeId"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let q = match bridge::cdp(
        "DOM.querySelector",
        json!({"nodeId": root, "selector": selector}),
    ) {
        Ok(v) => v,
        Err(e) => return err(e),
    };
    let node = q.get("nodeId").and_then(Value::as_i64).unwrap_or(0);
    if node == 0 {
        return json!({"ok": false, "error": format!("no element matches {selector}")});
    }
    match bridge::cdp(
        "DOM.setFileInputFiles",
        json!({"nodeId": node, "files": [path]}),
    ) {
        Ok(_) => json!({"ok": true, "uploaded": path}),
        Err(e) => err(e),
    }
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
    json!({"ok": true, "switched": tab_id, "title": title, "url": url})
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

pub(super) fn read_network(filter: &str) -> Value {
    require_conn!();
    let _ = bridge::cdp("Network.enable", json!({})); // idempotent; starts the feed
    let want = if filter.is_empty() {
        "Network."
    } else {
        filter
    };
    let items: Vec<Value> = bridge::recent_events(want, 30)
        .iter()
        .map(|e| {
            let p = e.get("params").cloned().unwrap_or_else(|| json!({}));
            json!({
                "method": e.get("method"),
                "url": p.get("response").and_then(|r| r.get("url"))
                    .or_else(|| p.get("request").and_then(|r| r.get("url"))),
                "status": p.get("response").and_then(|r| r.get("status")),
            })
        })
        .collect();
    json!({"ok": true, "events": items,
        "note": "Network just enabled if it wasn't - call again after the page makes requests."})
}

/// Read the page's recent CONSOLE output - both console.log/info/warn/error(...)
/// calls and browser LOG entries (CORS / security / network / deprecation errors).
/// Enables the feed if needed, so the FIRST call may be empty - call again after the
/// page runs. Lets the agent debug a web app without opening DevTools.
pub(super) fn read_console() -> Value {
    require_conn!();
    let _ = bridge::cdp("Runtime.enable", json!({})); // console.* calls
    let _ = bridge::cdp("Log.enable", json!({})); // browser log entries (CORS, etc.)
    let mut items: Vec<Value> = Vec::new();
    for e in bridge::recent_events("consoleAPICalled", 25) {
        let p = e.get("params").cloned().unwrap_or_else(|| json!({}));
        let text = p
            .get("args")
            .and_then(Value::as_array)
            .map(|args| {
                args.iter()
                    .map(|a| {
                        a.get("value")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .or_else(|| {
                                a.get("description")
                                    .and_then(Value::as_str)
                                    .map(str::to_string)
                            })
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        items.push(json!({"level": p.get("type"), "text": text}));
    }
    for e in bridge::recent_events("Log.entryAdded", 25) {
        let entry = e
            .get("params")
            .and_then(|p| p.get("entry"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        items.push(json!({"level": entry.get("level"), "text": entry.get("text"), "url": entry.get("url")}));
    }
    json!({"ok": true, "console": items,
        "note": "console.* + browser log entries since capture started - call again after the page runs/logs more."})
}
