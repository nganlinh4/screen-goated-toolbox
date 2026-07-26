//! Managed native sidecar shared by creation mini apps.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const RUNTIME_ASSET: &str = "sgt-creation-runtime-windows-x64.exe";
const RUNTIME_URL: &str = "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/sgt-creation-runtime-windows-x64.exe";
const RUNTIME_BYTES: u64 = 1_218_048;
const RUNTIME_SHA256: &str = "b69bf3c68a5a2e4abab26a9b8eddafbca675ed52d9a0662fa1da751a96a58636";
type RuntimeValidationCache = (PathBuf, u64, u128, bool);

pub(crate) const DOWNLOAD_TITLE: &str = "Downloading creation engine";

pub(crate) fn runtime_bundle_dir() -> PathBuf {
    crate::paths::app_local_data_dir()
        .join("3d-generator-runtime")
        .join("bin")
}

pub(crate) fn runtime_exe_path() -> PathBuf {
    runtime_bundle_dir().join("sgt_creation_runtime.exe")
}

fn validate_runtime(path: &Path) -> Result<()> {
    let metadata =
        std::fs::metadata(path).map_err(|error| anyhow!("Creation engine unavailable: {error}"))?;
    if !metadata.is_file() || metadata.len() != RUNTIME_BYTES {
        bail!(
            "Creation engine size {} does not match expected {RUNTIME_BYTES}",
            metadata.len()
        );
    }

    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
        .unwrap_or(0);
    static CACHE: OnceLock<Mutex<Option<RuntimeValidationCache>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Some((cached_path, bytes, modified, valid)) = cache
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .as_ref()
        && cached_path == path
        && *bytes == metadata.len()
        && *modified == modified_ms
    {
        return if *valid {
            Ok(())
        } else {
            bail!("Creation engine checksum mismatch")
        };
    }

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let valid = format!("{:x}", hasher.finalize()) == RUNTIME_SHA256;
    *cache.lock().unwrap_or_else(|value| value.into_inner()) =
        Some((path.to_path_buf(), metadata.len(), modified_ms, valid));
    if !valid {
        bail!("Creation engine checksum mismatch");
    }
    Ok(())
}

pub(crate) fn is_runtime_installed() -> bool {
    validate_runtime(&runtime_exe_path()).is_ok()
}

pub(crate) fn remove_runtime() -> Result<()> {
    let dir = runtime_bundle_dir();
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

pub(crate) fn update_installed_runtime_in_background() {
    let path = runtime_exe_path();
    if !path.is_file() || is_runtime_installed() {
        return;
    }

    std::thread::spawn(|| {
        let stop = Arc::new(AtomicBool::new(false));
        if let Err(error) = download_runtime(stop, true) {
            crate::log_info!("[Creation runtime] Background update failed: {error}");
        }
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SharedMaintenanceResult {
    Ready,
    Refilled,
    Deferred,
}

#[cfg(debug_assertions)]
fn newest_development_runtime_candidate(
    candidates: impl IntoIterator<Item = (PathBuf, std::time::SystemTime)>,
) -> Option<PathBuf> {
    candidates
        .into_iter()
        .max_by_key(|(_, modified)| *modified)
        .map(|(path, _)| path)
}

#[cfg(debug_assertions)]
pub(crate) fn development_runtime_path() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("native")
        .join("sgt_3d_generator_runtime")
        .join("target");
    newest_development_runtime_candidate(["debug", "release"].into_iter().filter_map(|profile| {
        let path = root.join(profile).join("sgt_creation_runtime.exe");
        let modified = path.metadata().ok()?.modified().ok()?;
        Some((path, modified))
    }))
}

#[cfg(not(debug_assertions))]
pub(crate) fn development_runtime_path() -> Option<PathBuf> {
    None
}

pub(crate) fn shared_runtime_path() -> Option<PathBuf> {
    development_runtime_path().or_else(|| is_runtime_installed().then(runtime_exe_path))
}

pub(crate) struct RuntimePreparationStatus {
    pub(crate) state: String,
    pub(crate) needs_preparation: bool,
}

pub(crate) fn query_preparation_status(tool: &str) -> Option<RuntimePreparationStatus> {
    if !matches!(tool, "3d" | "svg" | "image") {
        return None;
    }
    let mut command = Command::new(shared_runtime_path()?);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_command_window(&mut command);
    let mut child = command.spawn().ok()?;
    let request = json!({
        "id": "preparation-status",
        "cmd": "preparation_status",
        "args": { "tool": tool },
    });
    writeln!(child.stdin.take()?, "{request}").ok()?;
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
        .and_then(|value| {
            let result = value.get("result")?;
            Some(RuntimePreparationStatus {
                state: result.get("state")?.as_str()?.to_string(),
                needs_preparation: result.get("needsPreparation")?.as_bool()?,
            })
        })
}

fn parse_shared_maintenance(output: &[u8]) -> SharedMaintenanceResult {
    String::from_utf8_lossy(output)
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
        .and_then(|value| {
            let result = value.get("result")?;
            if result.get("ready").and_then(Value::as_bool) == Some(true) {
                Some(SharedMaintenanceResult::Ready)
            } else if result.get("refilled").and_then(Value::as_bool) == Some(true) {
                Some(SharedMaintenanceResult::Refilled)
            } else {
                Some(SharedMaintenanceResult::Deferred)
            }
        })
        .unwrap_or(SharedMaintenanceResult::Deferred)
}

fn run_shared_maintenance(path: &Path) -> SharedMaintenanceResult {
    let mut command = Command::new(path);
    command
        .arg("--maintain-shared-preparation-headless")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped());
    hide_command_window(&mut command);
    command
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| parse_shared_maintenance(&output.stdout))
        .unwrap_or(SharedMaintenanceResult::Deferred)
}

pub(crate) fn start_shared_preparation_maintainer() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    std::thread::spawn(|| {
        loop {
            let (result, runtime_available) = shared_runtime_path()
                .map(|path| (run_shared_maintenance(&path), true))
                .unwrap_or((SharedMaintenanceResult::Deferred, false));
            let delay = match (result, runtime_available) {
                (SharedMaintenanceResult::Ready, _) => Duration::from_secs(15 * 60),
                (SharedMaintenanceResult::Refilled, _) => Duration::from_secs(15),
                (SharedMaintenanceResult::Deferred, true) => Duration::from_secs(15 * 60),
                (SharedMaintenanceResult::Deferred, false) => Duration::from_secs(60),
            };
            std::thread::sleep(delay);
        }
    });
}

