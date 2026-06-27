//! Persisted offer/connection state for deep browser control — split out of
//! `browser/mod.rs` for the file-size limit. Decides when we may proactively offer
//! setup (post-decline snooze) and whether a current disconnect is just the installed
//! extension reconnecting (nap / restart) vs. genuinely gone.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Tracks whether/when we may proactively offer to set up browser control (so a user
/// who declined isn't nagged - but is asked again much later) and the last live
/// connection (to tell a transient nap from a real removal).
#[derive(Serialize, Deserialize, Default)]
struct OfferPrefs {
    declined: u32,
    snooze_until: u64, // unix seconds; offers are suppressed until this time
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
pub(crate) fn offer_due() -> bool {
    now_secs() >= load_prefs().snooze_until
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

/// The user declined the offer → back off, longer each time, but never forever
/// (3 days, then 2 weeks, then ~2 months — after which we ask again once).
pub(crate) fn record_decline() {
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

/// Forget all offer/connection state (used by `reset()` to re-enable setup offers).
pub(crate) fn clear() {
    let _ = std::fs::remove_file(prefs_path());
}
