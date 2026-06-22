//! Deep browser control for the Computer Control agent: a thin Chrome MV3
//! extension (a CDP-over-WebSocket bridge) + this Rust side that hosts the socket
//! and implements high-level tools on raw CDP. See `temp-browser-extension-design.md`.
//!
//! The extension runs in the user's REAL logged-in session via `chrome.debugger`
//! (the only route left after Chrome 136 blocked `--remote-debugging-port` on the
//! default profile). All logic lives here in Rust; the extension just forwards CDP.

mod bridge;
mod crypto;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) use bridge::{ensure_started, is_connected};

// ── Proactive-offer preferences (persisted across sessions) ──────────────────

/// Tracks whether/when we may proactively offer to set up browser control, so a
/// user who declined isn't nagged - but is asked again much later.
#[derive(Serialize, Deserialize, Default)]
struct OfferPrefs {
    declined: u32,
    snooze_until: u64, // unix seconds; offers are suppressed until this time
}

fn prefs_path() -> std::path::PathBuf {
    crate::paths::app_config_dir().join("cc_browser_prefs.json")
}

fn load_prefs() -> OfferPrefs {
    std::fs::File::open(prefs_path())
        .ok()
        .and_then(|f| serde_json::from_reader(f).ok())
        .unwrap_or_default()
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Whether a proactive offer is allowed now (past any post-decline snooze).
pub(super) fn offer_due() -> bool {
    now_secs() >= load_prefs().snooze_until
}

/// The user declined the offer → back off, longer each time, but never forever
/// (3 days, then 2 weeks, then ~2 months — after which we ask again once).
pub(super) fn record_decline() {
    let mut p = load_prefs();
    p.declined = p.declined.saturating_add(1);
    let days: u64 = match p.declined {
        1 => 3,
        2 => 14,
        _ => 60,
    };
    p.snooze_until = now_secs() + days * 86_400;
    let _ = crate::atomic_json::write_json_atomic(&prefs_path(), &p);
}

// The unpacked extension, shipped in the binary and written to disk on setup.
const EXT_MANIFEST: &[u8] = include_bytes!("../browser_ext/manifest.json");
const EXT_SW: &[u8] = include_bytes!("../browser_ext/sw.js");
const EXT_POPUP_HTML: &[u8] = include_bytes!("../browser_ext/popup.html");
const EXT_POPUP_JS: &[u8] = include_bytes!("../browser_ext/popup.js");
const EXT_ICON16: &[u8] = include_bytes!("../browser_ext/icon16.png");
const EXT_ICON48: &[u8] = include_bytes!("../browser_ext/icon48.png");
const EXT_ICON128: &[u8] = include_bytes!("../browser_ext/icon128.png");

fn err(e: anyhow::Error) -> Value {
    json!({"ok": false, "error": e.to_string()})
}

fn not_connected() -> Value {
    json!({
        "ok": false,
        "error": "the browser extension isn't connected",
        "hint": "Call browser_setup, then guide the user to load the extension + paste the pairing code."
    })
}

/// Run `Runtime.evaluate` and return its by-value result (or a JS-exception error).
fn eval_value(expr: &str) -> anyhow::Result<Value> {
    let r = bridge::cdp(
        "Runtime.evaluate",
        json!({ "expression": expr, "returnByValue": true, "awaitPromise": true }),
    )?;
    if let Some(exc) = r.get("exceptionDetails") {
        anyhow::bail!("js exception: {}", exc.get("text").and_then(Value::as_str).unwrap_or("error"));
    }
    Ok(r.get("result").and_then(|x| x.get("value")).cloned().unwrap_or(Value::Null))
}

// ── Setup / status ───────────────────────────────────────────────────────────

fn ext_dir() -> std::path::PathBuf {
    crate::paths::app_config_dir().join("cc_browser_ext")
}

/// Extract the bundled extension to disk and return its folder (for "Load unpacked").
fn write_extension() -> anyhow::Result<std::path::PathBuf> {
    let dir = ext_dir();
    std::fs::create_dir_all(&dir)?;
    // Centralize the version on Cargo.toml: stamp it into the manifest at extract
    // time (Chrome wants a plain x.y.z, so drop any -pre/+build suffix).
    let ver = env!("CARGO_PKG_VERSION");
    let ver = ver.split(['-', '+']).next().unwrap_or(ver);
    let manifest = String::from_utf8_lossy(EXT_MANIFEST)
        .replace("\"version\": \"0.1.0\"", &format!("\"version\": \"{ver}\""));
    std::fs::write(dir.join("manifest.json"), manifest.as_bytes())?;
    std::fs::write(dir.join("sw.js"), EXT_SW)?;
    std::fs::write(dir.join("popup.html"), EXT_POPUP_HTML)?;
    std::fs::write(dir.join("popup.js"), EXT_POPUP_JS)?;
    std::fs::write(dir.join("icon16.png"), EXT_ICON16)?;
    std::fs::write(dir.join("icon48.png"), EXT_ICON48)?;
    std::fs::write(dir.join("icon128.png"), EXT_ICON128)?;
    Ok(dir)
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The user's browser, as far as the extension flow needs to know.
struct BrowserInfo {
    ext_url: &'static str, // the …://extensions page
    name: &'static str,    // display name
    chromium: bool,        // deep control only works on Chromium browsers
}

/// Detect the default browser from the https UserChoice ProgId and map it to a
/// Chromium browser. Firefox/unknown → fall back to Chrome (and flag it).
fn detect_browser() -> BrowserInfo {
    let prog = default_https_progid().unwrap_or_default().to_lowercase();
    if prog.contains("msedge") || prog.contains("edge") {
        BrowserInfo { ext_url: "edge://extensions", name: "Microsoft Edge", chromium: true }
    } else if prog.contains("brave") {
        BrowserInfo { ext_url: "brave://extensions", name: "Brave", chromium: true }
    } else if prog.contains("opera") {
        BrowserInfo { ext_url: "opera://extensions", name: "Opera", chromium: true }
    } else if prog.contains("firefox") {
        BrowserInfo { ext_url: "chrome://extensions", name: "Firefox", chromium: false }
    } else {
        BrowserInfo { ext_url: "chrome://extensions", name: "Google Chrome", chromium: true }
    }
}

fn default_https_progid() -> Option<String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice",
            "/v",
            "ProgId",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .find(|l| l.contains("ProgId") && l.contains("REG_SZ"))
        .and_then(|l| l.split_whitespace().last())
        .map(str::to_string)
}


/// Bring the bridge up, lay down the extension files, and open chrome://extensions.
/// Returns what the agent needs to finish the install ITSELF (it should perform
/// the clicks, not recite them to the user) — pausing only at the permission grant.
pub(super) fn setup() -> Value {
    ensure_started();
    // Already connected → nothing to do. Without this the agent would re-open
    // chrome://extensions and re-run the whole install, looping after success.
    if is_connected() {
        return json!({
            "ok": true,
            "connected": true,
            "note": "Browser control is ALREADY set up and connected - do NOT install again. Tell the user it's ready and stop."
        });
    }
    let dir = match write_extension() {
        Ok(d) => d.display().to_string(),
        Err(e) => return err(e),
    };
    let browser = detect_browser();
    bridge::open_pairing_window(); // ~2 min: a fresh extension auto-pairs, no popup
    json!({
        "ok": true,
        "connected": is_connected(),
        "browser": browser.name,
        "chromium": browser.chromium,
        "extensions_page": browser.ext_url,
        "extension_folder": dir,
        "port": bridge::port_for_display(),
        "warning": if browser.chromium { Value::Null } else {
            json!(format!("Your default browser ({}) isn't Chromium - deep browser control needs Chrome/Edge/Brave.", browser.name))
        },
        "do_yourself": [
            format!("In the EXISTING browser window (focus_window it if needed - do NOT open a new window), go to {} by typing it in the ADDRESS BAR: key_combination 'Ctrl+L', then type_text '{}' with press_enter:true.", browser.ext_url, browser.ext_url),
            "Toggle ON 'Developer mode' (top-right) ONLY if it's off - look() first; don't toggle what's already on. Click the SWITCH (far right), then look() to confirm it flipped - DON'T retry on 'no visual change' (the detector misses tiny toggles).",
            "Click 'Load unpacked'. In the file dialog, type_text the extension_folder path with press_enter:true, then click 'Select Folder'.",
            "A permission prompt USUALLY does NOT appear for unpacked extensions - do NOT wait for one or tell the user to confirm it. If one happens to show, pause for the user; otherwise just continue.",
            "That's it - NO popup, NO pairing code. The extension auto-pairs over the socket within ~2 minutes.",
            "Poll browser_status (wait a few seconds between tries) until 'connected' is true, then STOP."
        ]
    })
}

/// Reset / repair browser control: re-open the pairing window so a stuck or
/// stale-secret extension re-pairs cleanly, and re-enable the proactive offer.
/// The clean alternative to manually deleting files - no raw deletion, bounded.
pub(super) fn reset() -> Value {
    let _ = std::fs::remove_file(prefs_path()); // re-enable the setup offer
    bridge::open_pairing_window(); // a loaded extension re-pairs within the window
    json!({
        "ok": true,
        "note": "Browser-control pairing reset: a loaded extension re-pairs within ~2 minutes. If it still won't connect, reload it on the browser's extensions page; to fully UNINSTALL, the user removes it there."
    })
}

pub(super) fn status() -> Value {
    json!({
        "ok": true,
        "connected": is_connected(),
        "pairing_code": bridge::pairing_code(),
        "port": bridge::port_for_display()
    })
}

// ── Page tools (all guard on connection) ─────────────────────────────────────

macro_rules! require_conn {
    () => {
        if !is_connected() {
            return not_connected();
        }
    };
}

pub(super) fn read_page() -> Value {
    require_conn!();
    let js = r#"(() => ({ title: document.title, url: location.href,
        text: (document.body ? document.body.innerText : "").slice(0, 12000) }))()"#;
    match eval_value(js) {
        Ok(v) => json!({"ok": true, "page": v}),
        Err(e) => err(e),
    }
}

