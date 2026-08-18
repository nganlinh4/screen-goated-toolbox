use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use anyhow::{Context as _, Result};

pub(super) const WEBVIEW_RUNTIME_ROOT_ENV: [&str; 2] = ["ProgramFiles", "ProgramFiles(x86)"];
pub(super) const PROVIDER_CREDENTIAL_ENV: [&str; 3] = [
    "GEMINI_API_KEY",
    "GROQ_API_KEY",
    "OPENROUTER_API_KEY",
];

pub(super) fn forward_provider_credentials(
    command: &mut ProcessCommand,
    mut lookup: impl FnMut(&str) -> Option<OsString>,
) {
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
            crate::overlay::get_shared_webview_data_dir(Some(super::RECORDER_WEBVIEW_PROFILE))
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
