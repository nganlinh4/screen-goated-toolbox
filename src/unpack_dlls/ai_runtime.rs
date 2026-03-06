use anyhow::{Context, Result, anyhow};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const INSTALL_TITLE: &str = "Installing local AI runtime";
const ONNX_PACKAGE_URL: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime.directml/1.22.0/microsoft.ml.onnxruntime.directml.1.22.0.nupkg";
const DIRECTML_PACKAGE_URL: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ai.directml/1.15.4/microsoft.ai.directml.1.15.4.nupkg";
const ONNX_ARCHIVE_NAME: &str = "onnxruntime-directml-1.22.0.nupkg";
const DIRECTML_ARCHIVE_NAME: &str = "directml-1.15.4.nupkg";
const ONNX_DLL: &str = "onnxruntime.dll";
const ONNX_SHARED_DLL: &str = "onnxruntime_providers_shared.dll";
const DIRECTML_DLL: &str = "DirectML.dll";

#[derive(Clone, Debug)]
pub enum AiRuntimeStatus {
    Missing,
    Installing { label: String, progress: f32 },
    Installed { bytes: u64 },
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiRuntimeUi {
    None,
    RealtimeOverlay,
    Badge,
}

#[derive(Clone, Copy)]
struct RuntimePackage {
    url: &'static str,
    archive_name: &'static str,
    label: &'static str,
    progress_start: f32,
    progress_end: f32,
    entries: &'static [super::remote_zip::RequestedZipEntry],
}

const ONNX_ENTRIES: &[super::remote_zip::RequestedZipEntry] = &[
    super::remote_zip::RequestedZipEntry {
        source_path: "runtimes/win-x64/native/onnxruntime.dll",
        dest_name: ONNX_DLL,
    },
    super::remote_zip::RequestedZipEntry {
        source_path: "runtimes/win-x64/native/onnxruntime_providers_shared.dll",
        dest_name: ONNX_SHARED_DLL,
    },
];

const DIRECTML_ENTRIES: &[super::remote_zip::RequestedZipEntry] =
    &[super::remote_zip::RequestedZipEntry {
        source_path: "bin/x64-win/DirectML.dll",
        dest_name: DIRECTML_DLL,
    }];

const PACKAGES: &[RuntimePackage] = &[
    RuntimePackage {
        url: ONNX_PACKAGE_URL,
        archive_name: ONNX_ARCHIVE_NAME,
        label: "Downloading ONNX Runtime",
        progress_start: 0.0,
        progress_end: 48.0,
        entries: ONNX_ENTRIES,
    },
    RuntimePackage {
        url: DIRECTML_PACKAGE_URL,
        archive_name: DIRECTML_ARCHIVE_NAME,
        label: "Downloading DirectML",
        progress_start: 50.0,
        progress_end: 98.0,
        entries: DIRECTML_ENTRIES,
    },
];

lazy_static::lazy_static! {
    static ref INSTALL_MUTEX: Mutex<()> = Mutex::new(());
    static ref STATUS: Mutex<AiRuntimeStatus> = Mutex::new(AiRuntimeStatus::Missing);
}

fn core_runtime_present(bin_dir: &Path) -> bool {
    bin_dir.join(ONNX_DLL).exists() && bin_dir.join(DIRECTML_DLL).exists()
}

fn runtime_bytes(bin_dir: &Path) -> u64 {
    [ONNX_DLL, ONNX_SHARED_DLL, DIRECTML_DLL]
        .into_iter()
        .filter_map(|name| fs::metadata(bin_dir.join(name)).ok())
        .map(|meta| meta.len())
        .sum()
}

fn set_status(status: AiRuntimeStatus) {
    *STATUS.lock().unwrap() = status;
}

fn post_realtime_download_state(active: bool, title: &str, message: &str, progress: f32) {
    use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
    use crate::overlay::realtime_webview::state::{REALTIME_HWND, REALTIME_STATE};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = active;
        state.download_title = title.to_string();
        state.download_message = message.to_string();
        state.download_progress = progress;
    }

