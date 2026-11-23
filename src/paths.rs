//! Canonical application data directories.
//!
//! App data is split across OS roots — `%APPDATA%/screen-goated-toolbox` for config +
//! history ([`app_config_dir`]); `%LOCALAPPDATA%/screen-goated-toolbox`
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

pub(crate) const RUNTIME_STATE_ROOT_ENV: &str = "SGT_RUNTIME_STATE_ROOT";

pub(crate) fn runtime_state_root() -> Option<PathBuf> {
    runtime_state_root_from(std::env::var_os(RUNTIME_STATE_ROOT_ENV))
}

pub(crate) fn runtime_state_root_from(raw: Option<std::ffi::OsString>) -> Option<PathBuf> {
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
/// root. Individual stores choose whether any read-only fallback is appropriate.
pub fn app_runtime_config_dir() -> PathBuf {
    if let Some(root) = runtime_state_root() {
        return root.join("roaming-state").join("screen-goated-toolbox");
    }
    app_config_dir()
}

/// `%LOCALAPPDATA%/screen-goated-toolbox` — caches, recordings, downloaded tools,
/// assets, export artifacts.
pub fn app_local_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join("screen-goated-toolbox")
}

/// The user's Downloads directory. Creation results are user-owned, so callers
/// must surface an unavailable OS folder instead of falling back to app data.
#[cfg(not(feature = "recorder-worker"))]
pub fn user_downloads_dir() -> Result<PathBuf, String> {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .ok_or_else(|| "The system Downloads folder is unavailable.".to_string())
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

#[cfg(not(feature = "recorder-worker"))]
/// Temporary directory passed to child processes. Isolated runs get their own
/// directory; normal launches keep the operating system's existing TEMP root.
pub fn child_process_temp_dir() -> std::io::Result<PathBuf> {
    if runtime_state_root().is_none() {
        return Ok(std::env::temp_dir());
    }
    let path = app_temp_dir();
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) trait ChildEnvironment {
    fn set_environment(&mut self, name: &std::ffi::OsStr, value: &std::ffi::OsStr);
    fn remove_environment(&mut self, name: &std::ffi::OsStr);
}

#[cfg(not(feature = "recorder-worker"))]
impl ChildEnvironment for std::process::Command {
    fn set_environment(&mut self, name: &std::ffi::OsStr, value: &std::ffi::OsStr) {
        self.env(name, value);
    }
    fn remove_environment(&mut self, name: &std::ffi::OsStr) {
        self.env_remove(name);
    }
}

#[cfg(not(feature = "recorder-worker"))]
impl ChildEnvironment for crate::process_job::HelperCommand {
    fn set_environment(&mut self, name: &std::ffi::OsStr, value: &std::ffi::OsStr) {
        self.env(name, value);
    }
    fn remove_environment(&mut self, name: &std::ffi::OsStr) {
        self.env_remove(name);
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn configure_isolated_child_environment(
    command: &mut impl ChildEnvironment,
) -> std::io::Result<()> {
    if runtime_state_root().is_some() {
        let path = child_process_temp_dir()?;
        command.set_environment(std::ffi::OsStr::new("TEMP"), path.as_os_str());
        command.set_environment(std::ffi::OsStr::new("TMP"), path.as_os_str());
        for (name, _) in std::env::vars_os() {
            if is_credential_environment_name(&name) {
                command.remove_environment(&name);
            }
        }
    }
    Ok(())
}

fn is_credential_environment_name(name: &std::ffi::OsStr) -> bool {
    let value = name.to_string_lossy().to_ascii_uppercase();
    let parts: Vec<_> = value.split('_').collect();
    value.ends_with("KEY")
        || value.ends_with("TOKEN")
        || value.contains("SECRET")
        || value.contains("PASSWORD")
        || parts.iter().any(|part| {
            matches!(
                *part,
                "TOKEN"
                    | "SECRET"
                    | "PASSWORD"
                    | "CREDENTIAL"
                    | "CREDENTIALS"
                    | "KEY"
                    | "AUTH"
                    | "BEARER"
            )
        })
        || parts
            .windows(2)
            .any(|pair| pair == ["API", "KEY"] || pair == ["ACCESS", "KEY"])
}

/// Called at process entry, before app threads and children are started.
pub(crate) fn isolate_inherited_credentials(preserved_names: &[&str]) {
    if runtime_state_root().is_some() {
        for (name, _) in std::env::vars_os() {
            if is_credential_environment_name(&name)
                && !preserved_names
                    .iter()
                    .any(|allowed| name.eq_ignore_ascii_case(allowed))
            {
                // On Windows environment mutation is thread-safe. Run this at
                // entry as well so no application code can observe inherited keys.
                unsafe { std::env::remove_var(name) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RUNTIME_STATE_ROOT_ENV, is_credential_environment_name, runtime_state_root_from};
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

    #[test]
    fn credential_names_cover_custom_providers_without_removing_system_paths() {
        for name in [
            "CUSTOM_PROVIDER_KEY",
            "PLUGIN_AUTH_TOKEN",
            "PLUGINSECRET",
            "EXAMPLE_PASSWORD",
            "CUSTOM_API_KEY_7",
        ] {
            assert!(is_credential_environment_name(name.as_ref()), "{name}");
        }
        for name in ["PATH", "SystemRoot", "TEMP", "SGT_RUNTIME_STATE_ROOT"] {
            assert!(!is_credential_environment_name(name.as_ref()), "{name}");
        }
    }

    #[test]
    fn isolation_is_the_first_application_action_in_both_entry_points() {
        for source in [
            include_str!("main.rs"),
            include_str!("../native/recorder_worker/src/main.rs"),
        ] {
            let main = source.split_once("fn main()").unwrap().1;
            let body = main.split_once('{').unwrap().1.trim_start();
            assert!(body.starts_with("paths::isolate_inherited_credentials("));
        }
    }
}
