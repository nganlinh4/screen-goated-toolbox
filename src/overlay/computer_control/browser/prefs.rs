//! Persisted connection state for deep browser control. It distinguishes a brief
//! extension reconnect from a connection that has never been established.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Default)]
struct ConnectionPrefs {
    #[serde(default)]
    connected_at: u64, // unix seconds of the LAST live connection (0 = never set up here)
}

/// How long after the last live connection we still treat a disconnect as "the
/// installed extension is just reconnecting" rather than "it's gone / never set up".
/// Covers MV3 service-worker naps and app/browser restarts (fixed port → auto-reconnect);
/// once it lapses, a genuine removal self-heals back to offering setup again.
const CONN_GRACE_SECS: u64 = 180;

fn prefs_path() -> std::path::PathBuf {
    crate::paths::app_config_dir().join("cc_browser_prefs.json")
}

fn load_prefs() -> ConnectionPrefs {
    std::fs::File::open(prefs_path())
        .ok()
        .and_then(|f| serde_json::from_reader(f).ok())
        .unwrap_or_default()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Whether deep browser control was set up here at all (ever connected). Distinguishes
/// "the user must install it" from "it's installed but momentarily disconnected".
pub(crate) fn ever_connected() -> bool {
    load_prefs().connected_at > 0
}

/// Whether we were connected recently enough that a current disconnect is almost
/// certainly the installed extension reconnecting (nap / restart), not a removal.
pub(crate) fn recently_connected() -> bool {
    let at = load_prefs().connected_at;
    at > 0 && now_secs().saturating_sub(at) < CONN_GRACE_SECS
}

/// Record that we are/were connected as of now (called on pair AND on disconnect, so
/// the timestamp marks the last moment control was live).
pub(crate) fn record_connection() {
    let mut p = load_prefs();
    p.connected_at = now_secs();
    let _ = crate::atomic_json::write_json_atomic(&prefs_path(), &p);
}

/// Forget the remembered connection state during an explicit pairing reset.
pub(crate) fn clear() {
    let _ = std::fs::remove_file(prefs_path());
}
