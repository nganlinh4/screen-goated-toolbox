use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use anyhow::{Context as _, Result};

pub(super) const WEBVIEW_RUNTIME_ROOT_ENV: [&str; 2] = ["ProgramFiles", "ProgramFiles(x86)"];
pub(super) const PROVIDER_CREDENTIAL_ENV: [&str; 3] =
    ["GEMINI_API_KEY", "GROQ_API_KEY", "OPENROUTER_API_KEY"];

#[cfg(debug_assertions)]
pub(super) fn forward_debug_cursor_source(
    command: &mut ProcessCommand,
    configured: Option<OsString>,
) {
    let Some(root) = configured
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    else {
        return;
    };
    let Ok(root) = root.canonicalize() else {
        return;
    };
    if root.join("Cargo.toml").is_file()
        && root.join("src").is_dir()
        && root.join("screen-record").is_dir()
    {
        command.env("SGT_RECORDER_DEV_ASSET_ROOT", root);
    }
}

pub(super) fn forward_runtime_state_root(
    command: &mut ProcessCommand,
    lookup: impl FnOnce(&str) -> Option<OsString>,
) {
    if let Some(path) =
        crate::paths::runtime_state_root_from(lookup(crate::paths::RUNTIME_STATE_ROOT_ENV))
    {
        command.env(crate::paths::RUNTIME_STATE_ROOT_ENV, path);
    }
}

pub(super) fn forward_provider_credentials(
    command: &mut ProcessCommand,
    lookup: impl FnMut(&str) -> Option<OsString>,
) {
    forward_provider_credentials_for_runtime(
        command,
        crate::paths::runtime_state_root().is_some(),
        lookup,
    );
}

pub(super) fn forward_provider_credentials_for_runtime(
    command: &mut ProcessCommand,
    isolated: bool,
    mut lookup: impl FnMut(&str) -> Option<OsString>,
) {
    if isolated {
        return;
    }
    for name in PROVIDER_CREDENTIAL_ENV {
        if let Some(value) = lookup(name) {
            command.env(name, value);
        }
    }
}

pub(super) fn forward_webview_runtime_roots(
    command: &mut ProcessCommand,
    mut lookup: impl FnMut(&str) -> Option<OsString>,
) {
    for name in WEBVIEW_RUNTIME_ROOT_ENV {
        let Some(path) = lookup(name).map(PathBuf::from) else {
            continue;
        };
        if path.is_absolute() && path.is_dir() {
            command.env(name, path);
        }
    }
}

pub(super) fn recorder_webview_data_dir(configured: Option<OsString>) -> Result<PathBuf> {
    let selected = configured
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| {
            crate::overlay::webview_runtime::data_dir(
                crate::overlay::webview_runtime::Profile::Recorder,
            )
        });
    std::fs::create_dir_all(&selected)
        .with_context(|| format!("create recorder WebView profile '{}'", selected.display()))?;
    Ok(selected)
}

pub(super) fn recorder_debug_port(configured: Option<OsString>) -> Option<String> {
    let raw = configured?.into_string().ok()?;
    let port = raw.parse::<u16>().ok()?;
    (port > 0).then_some(port.to_string())
}