#[cfg(windows)]
fn hide_command_window(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_command_window(_command: &mut Command) {}

pub(crate) fn download_runtime(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    use crate::overlay::auto_copy_badge::{
        NotificationType, hide_progress_notification, show_detailed_notification,
        show_error_notification, show_progress_notification,
    };
    use crate::overlay::realtime_webview::state::REALTIME_STATE;

    static DOWNLOAD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = DOWNLOAD_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if is_runtime_installed() {
        return Ok(());
    }

    let path = runtime_exe_path();
    let partial = runtime_bundle_dir().join(format!("{RUNTIME_ASSET}.download"));
    std::fs::create_dir_all(runtime_bundle_dir())?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    if partial.exists() {
        std::fs::remove_file(&partial)?;
    }

    let badge = crate::overlay::auto_copy_badge::locale_text();
    let title = crate::overlay::auto_copy_badge::format_locale(
        badge.downloading_runtime_fmt,
        &[("name", "Creation tools")],
    );
    let preparing = crate::overlay::auto_copy_badge::format_locale(
        badge.preparing_runtime_fmt,
        &[("name", "Creation tools")],
    );
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = DOWNLOAD_TITLE.to_string();
        state.download_message = preparing.clone();
        state.download_progress = 0.0;
    }
    if use_badge {
        show_progress_notification(&title, &preparing, 0.0);
    }

    let result = crate::api::realtime_audio::model_loader::download_file_with_progress(
        RUNTIME_URL,
        &partial,
        &stop,
        |downloaded, total| {
            let progress = if total > 0 {
                downloaded as f32 / total as f32 * 100.0
            } else {
                0.0
            };
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = title.clone();
                state.download_progress = progress;
            }
            if use_badge {
                show_progress_notification(&title, &title, progress);
            }
        },
    )
    .and_then(|()| validate_runtime(&partial))
    .and_then(|()| {
        std::fs::rename(&partial, &path)
            .map_err(|error| anyhow!("Could not install creation engine: {error}"))
    })
    .and_then(|()| validate_runtime(&path));

    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
        state.download_progress = if result.is_ok() { 100.0 } else { 0.0 };
    }
    if use_badge {
        hide_progress_notification();
        if result.is_ok() {
            let ready = crate::overlay::auto_copy_badge::format_locale(
                badge.model_ready_fmt,
                &[("name", "Creation tools")],
            );
            let installed = crate::overlay::auto_copy_badge::format_locale(
                badge.model_installed_fmt,
                &[("name", "Creation engine")],
            );
            show_detailed_notification(&ready, &installed, NotificationType::Success);
        } else {
            let failed = crate::overlay::auto_copy_badge::format_locale(
                badge.model_download_failed_fmt,
                &[("name", "Creation engine")],
            );
            show_error_notification(&failed);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(debug_assertions)]
    #[test]
    fn development_runtime_uses_the_newest_binary() {
        let older = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1);
        let newer = std::time::UNIX_EPOCH + std::time::Duration::from_secs(2);

        let selected = newest_development_runtime_candidate([
            (PathBuf::from("debug.exe"), older),
            (PathBuf::from("release.exe"), newer),
        ]);

        assert_eq!(selected, Some(PathBuf::from("release.exe")));
    }

    #[test]
    fn shared_maintenance_parser_distinguishes_ready_refill_and_failure() {
        assert_eq!(
            parse_shared_maintenance(br#"{"ok":true,"result":{"ready":true,"refilled":false}}"#,),
            SharedMaintenanceResult::Ready
        );
        assert_eq!(
            parse_shared_maintenance(
                br#"{"event":"progress"}
{"ok":true,"result":{"ready":false,"refilled":true}}"#,
            ),
            SharedMaintenanceResult::Refilled
        );
        assert_eq!(
            parse_shared_maintenance(br#"{"ok":false,"error":"unavailable"}"#),
            SharedMaintenanceResult::Deferred
        );
    }
}
