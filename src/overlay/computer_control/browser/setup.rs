use serde_json::{Value, json};

// The unpacked extension, shipped in the binary and written to disk on setup.
const EXT_MANIFEST: &[u8] = include_bytes!("../browser_ext/manifest.json");
const EXT_SW: &[u8] = include_bytes!("../browser_ext/sw.js");
const EXT_POPUP_HTML: &[u8] = include_bytes!("../browser_ext/popup.html");
const EXT_POPUP_JS: &[u8] = include_bytes!("../browser_ext/popup.js");
const EXT_ICON16: &[u8] = include_bytes!("../browser_ext/icon16.png");
const EXT_ICON48: &[u8] = include_bytes!("../browser_ext/icon48.png");
const EXT_ICON128: &[u8] = include_bytes!("../browser_ext/icon128.png");

fn ext_dir() -> std::path::PathBuf {
    crate::paths::app_runtime_config_dir().join("cc_browser_ext")
}

/// Extract the bundled extension to disk and return its folder (for "Load unpacked").
fn write_extension() -> anyhow::Result<std::path::PathBuf> {
    let dir = ext_dir();
    std::fs::create_dir_all(&dir)?;
    // Centralize the version on Cargo.toml: stamp it into the manifest at extract
    // time (Chrome wants a plain x.y.z, so drop any -pre/+build suffix).
    let ver = env!("CARGO_PKG_VERSION");
    let ver = ver.split(['-', '+']).next().unwrap_or(ver);
    let manifest = rendered_manifest(EXT_MANIFEST, ver)?;
    std::fs::write(dir.join("manifest.json"), manifest)?;
    std::fs::write(dir.join("sw.js"), EXT_SW)?;
    // Stamp the per-install bootstrap key into a script the service worker loads via
    // importScripts() on startup. The extension proves knowledge of it on first
    // connect to receive the durable secret - so the secret is never handed to an
    // unauthenticated local socket. (Re)written every setup so a fresh extract pairs.
    let boot =
        serde_json::to_string(&super::bridge::bootstrap_secret()).unwrap_or_else(|_| "\"\"".into());
    std::fs::write(
        dir.join("bootstrap.js"),
        format!("self.SGT_BOOTSTRAP = {boot};\n").as_bytes(),
    )?;
    std::fs::write(dir.join("popup.html"), EXT_POPUP_HTML)?;
    std::fs::write(dir.join("popup.js"), EXT_POPUP_JS)?;
    std::fs::write(dir.join("icon16.png"), EXT_ICON16)?;
    std::fs::write(dir.join("icon48.png"), EXT_ICON48)?;
    std::fs::write(dir.join("icon128.png"), EXT_ICON128)?;
    Ok(dir)
}