pub(super) fn eval_js(code: &str) -> Value {
    require_conn!();
    match eval_value(code) {
        Ok(v) => json!({"ok": true, "result": v}),
        Err(e) => err(e),
    }
}

pub(super) fn query(selector: &str) -> Value {
    require_conn!();
    let js = format!(
        r#"(() => {{
            const els = [...document.querySelectorAll({sel})].slice(0, 50);
            return els.map(e => {{ const r = e.getBoundingClientRect();
                return {{ text: (e.innerText || e.value || "").slice(0,120).trim(),
                    tag: e.tagName.toLowerCase(),
                    rect: [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)] }}; }});
        }})()"#,
        sel = json!(selector)
    );
    match eval_value(&js) {
        Ok(v) => json!({"ok": true, "matches": v}),
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
        Ok(Value::Null) => return json!({"ok": false, "error": format!("no element matches {selector}")}),
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
    bridge::cdp("Input.dispatchMouseEvent", json!({"type":"mouseMoved","x":x,"y":y}))?;
    bridge::cdp(
        "Input.dispatchMouseEvent",
        json!({"type":"mousePressed","x":x,"y":y,"button":"left","buttons":1,"clickCount":1}),
    )?;
    bridge::cdp(
        "Input.dispatchMouseEvent",
        json!({"type":"mouseReleased","x":x,"y":y,"button":"left","buttons":0,"clickCount":1}),
    )?;
    Ok(())
}