    unsafe {
        if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
            let _ = PostMessageW(
                Some(REALTIME_HWND),
                WM_DOWNLOAD_PROGRESS,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

fn update_progress(ui: AiRuntimeUi, label: &str, progress: f32) {
    set_status(AiRuntimeStatus::Installing {
        label: label.to_string(),
        progress,
    });

    match ui {
        AiRuntimeUi::None => {}
        AiRuntimeUi::RealtimeOverlay => {
            post_realtime_download_state(true, INSTALL_TITLE, label, progress);
        }
        AiRuntimeUi::Badge => {
            crate::overlay::auto_copy_badge::show_progress_notification(
                INSTALL_TITLE,
                label,
                progress,
            );
        }
    }
}

fn clear_progress(ui: AiRuntimeUi) {
    match ui {
        AiRuntimeUi::None => {}
        AiRuntimeUi::RealtimeOverlay => {
            post_realtime_download_state(false, "", "", 0.0);
        }
        AiRuntimeUi::Badge => {
            crate::overlay::auto_copy_badge::hide_progress_notification();
        }
    }
}

fn package_name(package: RuntimePackage) -> &'static str {
    package.label.trim_start_matches("Downloading ")
}

fn download_package(
    package: RuntimePackage,
    bin_dir: &Path,
    stop_signal: &AtomicBool,
    ui: AiRuntimeUi,
) -> Result<PathBuf> {
    let archive_path = bin_dir.join(package.archive_name);
    let temp_path = archive_path.with_extension("tmp");
    let _ = fs::remove_file(&temp_path);

    let response = ureq::get(package.url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|e| anyhow!("Download failed for {}: {}", package.label, e))?;

    let total_size = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);

    let mut reader = response.into_body().into_reader();
    let mut file = fs::File::create(&temp_path)
        .with_context(|| format!("Failed to create temp file '{}'", temp_path.display()))?;
    let mut downloaded = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    let mut last_update = Instant::now() - Duration::from_secs(1);

    loop {
        if stop_signal.load(Ordering::Relaxed) {
            let _ = fs::remove_file(&temp_path);
            return Err(anyhow!("Download cancelled"));
        }

        let bytes_read = match reader.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(err) => {
                let _ = fs::remove_file(&temp_path);
                return Err(err.into());
            }
        };
        if bytes_read == 0 {
            break;
        }

        if let Err(err) = file.write_all(&buffer[..bytes_read]) {
            let _ = fs::remove_file(&temp_path);
            return Err(err.into());
        }
        downloaded += bytes_read as u64;

        if total_size > 0 && last_update.elapsed() >= Duration::from_millis(100) {
            let local_progress = downloaded as f32 / total_size as f32;
            let progress = package.progress_start
                + (package.progress_end - package.progress_start) * local_progress;
            let detail = format!(
                "{} ({:.1} MB / {:.1} MB)",
                package.label,
                downloaded as f64 / 1024.0 / 1024.0,
                total_size as f64 / 1024.0 / 1024.0
            );
            update_progress(ui, &detail, progress);
            last_update = Instant::now();
        }
    }

    drop(file);
    if let Err(err) = fs::rename(&temp_path, &archive_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(anyhow!(
            "Failed to move downloaded runtime archive into '{}': {}",
            archive_path.display(),
            err
        ));
    }

    Ok(archive_path)
}

fn install_package_with_ranged_fetch(
    package: RuntimePackage,
    bin_dir: &Path,
    stop_signal: &AtomicBool,
    ui: AiRuntimeUi,
) -> Result<()> {
    update_progress(
        ui,
        &format!("Preparing {} x64 payload", package_name(package)),
        package.progress_start,
    );

    let label = package_name(package);
    super::remote_zip::download_entries_to_dir(
        package.url,
        package.entries,
        bin_dir,
        stop_signal,
        |downloaded, total| {
            let fraction = downloaded as f32 / total.max(1) as f32;
            let progress =
                package.progress_start + (package.progress_end - package.progress_start) * fraction;
            let detail = format!(
                "Downloading {} x64 payload ({:.1} MB / {:.1} MB)",
                label,
                downloaded as f64 / 1024.0 / 1024.0,
                total as f64 / 1024.0 / 1024.0
            );
            update_progress(ui, &detail, progress);
        },
    )?;

    update_progress(
        ui,
        &format!("Installing {} x64 payload", label),
        package.progress_end,
    );
    Ok(())
}

