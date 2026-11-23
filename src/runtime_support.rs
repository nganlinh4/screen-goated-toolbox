use std::fs;
use std::io::{Read, Write};
use std::io::{Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use anyhow::{Context, Result, anyhow, bail};

#[cfg(windows)]
mod authenticode;
#[cfg(windows)]
mod restart;
mod restart_retry;
mod webview_capability;
mod webview_readiness;

pub use webview_capability::{
    WebViewFeature, notify_capability_issue, require_webview2, webview2_runtime_installed,
};

#[cfg(windows)]
pub(crate) use restart::await_previous_process;

#[cfg(not(windows))]
pub(crate) fn await_previous_process() -> Result<()> {
    Ok(())
}

#[cfg(not(windows))]
mod restart {
    pub(super) const PARENT_MARKER: &str = "SGT_WEBVIEW2_RESTART_PARENT";

    pub(super) fn current_process_marker() -> std::io::Result<String> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "WebView2 restart is supported only on Windows",
        ))
    }
}

static WEBVIEW2_INSTALLING: AtomicBool = AtomicBool::new(false);
const WEBVIEW2_TRANSFER_ID: &str = "webview2-runtime";
static WEBVIEW2_DOWNLOAD_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn start_webview2_runtime_install() -> bool {
    if webview2_runtime_installed() {
        return false;
    }
    if WEBVIEW2_INSTALLING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    std::thread::spawn(|| {
        if let Err(error) = install_webview2_runtime() {
            crate::log_info!("[WebView2] Install failed: {error:#}");
            crate::overlay::auto_copy_badge::show_error_notification(
                crate::overlay::auto_copy_badge::locale_text().webview2_install_failed,
            );
        }
        WEBVIEW2_INSTALLING.store(false, Ordering::Release);
    });
    true
}

fn install_webview2_runtime() -> Result<()> {
    // The bootstrapper is not a registry component. Its download and the
    // system install run under their own cross-process guard, so a slow
    // install never blocks the component registry.
    let never_cancelled = AtomicBool::new(false);
    let _transfer = crate::component_registry::acquire_transfer_guard_cancellable(
        WEBVIEW2_TRANSFER_ID,
        &never_cancelled,
    )?;
    let delivery = crate::component_registry::external_tools::webview2_bootstrapper_delivery()?;
    crate::log_info!(
        "[WebView2] installing pinned bootstrapper {} ({})",
        delivery.version,
        delivery.asset
    );
    let badge = crate::overlay::auto_copy_badge::locale_text();
    let progress_badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::with_text(
        badge.installing_webview2,
        badge.downloading_webview2_installer,
    );
    progress_badge.report(5, 100);

    let scratch = crate::paths::app_runtime_local_data_dir().join("component-downloads");
    ensure_regular_directory(&scratch)?;
    let sequence = WEBVIEW2_DOWNLOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let installer_path = scratch.join(format!(
        "webview2-bootstrapper-{}-{}-{sequence}.exe",
        delivery.version,
        std::process::id()
    ));
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(delivery.download_url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|error| anyhow!("failed to download WebView2 bootstrapper: {error}"))?;
    if response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size != delivery.size_bytes)
    {
        bail!("WebView2 bootstrapper size does not match this build");
    }
    let mut reader = response.into_body().into_reader();
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&installer_path)
        .with_context(|| {
            format!(
                "create WebView2 bootstrapper '{}'",
                installer_path.display()
            )
        })?;
    let cleanup = OwnedBootstrapperDownload::new(installer_path.clone());
    let download = download_exact(&mut reader, &mut file, delivery.size_bytes, delivery.sha256);
    if let Err(error) = download {
        drop(file);
        return Err(error);
    }
    file.flush()?;
    file.sync_all()?;
    drop(file);

    progress_badge.report(55, 100);

    let status = with_locked_bootstrapper(
        &installer_path,
        delivery.size_bytes,
        delivery.sha256,
        |path| {
            #[cfg(windows)]
            {
                authenticode::verify_publisher(path, delivery.expected_publisher)
            }
            #[cfg(not(windows))]
            {
                let _ = path;
                bail!("WebView2 bootstrapper is supported only on Windows");
            }
        },
        |path| {
            crate::system_command::background_command(path)
                .args(["/silent", "/install"])
                .status()
                .map_err(|error| anyhow!("failed to launch WebView2 installer: {error}"))
        },
    )?;
    drop(cleanup);

    if let Err(error) =
        webview_readiness::verify(status.success(), webview2_runtime_installed, || {
            std::thread::sleep(std::time::Duration::from_millis(250))
        })
    {
        match error {
            webview_readiness::Failure::InstallerFailed => {
                bail!(
                    "WebView2 installer exited with status {status} and the runtime was not detected"
                );
            }
            webview_readiness::Failure::NotDetected => {
                bail!("WebView2 installer reported success but the runtime was not detected");
            }
        }
    }

    progress_badge.finish();
    // Exit preparation can wait for other component owners. Never hold the
    // installation guard while asking them to shut down.
    drop(_transfer);
    // Auto-restart only after the same fallible preparation used by normal
    // quit/update. A pending save or failed launch leaves this process alive.
    if let Err(error) = guarded_restart(
        || {
            restart_retry::prepare_with_bounded_retry(
                crate::gui::app::prepare_to_exit,
                crate::gui::app::has_pending_exit_request,
                100,
                || std::thread::sleep(std::time::Duration::from_millis(100)),
            )
            .ok()
        },
        || {
            let exe = std::env::current_exe()?;
            let marker = restart::current_process_marker()?;
            crate::gui::app::reserve_automatic_restart(|| {
                webview2_restart_command(&exe, &marker).spawn().map(|_| ())
            })
        },
        || {
            crate::overlay::auto_copy_badge::show_detailed_notification(
                badge.webview2_ready,
                badge.webview2_installed_restarting,
                crate::overlay::auto_copy_badge::NotificationType::Success,
            );
            std::thread::sleep(std::time::Duration::from_millis(900));
        },
        || {
            crate::overlay::auto_copy_badge::show_detailed_notification(
                badge.webview2_ready,
                badge.webview2_installed_restart_manually,
                crate::overlay::auto_copy_badge::NotificationType::Info,
            );
        },
        |prepared| crate::gui::app::exit_prepared(prepared),
    ) {
        crate::log_info!("[WebView2] Installed; automatic restart failed: {error}");
    }
    Ok(())
}