pub(super) fn fill(selector: &str, text: &str) -> Value {
    require_conn!();
    let js = format!(
        r#"(() => {{ const e = document.querySelector({sel}); if (!e) return false;
            e.focus(); if (e.select) e.select(); return true; }})()"#,
        sel = json!(selector)
    );
    match eval_value(&js) {
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
    let js = format!("(() => !!document.querySelector({sel}))()", sel = json!(selector));
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

pub(super) fn upload_file(selector: &str, path: &str) -> Value {
    require_conn!();
    let doc = match bridge::cdp("DOM.getDocument", json!({"depth": 0})) {
        Ok(v) => v,
        Err(e) => return err(e),
    };
    let root = doc.get("root").and_then(|r| r.get("nodeId")).and_then(Value::as_i64).unwrap_or(0);
    let q = match bridge::cdp("DOM.querySelector", json!({"nodeId": root, "selector": selector})) {
        Ok(v) => v,
        Err(e) => return err(e),
    };
    let node = q.get("nodeId").and_then(Value::as_i64).unwrap_or(0);
    if node == 0 {
        return json!({"ok": false, "error": format!("no element matches {selector}")});
    }
    match bridge::cdp("DOM.setFileInputFiles", json!({"nodeId": node, "files": [path]})) {
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
    match bridge::rpc("tabs", json!({"action": "activate", "tabId": tab_id})) {
        Ok(_) => json!({"ok": true, "switched": tab_id}),
        Err(e) => err(e),
    }
}

pub(super) fn read_network(filter: &str) -> Value {
    require_conn!();
    let _ = bridge::cdp("Network.enable", json!({})); // idempotent; starts the feed
    let want = if filter.is_empty() { "Network." } else { filter };
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
