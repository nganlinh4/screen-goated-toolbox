use std::fs;
use std::io::{Read, Write};
use std::io::{Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

#[cfg(windows)]
mod authenticode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityStatus {
    Supported,
    MissingDependency,
}

#[derive(Clone, Debug)]
pub struct FeatureCapability {
    pub status: CapabilityStatus,
    pub title: String,
    pub details: String,
}

impl FeatureCapability {
    pub fn supported() -> Self {
        Self {
            status: CapabilityStatus::Supported,
            title: String::new(),
            details: String::new(),
        }
    }

    pub fn is_supported(&self) -> bool {
        self.status == CapabilityStatus::Supported
    }
}

static WEBVIEW2_INSTALLING: AtomicBool = AtomicBool::new(false);
static WEBVIEW2_DOWNLOAD_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn require_webview2(feature_name: &str) -> FeatureCapability {
    if webview2_runtime_installed() {
        FeatureCapability::supported()
    } else {
        let badge = crate::overlay::auto_copy_badge::locale_text();
        let feature_name = match feature_name {
            "Window selector" => badge.feature_window_selector,
            "Realtime overlay" => badge.feature_realtime_overlay,
            "Preset wheel" => badge.feature_preset_wheel,
            "TTS Playground" => badge.feature_tts_playground,
            "Text input overlay" => badge.feature_text_input,
            "Screen record" => badge.feature_screen_record,
            "Markdown view" => badge.feature_markdown_view,
            name => name,
        };
        FeatureCapability {
            status: CapabilityStatus::MissingDependency,
            title: crate::overlay::auto_copy_badge::format_locale(
                badge.feature_needs_webview2_fmt,
                &[("name", feature_name)],
            ),
            details: badge.install_webview2_hint.to_string(),
        }
    }
}

pub fn notify_capability_issue(capability: &FeatureCapability) {
    if capability.is_supported() {
        return;
    }
    let notification_type = match capability.status {
        CapabilityStatus::MissingDependency => {
            crate::overlay::auto_copy_badge::NotificationType::Info
        }
        CapabilityStatus::Supported => crate::overlay::auto_copy_badge::NotificationType::Success,
    };
    crate::overlay::auto_copy_badge::show_detailed_notification(
        &capability.title,
        &capability.details,
        notification_type,
    );
}

pub fn webview2_runtime_installed() -> bool {
    find_webview2_executable().is_some()
}

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

fn find_webview2_executable() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(value) = std::env::var("ProgramFiles") {
        roots.push(PathBuf::from(value));
    }
    if let Ok(value) = std::env::var("ProgramFiles(x86)") {
        roots.push(PathBuf::from(value));
    }
    if let Ok(value) = std::env::var("LocalAppData") {
        roots.push(PathBuf::from(value));
    }

    for root in roots {
        let app_root = root
            .join("Microsoft")
            .join("EdgeWebView")
            .join("Application");
        if let Some(found) = find_webview2_under(&app_root) {
            return Some(found);
        }
    }

    None
}

fn find_webview2_under(path: &Path) -> Option<PathBuf> {
    let direct = path.join("msedgewebview2.exe");
    if direct.exists() {
        return Some(direct);
    }
    let entries = fs::read_dir(path).ok()?;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let candidate = entry_path.join("msedgewebview2.exe");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn install_webview2_runtime() -> Result<()> {
    let _mutation = crate::component_registry::acquire_mutation_guard()?;
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
            std::process::Command::new(path)
                .args(["/silent", "/install"])
                .status()
                .map_err(|error| anyhow!("failed to launch WebView2 installer: {error}"))
        },
    )?;
    drop(cleanup);

    if !status.success() && !webview2_runtime_installed() {
        bail!("WebView2 installer exited with status {status}");
    }

    progress_badge.finish();
    crate::overlay::auto_copy_badge::show_detailed_notification(
        badge.webview2_ready,
        badge.webview2_installed_restarting,
        crate::overlay::auto_copy_badge::NotificationType::Success,
    );

    // Auto-restart the app so the new WebView2 runtime is loaded fresh and
    // every overlay that fell back to native menus (tray popup, etc.) picks
    // up the full web UI on the next launch.
    if let Ok(exe) = std::env::current_exe() {
        // Give the notification a brief moment to render before replacing
        // the process.
        std::thread::sleep(std::time::Duration::from_millis(900));
        let _ = std::process::Command::new(&exe)
            .args(std::env::args().skip(1))
            .spawn();
        std::process::exit(0);
    }
    Ok(())
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
    let mut locked = open_locked_regular_file(path)?;
    verify_locked_identity(&mut locked, expected_size, expected_sha256)?;
    verify_signature(path)?;
    let result = launch(path);
    drop(locked);
    result
}

