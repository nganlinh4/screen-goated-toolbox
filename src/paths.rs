//! Canonical application data directories.
//!
//! App data is split across two OS roots — `%APPDATA%/screen-goated-toolbox` for
//! config + history, and `%LOCALAPPDATA%/screen-goated-toolbox` for caches,
//! recordings and export artifacts — plus a legacy `%LOCALAPPDATA%/SGT` subfolder
//! for logs and the live WebView2 data. Those roots were derived ad hoc at ~35 call
//! sites, so changing the folder name or base path meant a wide, error-prone hunt.
//! These helpers are the single source of truth.
//!
//! NOTE: [`app_sgt_dir`] intentionally keeps the legacy `SGT` name rather than the
//! main `screen-goated-toolbox` folder, to avoid orphaning existing users' logs and
//! WebView2 session data. Consolidating it needs a one-time data-migration step and
//! is tracked separately.

use std::path::PathBuf;

/// `%APPDATA%/screen-goated-toolbox` — config and history database live here.
pub fn app_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox")
}

/// `%LOCALAPPDATA%/screen-goated-toolbox` — caches, recordings, downloaded tools,
/// assets, export artifacts.
pub fn app_local_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox")
}

/// `%LOCALAPPDATA%/SGT` — legacy folder for logs and live WebView2 data. Preserved
/// for backward compatibility; see the module note.
pub fn app_sgt_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_default().join("SGT")
}
