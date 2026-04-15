//! On-demand download and installation of sherpa-onnx Windows DLLs.
//!
//! Downloads the official sherpa-onnx shared-lib release from GitHub and
//! extracts the 4 required DLLs:
//!   sherpa-onnx-c-api.dll, sherpa-onnx-cxx-api.dll,
//!   onnxruntime.dll, onnxruntime_providers_shared.dll

use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::HWND;

const SHERPA_ONNX_VERSION: &str = "1.12.35";
/// Official sherpa-onnx shared-lib release for Windows x64 (MD/Release build)
const SHERPA_DLLS_URL: &str = concat!(
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/",
    "v1.12.35/",
    "sherpa-onnx-v1.12.35-win-x64-shared-MD-Release.tar.bz2"
);

const REQUIRED_DLLS: &[&str] = &[
    "sherpa-onnx-c-api.dll",
    "sherpa-onnx-cxx-api.dll",
    "onnxruntime.dll",
    "onnxruntime_providers_shared.dll",
];

pub fn sherpa_bin_dir() -> std::path::PathBuf {
    crate::unpack_dlls::private_bin_dir().join("sherpa-onnx")
}

pub fn is_sherpa_dlls_installed() -> bool {
    let dir = sherpa_bin_dir();
    REQUIRED_DLLS.iter().all(|name| dir.join(name).exists())
}

/// Downloads and installs sherpa-onnx DLLs.
/// `on_progress(p)` with p in 0.0..=1.0.
/// Returns Ok(()) if already installed or after successful install.
pub fn download_sherpa_dlls_with_progress(
    stop_signal: Arc<AtomicBool>,
    on_progress: impl Fn(f32),
) -> Result<()> {
    if is_sherpa_dlls_installed() {
        return Ok(());
    }

    let bin_dir = sherpa_bin_dir();
    std::fs::create_dir_all(&bin_dir)?;

    on_progress(0.05);

    let archive_path = bin_dir.join("sherpa-onnx-v1.12.35-win-x64-shared-MD-Release.tar.bz2");

    crate::api::realtime_audio::model_loader::download_file_with_progress(
        SHERPA_DLLS_URL,
        &archive_path,
        &stop_signal,
        |downloaded, total_bytes| {
            let file_frac = if total_bytes > 0 {
                (downloaded as f32 / total_bytes as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            // Map 0.0..=1.0 → 0.05..=0.75 (extraction is 0.75..=1.0)
            on_progress(0.05 + file_frac * 0.70);
        },
    )?;

    if stop_signal.load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(&archive_path);
        return Err(anyhow!("Download cancelled"));
    }

    on_progress(0.75);

    let temp_dir = bin_dir.join("_extract_tmp");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir)?;

    let status = std::process::Command::new("tar.exe")
        .args([
            "-xjf",
            &archive_path.to_string_lossy(),
            "-C",
            &temp_dir.to_string_lossy(),
        ])
        .status()
        .map_err(|e| anyhow!("Failed to run tar.exe: {e}"))?;

    if !status.success() {
        return Err(anyhow!(
            "tar.exe extraction failed (exit code {:?})",
            status.code()
        ));
    }

    copy_dlls_from_tree(&temp_dir, &bin_dir)?;

    let _ = std::fs::remove_dir_all(&temp_dir);
    let _ = std::fs::remove_file(&archive_path);

    if !is_sherpa_dlls_installed() {
        return Err(anyhow!(
            "sherpa-onnx DLLs not found after extraction — archive layout may have changed"
        ));
    }

    on_progress(1.0);
    Ok(())
}

pub fn download_sherpa_dlls(stop_signal: Arc<AtomicBool>, overlay_hwnd: HWND) -> Result<()> {
    if is_sherpa_dlls_installed() {
        return Ok(());
    }

    crate::log_info!(
        "[Sherpa] Downloading sherpa-onnx v{} DLLs from official release...",
        SHERPA_ONNX_VERSION
    );

    let bin_dir = sherpa_bin_dir();
    std::fs::create_dir_all(&bin_dir)?;

    fn post_download_state() {
        use crate::overlay::realtime_webview::state::REALTIME_HWND;
        unsafe {
            if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(REALTIME_HWND),
                    super::super::WM_DOWNLOAD_PROGRESS,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }
    }

    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = "Downloading sherpa-onnx DLLs".to_string();
        state.download_message = format!(
            "Downloading sherpa-onnx v{} runtime (~50 MB)...",
            SHERPA_ONNX_VERSION
        );
        state.download_progress = 0.0;
    }
    post_download_state();

    let result: Result<()> = (|| {
        let archive_path = bin_dir.join("sherpa-onnx-v1.12.35-win-x64-shared-MD-Release.tar.bz2");

        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.download_message = format!(
                "Downloading sherpa-onnx v{} shared library release...",
                SHERPA_ONNX_VERSION
            );
            state.download_progress = 5.0;
        }
        post_download_state();
        super::super::utils::update_overlay_text(
            overlay_hwnd,
            "Downloading sherpa-onnx runtime DLLs...",
        );

        crate::api::realtime_audio::model_loader::download_file(
            SHERPA_DLLS_URL,
            &archive_path,
            &stop_signal,
            false,
        )?;

        if stop_signal.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&archive_path);
            return Err(anyhow!("Download cancelled"));
        }

        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.download_message = "Extracting sherpa-onnx DLLs...".to_string();
            state.download_progress = 75.0;
        }
        post_download_state();
        super::super::utils::update_overlay_text(
            overlay_hwnd,
            "Extracting sherpa-onnx runtime DLLs...",
        );

        // Use Windows built-in tar.exe to extract into a temp dir
        let temp_dir = bin_dir.join("_extract_tmp");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir)?;

        let status = std::process::Command::new("tar.exe")
            .args([
                "-xjf",
                &archive_path.to_string_lossy(),
                "-C",
                &temp_dir.to_string_lossy(),
            ])
            .status()
            .map_err(|e| anyhow!("Failed to run tar.exe: {e}"))?;

        if !status.success() {
            return Err(anyhow!(
                "tar.exe extraction failed (exit code {:?})",
                status.code()
            ));
        }

        // Find and copy DLLs from any subfolder inside the extracted archive
        copy_dlls_from_tree(&temp_dir, &bin_dir)?;

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_file(&archive_path);

        if !is_sherpa_dlls_installed() {
            return Err(anyhow!(
                "sherpa-onnx DLLs not found after extraction — archive layout may have changed"
            ));
        }

        crate::log_info!("[Sherpa] DLLs installed to {:?}", bin_dir);
        Ok(())
    })();

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    post_download_state();

    result
}

/// Recursively walk the extracted tree and copy any DLL whose name matches REQUIRED_DLLS.
fn copy_dlls_from_tree(src_root: &std::path::Path, dest: &std::path::Path) -> Result<()> {
    let entries =
        std::fs::read_dir(src_root).map_err(|e| anyhow!("Failed to read extract dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            copy_dlls_from_tree(&path, dest)?;
        } else if path.is_file() {
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                if REQUIRED_DLLS.iter().any(|req| *req == name_str.as_ref()) {
                    std::fs::copy(&path, dest.join(name))?;
                    crate::log_info!("[Sherpa] Installed {}", name_str);
                }
            }
        }
    }
    Ok(())
}