fn rendered_manifest(source: &[u8], version: &str) -> anyhow::Result<Vec<u8>> {
    let mut manifest: Value = serde_json::from_slice(source)?;
    manifest["version"] = json!(version);
    Ok(serde_json::to_vec_pretty(&manifest)?)
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
        BrowserInfo {
            ext_url: "edge://extensions",
            name: "Microsoft Edge",
            chromium: true,
        }
    } else if prog.contains("brave") {
        BrowserInfo {
            ext_url: "brave://extensions",
            name: "Brave",
            chromium: true,
        }
    } else if prog.contains("opera") {
        BrowserInfo {
            ext_url: "opera://extensions",
            name: "Opera",
            chromium: true,
        }
    } else if prog.contains("firefox") {
        BrowserInfo {
            ext_url: "chrome://extensions",
            name: "Firefox",
            chromium: false,
        }
    } else {
        BrowserInfo {
            ext_url: "chrome://extensions",
            name: "Google Chrome",
            chromium: true,
        }
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

/// Bring the bridge up and lay down the extension files. Existing installations
/// reconnect without setup UI; a first-time or staged legacy install returns
/// explicit user-only steps when its worker cannot reload itself.
pub(in crate::overlay::computer_control) fn setup() -> Value {
    super::ensure_started();
    // Stage the newest bundled files even for an existing install. A running MV3
    // worker still needs one extension reload before new protocol code takes over.
    let dir = match write_extension() {
        Ok(d) => d.display().to_string(),
        Err(e) => return super::err(e),
    };
    let connected = super::is_connected();
    let usable = connected && super::capabilities::usable();
    let update_staged = super::capabilities::update_staged();
    let self_reload_available = super::extension_update::available();
    let self_reload_error = match super::extension_update::request_if_staged() {
        Ok(Some(request)) => return self_reloading_result(request),
        Ok(None) => None,
        Err(error) => Some(error.to_string()),
    };
    let route = if self_reload_error.is_some() {
        SetupRoute::Manual
    } else {
        setup_route(
            connected,
            usable,
            super::ever_connected(),
            update_staged && !self_reload_available,
        )
    };
    if opens_pairing_before_response(route) {
        super::bridge::open_pairing_window();
    }
    match route {
        SetupRoute::Connected => {
            return connected_result();
        }
        SetupRoute::Reconnecting => {
            return reconnecting_result();
        }
        SetupRoute::Manual => {}
    }

    let browser = detect_browser();
    super::bridge::open_pairing_window(); // a fresh extension auto-pairs on connect
    if super::is_connected() && super::capabilities::usable() {
        return connected_result();
    }
    let connected = super::is_connected();
    let update_staged = super::capabilities::update_staged();
    json!({
        "ok": true,
        "connected": connected,
        "state": super::connection_state(),
        "code": if connected && update_staged {
            json!("BROWSER_EXTENSION_RELOAD_REQUIRED")
        } else if connected {
            json!("BROWSER_EXTENSION_INCOMPATIBLE")
        } else {
            json!("BROWSER_SETUP_NEEDS_EXTENSION_LOAD")
        },
        "browser": browser.name,
        "chromium": browser.chromium,
        "extensions_page": browser.ext_url,
        "extension_folder": dir,
        "port": super::bridge::port_for_display(),
        "self_reload_available": super::extension_update::available(),
        "self_reload_error": self_reload_error,
        "warning": if browser.chromium { Value::Null } else {
            json!(format!("Your default browser ({}) isn't Chromium - deep browser control needs a Chromium browser.", browser.name))
        },
        "manual_user_steps": [
            format!("In {}, open {} yourself. Browser extension-manager pages cannot be automated by Computer Control.", browser.name, browser.ext_url),
            "If the extension is already listed, manually press its Reload button once; otherwise choose Load unpacked.",
            format!("For Load unpacked, select this folder: {dir}"),
            "After that user action, check browser_status up to three times and stop as soon as connected is true."
        ]
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupRoute {
    Connected,
    Reconnecting,
    Manual,
}

fn setup_route(
    connected: bool,
    usable: bool,
    ever_connected: bool,
    manual_update_required: bool,
) -> SetupRoute {
    if connected && usable && !manual_update_required {
        SetupRoute::Connected
    } else if !connected && ever_connected {
        SetupRoute::Reconnecting
    } else {
        SetupRoute::Manual
    }
}

fn opens_pairing_before_response(route: SetupRoute) -> bool {
    route == SetupRoute::Reconnecting
}

fn connected_result() -> Value {
    json!({
        "ok": true,
        "connected": true,
        "state": "connected",
        "protocol_version": super::capabilities::protocol_version(),
        "capabilities": super::capabilities::list(),
        "update_staged": super::capabilities::update_staged(),
        "reload_required": false,
        "self_reload_available": super::extension_update::available(),
        "note": "Browser control is connected and usable for its negotiated capabilities. Do not reinstall it. A staged update is optional until a requested command reports a typed missing-capability error."
    })
}

fn self_reloading_result(request: super::extension_update::ReloadRequest) -> Value {
    let newly_requested = request == super::extension_update::ReloadRequest::Requested;
    json!({
        "ok": true,
        "connected": super::is_connected(),
        "state": "updating",
        "code": "BROWSER_EXTENSION_SELF_RELOAD_STARTED",
        "protocol_version": super::capabilities::protocol_version(),
        "target_protocol_version": super::capabilities::CURRENT_PROTOCOL,
        "capabilities": super::capabilities::list(),
        "update_staged": true,
        "reload_required": false,
        "self_reload_available": true,
        "self_reload_requested": newly_requested,
        "retry": {
            "tool": "browser_status",
            "max_attempts": 4,
            "delay_ms": 1000,
            "stop_when": "protocol_version equals target_protocol_version and connected is true"
        },
        "instruction": "The connected extension accepted the staged update. Check browser_status at most four times about one second apart, then stop and report the typed state if the target protocol has not connected."
    })
}

fn reconnecting_result() -> Value {
    reconnecting_payload(
        super::capabilities::protocol_version(),
        super::capabilities::list(),
    )
}

fn reconnecting_payload(protocol_version: u64, capabilities: Vec<String>) -> Value {
    json!({
        "ok": true,
        "connected": false,
        "state": "reconnecting",
        "code": "BROWSER_EXTENSION_RECONNECTING",
        "protocol_version": protocol_version,
        "capabilities": capabilities,
        "retry": {
            "tool": "browser_status",
            "max_attempts": 4,
            "delay_ms": 2000,
            "stop_when": "connected is true"
        },
        "instruction": "The previously connected extension is reconnecting through a fresh pairing window. Do not rerun browser_setup or start installation. Check browser_status at most four times about two seconds apart, then report the current typed state if it is still disconnected."
    })
}

/// Reset / repair browser control: re-open the pairing window so a stuck or
/// stale-secret extension re-pairs cleanly.
/// The clean alternative to manually deleting files - no raw deletion, bounded.
pub(in crate::overlay::computer_control) fn reset() -> Value {
    super::prefs::clear();
    super::bridge::open_pairing_window(); // a loaded extension re-pairs within the window
    json!({
        "ok": true,
        "state": super::connection_state(),
        "note": "Browser-control pairing reset: a loaded, ENABLED extension re-pairs in a second or two (instant, not minutes). If it stays disconnected, toggle Developer mode off/on (Chrome often soft-disables it) or reload it on the extensions page; to fully UNINSTALL, the user removes it there."
    })
}

pub(in crate::overlay::computer_control) fn status() -> Value {
    // Deliberately does NOT return the pairing secret: with auto-pair the model
    // never needs it, and exposing it widens the blast radius if a transcript leaks.
    let connected = super::is_connected();
    let usable = connected && super::capabilities::usable();
    let (update_staged, reload_required) =
        update_flags(connected, usable, super::capabilities::update_staged());
    json!({
        "ok": true,
        "connected": connected,
        "state": super::connection_state(),
        "pairing_window_open": super::bridge::pairing_window_open(),
        "protocol_version": super::capabilities::protocol_version(),
        "capabilities": super::capabilities::list(),
        "usable": usable,
        "update_staged": update_staged,
        "reload_required": reload_required,
        "self_reload_available": update_staged && super::extension_update::available(),
        "port": super::bridge::port_for_display()
    })
}

fn update_flags(connected: bool, usable: bool, staged: bool) -> (bool, bool) {
    (connected && staged, connected && !usable && staged)
}

#[cfg(test)]
mod tests {
    use super::{
        EXT_MANIFEST, SetupRoute, opens_pairing_before_response, reconnecting_payload,
        rendered_manifest, setup_route, update_flags,
    };
    use serde_json::Value;

    #[test]
    fn manifest_version_is_stamped_without_a_placeholder_dependency() {
        let rendered = rendered_manifest(EXT_MANIFEST, "7.8.9").unwrap();
        let value: Value = serde_json::from_slice(&rendered).unwrap();
        assert_eq!(value["version"], "7.8.9");
    }

    #[test]
    fn embedded_worker_protocol_matches_rust_contract() {
        let source = std::str::from_utf8(super::EXT_SW).unwrap();
        let declaration = format!(
            "const BRIDGE_PROTOCOL = {};",
            super::super::capabilities::CURRENT_PROTOCOL
        );
        assert!(source.contains(&declaration));
        assert!(source.contains(super::super::capabilities::RUNTIME_RELOAD));
    }

    #[test]
    fn compatible_connected_extension_keeps_optional_update_non_blocking() {
        assert_eq!(update_flags(true, true, true), (true, false));
        assert_eq!(update_flags(true, false, true), (true, true));
        assert_eq!(update_flags(false, false, true), (false, false));
    }

    #[test]
    fn setup_route_preserves_usable_connected_control() {
        assert_eq!(setup_route(true, true, true, false), SetupRoute::Connected);
    }

    #[test]
    fn setup_surfaces_a_staged_worker_that_cannot_reload_itself() {
        assert_eq!(setup_route(true, true, true, true), SetupRoute::Manual);
    }

    #[test]
    fn setup_route_separates_reconnect_from_first_time_setup() {
        assert_eq!(
            setup_route(false, false, true, false),
            SetupRoute::Reconnecting
        );
        assert_eq!(setup_route(false, false, false, false), SetupRoute::Manual);
        assert_eq!(setup_route(true, false, true, false), SetupRoute::Manual);
    }

    #[test]
    fn prior_connection_recovery_opens_pairing_without_waiting() {
        let route = setup_route(false, false, true, false);
        assert_eq!(route, SetupRoute::Reconnecting);
        assert!(opens_pairing_before_response(route));
        assert!(!opens_pairing_before_response(SetupRoute::Connected));
    }

    #[test]
    fn reconnecting_payload_is_bounded_and_has_no_install_route() {
        let payload = reconnecting_payload(1, vec!["cdp.command".to_string()]);
        assert_eq!(payload["state"], "reconnecting");
        assert_eq!(payload["retry"]["tool"], "browser_status");
        assert_eq!(payload["retry"]["max_attempts"], 4);
        assert_eq!(payload["retry"]["delay_ms"], 2000);
        assert!(payload.get("extensions_page").is_none());
        assert!(payload.get("extension_folder").is_none());
        assert!(payload.get("manual_user_steps").is_none());
    }
}
