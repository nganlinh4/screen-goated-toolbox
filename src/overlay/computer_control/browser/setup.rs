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

/// Bring the bridge up, lay down the extension files, and open chrome://extensions.
/// Returns what the agent needs to finish the install ITSELF (it should perform
/// the clicks, not recite them to the user) — pausing only at the permission grant.
pub(in crate::overlay::computer_control) fn setup() -> Value {
    super::ensure_started();
    // Stage the newest bundled files even for an existing install. A running MV3
    // worker still needs one extension reload before new protocol code takes over.
    let dir = match write_extension() {
        Ok(d) => d.display().to_string(),
        Err(e) => return super::err(e),
    };
    if super::is_connected() && super::bridge::protocol_version() >= 2 {
        return json!({
            "ok": true,
            "connected": true,
            "state": "connected",
            "note": "Browser control is ALREADY set up and connected - do NOT install again. Tell the user it's ready and stop."
        });
    }
    let browser = detect_browser();
    super::bridge::open_pairing_window(); // a fresh extension auto-pairs INSTANTLY on connect, no popup
    json!({
        "ok": true,
        "connected": super::is_connected(),
        "state": super::connection_state(),
        "code": if super::is_connected() { json!("BROWSER_EXTENSION_RELOAD_REQUIRED") } else { json!("BROWSER_SETUP_NEEDS_EXTENSION_LOAD") },
        "browser": browser.name,
        "chromium": browser.chromium,
        "extensions_page": browser.ext_url,
        "extension_folder": dir,
        "port": super::bridge::port_for_display(),
        "warning": if browser.chromium { Value::Null } else {
            json!(format!("Your default browser ({}) isn't Chromium - deep browser control needs Chrome/Edge/Brave.", browser.name))
        },
        "do_yourself": [
            format!("Open a new tab in the current browser window, go to {}, and use observe/act or keyboard controls from there.", browser.ext_url),
            "If the extension is already listed, press its Reload button once so the staged protocol update takes effect; otherwise continue with Load unpacked.",
            "For Load unpacked, type_text the extension_folder path with press_enter:true, then click 'Select Folder'.",
            "Check browser_status after each visible setup step. If connected is true, stop; if it remains false after a bounded recovery, report the blocker instead of looping."
        ]
    })
}

/// Reset / repair browser control: re-open the pairing window so a stuck or
/// stale-secret extension re-pairs cleanly, and re-enable the proactive offer.
/// The clean alternative to manually deleting files - no raw deletion, bounded.
pub(in crate::overlay::computer_control) fn reset() -> Value {
    super::prefs::clear(); // re-enable the setup offer
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
    json!({
        "ok": true,
        "connected": super::is_connected(),
        "state": super::connection_state(),
        "pairing_window_open": super::bridge::pairing_window_open(),
        "protocol_version": super::bridge::protocol_version(),
        "reload_required": super::is_connected() && super::bridge::protocol_version() < 2,
        "port": super::bridge::port_for_display()
    })
}
