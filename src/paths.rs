//! Canonical application data directories.
//!
//! App data is split across OS roots — `%APPDATA%/screen-goated-toolbox` for config +
//! history ([`app_config_dir`]) and for Roaming model/bin data ([`app_data_dir`] /
//! [`app_models_dir`], same folder on Windows); `%LOCALAPPDATA%/screen-goated-toolbox`
//! for caches, recordings and export artifacts ([`app_local_data_dir`]); a legacy
//! `%LOCALAPPDATA%/SGT` subfolder for logs and the live WebView2 data ([`app_sgt_dir`]);
//! and `<temp>/screen-goated-toolbox` for short-lived sidecars ([`app_temp_dir`]).
//! Those roots were derived ad hoc at ~35 call sites, so changing the folder name or
//! base path meant a wide, error-prone hunt. These helpers are the single source of
//! truth.
//!
//! NOTE: a handful of `Option`-returning call sites (export/cache paths that propagate
//! a `None` when the OS dir can't be resolved, via `?` / `.map` / `.ok_or_else`) still
//! derive `%LOCALAPPDATA%/screen-goated-toolbox` inline rather than through
//! [`app_local_data_dir`], because the helper collapses that `None` to a default. Their
//! control flow — not the folder name — is what differs, so they are left as-is.
//!
//! NOTE: [`app_sgt_dir`] intentionally keeps the legacy `SGT` name rather than the
//! main `screen-goated-toolbox` folder, to avoid orphaning existing users' logs and
//! WebView2 session data. Consolidating it needs a one-time data-migration step and
//! is tracked separately.

use std::path::PathBuf;

const RUNTIME_STATE_ROOT_ENV: &str = "SGT_RUNTIME_STATE_ROOT";

fn runtime_state_root() -> Option<PathBuf> {
    runtime_state_root_from(std::env::var_os(RUNTIME_STATE_ROOT_ENV))
}

fn runtime_state_root_from(raw: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let raw = raw?;
    if raw.is_empty() {
        return None;
    }
    let path = PathBuf::from(raw);
    assert!(
        path.is_absolute(),
        "{RUNTIME_STATE_ROOT_ENV} must be an absolute path"
    );
    Some(path)
}

/// `%APPDATA%/screen-goated-toolbox` — config and history database live here.
pub fn app_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox")
}

/// Mutable config-shaped state for an isolated runtime. Normal launches share
/// [`app_config_dir`]; a run with `SGT_RUNTIME_STATE_ROOT` writes under that
/// root while remaining free to read the user's existing configuration.
pub fn app_runtime_config_dir() -> PathBuf {
    if let Some(root) = runtime_state_root() {
        return root.join("roaming-state").join("screen-goated-toolbox");
    }
    app_config_dir()
}

/// `%APPDATA%/screen-goated-toolbox` — Roaming app-data root for downloaded models
/// and sidecar binaries. On Windows this resolves to the same folder as
/// [`app_config_dir`]; it is kept distinct because these call sites semantically use
/// `dirs::data_dir()` (Roaming *data*), not `dirs::config_dir()`.
pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox")
}

/// `%APPDATA%/screen-goated-toolbox/models` — root for all downloaded model packs
/// (Kokoro, Parakeet, Qwen3, Sherpa/Zipformer, Supertonic, Voxtral, …).
pub fn app_models_dir() -> PathBuf {
    app_data_dir().join("models")
}

/// `%LOCALAPPDATA%/screen-goated-toolbox` — caches, recordings, downloaded tools,
/// assets, export artifacts.
pub fn app_local_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox")
}

/// Writable local state for bootstrap/runtime support files. Installed models
/// and runtimes continue to be read through [`app_local_data_dir`].
pub fn app_runtime_local_data_dir() -> PathBuf {
    if let Some(root) = runtime_state_root() {
        return root.join("local-app-data").join("screen-goated-toolbox");
    }
    app_local_data_dir()
}

/// `%LOCALAPPDATA%/SGT` — legacy folder for logs and live WebView2 data. Preserved
/// for backward compatibility; see the module note.
pub fn app_sgt_dir() -> PathBuf {
    if let Some(root) = runtime_state_root() {
        return root.join("local-app-data").join("SGT");
    }
    dirs::data_local_dir().unwrap_or_default().join("SGT")
}

/// `<temp>/screen-goated-toolbox` — scratch root for short-lived sidecar artifacts
/// (TTS wavs, subtitle media renders) under the OS temp directory.
pub fn app_temp_dir() -> PathBuf {
    if let Some(root) = runtime_state_root() {
        return root.join("temp").join("screen-goated-toolbox");
    }
    std::env::temp_dir().join("screen-goated-toolbox")
}

#[cfg(test)]
mod tests {
    use super::{RUNTIME_STATE_ROOT_ENV, runtime_state_root_from};
    use std::ffi::OsString;

    #[test]
    fn runtime_state_override_is_optional() {
        assert!(runtime_state_root_from(None).is_none());
        assert!(runtime_state_root_from(Some(OsString::new())).is_none());
        assert_eq!(
            runtime_state_root_from(Some(OsString::from(r"C:\isolated-run"))),
            Some(r"C:\isolated-run".into())
        );
    }

    #[test]
    #[should_panic(expected = "SGT_RUNTIME_STATE_ROOT must be an absolute path")]
    fn runtime_state_override_rejects_relative_paths() {
        let _ = runtime_state_root_from(Some(OsString::from("relative")));
    }

    #[test]
    fn runtime_state_override_name_is_stable() {
        assert_eq!(RUNTIME_STATE_ROOT_ENV, "SGT_RUNTIME_STATE_ROOT");
    }
}