fn webview2_restart_command(executable: &Path, parent_marker: &str) -> std::process::Command {
    restart_command_with_parent(executable, std::iter::empty::<&str>(), parent_marker)
}

/// Replacement startup waits for this exact process before acquiring its singleton.
pub(crate) fn restart_command(
    executable: &Path,
    arguments: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> std::io::Result<std::process::Command> {
    let marker = restart::current_process_marker()?;
    Ok(restart_command_with_parent(executable, arguments, &marker))
}

fn restart_command_with_parent(
    executable: &Path,
    arguments: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    marker: &str,
) -> std::process::Command {
    let mut command = crate::system_command::background_command(executable);
    command.args(arguments).env(restart::PARENT_MARKER, marker);
    command
}

fn guarded_restart<T, R>(
    prepare: impl FnOnce() -> Option<T>,
    spawn: impl FnOnce() -> std::io::Result<R>,
    on_spawned: impl FnOnce(),
    on_not_restarted: impl FnOnce(),
    exit: impl FnOnce(T),
) -> std::io::Result<bool> {
    let Some(prepared) = prepare() else {
        on_not_restarted();
        return Ok(false);
    };
    let _reservation = match spawn() {
        Ok(reservation) => reservation,
        Err(error) => {
            on_not_restarted();
            return Err(error);
        }
    };
    on_spawned();
    exit(prepared);
    Ok(true)
}

struct OwnedBootstrapperDownload {
    path: PathBuf,
}

impl OwnedBootstrapperDownload {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for OwnedBootstrapperDownload {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn verify_locked_identity(
    file: &mut fs::File,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut sink = std::io::sink();
    download_exact(file, &mut sink, expected_size, expected_sha256)
}

fn with_locked_bootstrapper<T>(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
    verify_signature: impl FnOnce(&Path) -> Result<()>,
    launch: impl FnOnce(&Path) -> Result<T>,
) -> Result<T> {
    let mut locked = crate::component_registry::open_regular(path)
        .context("WebView2 bootstrapper path is unsafe")?;
    verify_locked_identity(&mut locked, expected_size, expected_sha256)?;
    verify_signature(path)?;
    let result = launch(path);
    drop(locked);
    result
}

/// Copies the bootstrapper while checking it against this build's pin.
fn download_exact(
    reader: &mut impl Read,
    output: &mut impl Write,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<()> {
    let matched = crate::component_registry::stream_matches(
        reader,
        output,
        expected_size,
        expected_sha256,
        &|| false,
        &|_| {},
    )
    .context("WebView2 bootstrapper exceeds its exact size")?;
    if !matched {
        bail!("WebView2 bootstrapper identity does not match this build");
    }
    Ok(())
}

fn ensure_regular_directory(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || crate::component_registry::is_reparse_point(&metadata) {
        bail!("WebView2 download directory is unsafe");
    }
    Ok(())
}

#[cfg(test)]
mod delivery_tests;