fn extract_package(
    package: RuntimePackage,
    archive_path: &Path,
    bin_dir: &Path,
    stop_signal: &AtomicBool,
    ui: AiRuntimeUi,
) -> Result<()> {
    let file = fs::File::open(archive_path)
        .with_context(|| format!("Failed to open archive '{}'", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("Failed to read archive '{}'", archive_path.display()))?;

    let extraction_progress = package.progress_end + 2.0;
    let extract_label = format!("Extracting {}", package_name(package));
    update_progress(ui, &extract_label, extraction_progress);

    for entry in package.entries {
        if stop_signal.load(Ordering::Relaxed) {
            return Err(anyhow!("Download cancelled"));
        }

        let mut zipped = archive.by_name(entry.source_path).with_context(|| {
            format!(
                "Missing '{}' in runtime archive '{}'",
                entry.source_path,
                archive_path.display()
            )
        })?;

        let final_path = bin_dir.join(entry.dest_name);
        let temp_path = final_path.with_extension("tmp");
        let _ = fs::remove_file(&temp_path);

        let mut out = fs::File::create(&temp_path)
            .with_context(|| format!("Failed to create '{}'", temp_path.display()))?;
        if let Err(err) = std::io::copy(&mut zipped, &mut out) {
            let _ = fs::remove_file(&temp_path);
            return Err(anyhow!("Failed to extract '{}': {}", entry.dest_name, err));
        }
        drop(out);

        if final_path.exists() {
            let _ = fs::remove_file(&final_path);
        }
        if let Err(err) = fs::rename(&temp_path, &final_path) {
            let _ = fs::remove_file(&temp_path);
            return Err(anyhow!(
                "Failed to finalize '{}': {}",
                final_path.display(),
                err
            ));
        }
    }

    let _ = fs::remove_file(archive_path);
    Ok(())
}

fn install_runtime(stop_signal: &AtomicBool, ui: AiRuntimeUi) -> Result<()> {
    let bin_dir = super::private_bin_dir();
    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("Failed to create '{}'", bin_dir.display()))?;

    for package in PACKAGES {
        if let Err(err) = install_package_with_ranged_fetch(*package, &bin_dir, stop_signal, ui) {
            crate::log_info!(
                "[AI Runtime] Ranged fetch for {} failed, falling back to full package download: {}",
                package_name(*package),
                err
            );
            let archive_path = download_package(*package, &bin_dir, stop_signal, ui)?;
            extract_package(*package, &archive_path, &bin_dir, stop_signal, ui)?;
        }
    }

    update_progress(ui, "Finalizing local AI runtime", 100.0);
    Ok(())
}

pub fn is_ai_runtime_installed() -> bool {
    core_runtime_present(&super::private_bin_dir())
}

fn current_ai_runtime_usage_bytes() -> u64 {
    runtime_bytes(&super::private_bin_dir())
}

pub fn current_ai_runtime_status() -> AiRuntimeStatus {
    let status = STATUS.lock().unwrap().clone();
    let bin_dir = super::private_bin_dir();

    match status {
        AiRuntimeStatus::Installing { .. } => status,
        _ if core_runtime_present(&bin_dir) => AiRuntimeStatus::Installed {
            bytes: runtime_bytes(&bin_dir),
        },
        AiRuntimeStatus::Error(message) => AiRuntimeStatus::Error(message),
        _ => AiRuntimeStatus::Missing,
    }
}

pub fn remove_ai_runtime() -> Result<()> {
    let _guard = INSTALL_MUTEX.lock().unwrap();
    let bin_dir = super::private_bin_dir();

    for name in [ONNX_DLL, ONNX_SHARED_DLL, DIRECTML_DLL] {
        let path = bin_dir.join(name);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove '{}'", path.display()))?;
        }
    }

    set_status(AiRuntimeStatus::Missing);
    Ok(())
}

pub fn ensure_ai_runtime_installed(stop_signal: Arc<AtomicBool>, ui: AiRuntimeUi) -> Result<()> {
    if is_ai_runtime_installed() {
        set_status(AiRuntimeStatus::Installed {
            bytes: current_ai_runtime_usage_bytes(),
        });
        return Ok(());
    }

    let _guard = INSTALL_MUTEX.lock().unwrap();
    if is_ai_runtime_installed() {
        set_status(AiRuntimeStatus::Installed {
            bytes: current_ai_runtime_usage_bytes(),
        });
        return Ok(());
    }

    let result = install_runtime(&stop_signal, ui);
    clear_progress(ui);

    match result {
        Ok(()) => {
            set_status(AiRuntimeStatus::Installed {
                bytes: current_ai_runtime_usage_bytes(),
            });
            if ui == AiRuntimeUi::Badge {
                crate::overlay::auto_copy_badge::show_detailed_notification(
                    "Local AI runtime ready",
                    "DirectML + ONNX Runtime installed",
                    crate::overlay::auto_copy_badge::NotificationType::Success,
                );
            }
            Ok(())
        }
        Err(err) => {
            if err.to_string().contains("cancelled") {
                set_status(AiRuntimeStatus::Missing);
            } else {
                set_status(AiRuntimeStatus::Error(err.to_string()));
                if ui != AiRuntimeUi::None {
                    crate::overlay::auto_copy_badge::show_error_notification(
                        "Failed to install local AI runtime",
                    );
                }
            }
            Err(err)
        }
    }
}

pub fn start_ai_runtime_install() -> bool {
    if is_ai_runtime_installed()
        || matches!(
            current_ai_runtime_status(),
            AiRuntimeStatus::Installing { .. }
        )
    {
        return false;
    }

    std::thread::spawn(|| {
        let stop_signal = Arc::new(AtomicBool::new(false));
        if let Err(err) = ensure_ai_runtime_installed(stop_signal, AiRuntimeUi::Badge) {
            crate::log_info!("[AI Runtime] Install failed: {err}");
        }
    });
    true
}