fn download_exact(
    reader: &mut impl Read,
    output: &mut impl Write,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<()> {
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or_else(|| anyhow!("WebView2 bootstrapper is too large"))?;
        if downloaded > expected_size {
            bail!("WebView2 bootstrapper exceeds its exact size");
        }
        hasher.update(&buffer[..read]);
        output.write_all(&buffer[..read])?;
    }
    if downloaded != expected_size
        || !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected_sha256)
    {
        bail!("WebView2 bootstrapper identity does not match this build");
    }
    Ok(())
}

fn ensure_regular_directory(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("WebView2 download directory is unsafe");
    }
    Ok(())
}

fn open_locked_regular_file(path: &Path) -> Result<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("WebView2 bootstrapper path is unsafe");
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
        options
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("WebView2 bootstrapper path is unsafe");
    }
    Ok(file)
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[cfg(test)]
mod delivery_tests {
    use super::*;

    fn temporary_bootstrapper(label: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "sgt-webview-{label}-{}-{}.exe",
            std::process::id(),
            WEBVIEW2_DOWNLOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn bounded_bootstrapper_download_rejects_size_and_hash_mismatch() {
        let mut output = Vec::new();
        assert!(download_exact(&mut &b"bytes"[..], &mut output, 4, "00").is_err());
        output.clear();
        assert!(download_exact(&mut &b"data"[..], &mut output, 4, &"0".repeat(64)).is_err());
    }

    #[test]
    fn owned_bootstrapper_is_removed_when_locked_open_fails() {
        let path = temporary_bootstrapper("open-cleanup", b"bootstrapper");
        fs::remove_file(&path).unwrap();
        {
            let _cleanup = OwnedBootstrapperDownload::new(path.clone());
            assert!(
                with_locked_bootstrapper(&path, 12, &"0".repeat(64), |_| Ok(()), |_| Ok(()))
                    .is_err()
            );
        }
        assert!(!path.exists());
    }

    #[test]
    fn owned_bootstrapper_is_removed_when_signature_fails() {
        let bytes = b"bootstrapper";
        let path = temporary_bootstrapper("signature-cleanup", bytes);
        let digest = format!("{:x}", Sha256::digest(bytes));
        {
            let _cleanup = OwnedBootstrapperDownload::new(path.clone());
            let result: Result<()> = with_locked_bootstrapper(
                &path,
                bytes.len() as u64,
                &digest,
                |_| bail!("simulated signature failure"),
                |_| -> Result<()> { panic!("launch must not run after signature failure") },
            );
            assert!(result.is_err());
        }
        assert!(!path.exists());
    }

    #[test]
    fn owned_bootstrapper_is_removed_when_launch_fails() {
        let bytes = b"bootstrapper";
        let path = temporary_bootstrapper("launch-cleanup", bytes);
        let digest = format!("{:x}", Sha256::digest(bytes));
        {
            let _cleanup = OwnedBootstrapperDownload::new(path.clone());
            let result: Result<()> = with_locked_bootstrapper(
                &path,
                bytes.len() as u64,
                &digest,
                |_| Ok(()),
                |_| bail!("simulated launch failure"),
            );
            assert!(result.is_err());
        }
        assert!(!path.exists());
    }

    #[cfg(windows)]
    #[test]
    fn bootstrapper_lock_precedes_signature_and_survives_launch() {
        let bytes = b"bootstrapper";
        let path = temporary_bootstrapper("lock-lifetime", bytes);
        let digest = format!("{:x}", Sha256::digest(bytes));
        let assert_write_locked = |candidate: &Path| {
            assert!(fs::OpenOptions::new().write(true).open(candidate).is_err());
            Ok(())
        };
        with_locked_bootstrapper(
            &path,
            bytes.len() as u64,
            &digest,
            assert_write_locked,
            assert_write_locked,
        )
        .unwrap();
        fs::remove_file(path).unwrap();
    }
}
