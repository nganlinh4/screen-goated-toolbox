//! On-demand download and installation of sherpa-onnx Windows DLLs.
//!
//! Downloads the official sherpa-onnx shared-lib release from GitHub and
//! extracts the two Sherpa API DLLs. ONNX Runtime is owned by the app-wide AI
//! runtime bundle so every local inference feature uses one verified module.

use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use windows::Win32::Foundation::HWND;

const SHERPA_ONNX_VERSION: &str = "1.13.2";
/// Official sherpa-onnx shared-lib release for Windows x64 (MD/Release build)
const SHERPA_DLLS_URL: &str = concat!(
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/",
    "v1.13.2/",
    "sherpa-onnx-v1.13.2-win-x64-shared-MD-Release.tar.bz2"
);

const REQUIRED_DLLS: &[&str] = &["sherpa-onnx-c-api.dll", "sherpa-onnx-cxx-api.dll"];

const RETIRED_PRIVATE_RUNTIME_DLLS: &[&str] =
    &["onnxruntime.dll", "onnxruntime_providers_shared.dll"];

const VERSION_MARKER: &str = "sherpa_onnx_version.txt";
static SHERPA_PACKAGE_LOCK: Mutex<()> = Mutex::new(());

fn lock_sherpa_package() -> MutexGuard<'static, ()> {
    SHERPA_PACKAGE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn sherpa_bin_dir() -> std::path::PathBuf {
    crate::unpack_dlls::private_bin_dir().join("sherpa-onnx")
}

pub(crate) fn resolved_sherpa_dll_dir() -> std::path::PathBuf {
    #[cfg(any(debug_assertions, test))]
    if let Some(path) = std::env::var_os("SGT_SHERPA_RUNTIME_DIR").map(std::path::PathBuf::from)
        && path.is_absolute()
        && path.join("sherpa-onnx-c-api.dll").is_file()
    {
        return std::fs::canonicalize(&path).unwrap_or(path);
    }

    let private_bin = crate::unpack_dlls::private_bin_dir();
    let candidates = [
        sherpa_bin_dir(),
        private_bin,
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
            .unwrap_or_else(|| std::path::PathBuf::from(".")),
        std::path::PathBuf::from("third_party/sherpa-onnx-win/lib"),
    ];
    candidates
        .iter()
        .find(|path| path.join("sherpa-onnx-c-api.dll").is_file())
        .map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
        .unwrap_or_else(sherpa_bin_dir)
}

pub fn is_sherpa_dlls_installed() -> bool {
    let dir = sherpa_bin_dir();
    required_dlls_present(&dir) && installed_version_matches(&dir)
}

/// Runtime readiness for any Sherpa-backed inference path.
///
/// Keep this distinct from package inventory: the Downloaded Tools UI may still
/// report the Sherpa package itself as installed, while execution additionally
/// requires the shared app-owned ONNX Runtime.
pub fn is_sherpa_runtime_ready() -> bool {
    runtime_dependencies_ready(
        is_sherpa_dlls_installed(),
        crate::unpack_dlls::is_ai_runtime_installed(),
    )
}

fn runtime_dependencies_ready(sherpa_installed: bool, ai_runtime_installed: bool) -> bool {
    sherpa_installed && ai_runtime_installed
}

fn require_sherpa_runtime_ready() -> Result<()> {
    if !is_sherpa_dlls_installed() {
        return Err(anyhow!(
            "sherpa-onnx DLLs not found after extraction — archive layout may have changed"
        ));
    }
    if !crate::unpack_dlls::is_ai_runtime_installed() {
        return Err(anyhow!(
            "shared ONNX Runtime became unavailable during sherpa-onnx installation"
        ));
    }
    Ok(())
}

pub fn remove_sherpa_dlls() -> Result<()> {
    let _package_guard = lock_sherpa_package();
    let dir = sherpa_bin_dir();
    if !dir.exists() {
        return Ok(());
    }

    let mut failures = Vec::new();
    for name in REQUIRED_DLLS
        .iter()
        .chain(RETIRED_PRIVATE_RUNTIME_DLLS)
        .copied()
        .chain(std::iter::once(VERSION_MARKER))
    {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(remove_err) => {
                let pending = dir.join(format!("{name}.delete-pending"));
                let _ = std::fs::remove_file(&pending);
                match std::fs::rename(&path, &pending) {
                    Ok(()) => {}
                    Err(rename_err) => failures.push(format!(
                        "{name}: remove failed ({remove_err}); rename failed ({rename_err})"
                    )),
                }
            }
        }
    }

    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(".delete-pending"))
            .unwrap_or(false)
        {
            let _ = std::fs::remove_file(path);
        }
    }

    let _ = std::fs::remove_dir(&dir);

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "Some sherpa-onnx DLLs are still in use. Close active realtime/Kokoro sessions or restart the app, then remove again. {}",
            failures.join("; ")
        ))
    }
}

/// Downloads and installs sherpa-onnx DLLs.
/// `on_progress(p)` with p in 0.0..=1.0.
/// Returns Ok(()) if already installed or after successful install.
pub fn download_sherpa_dlls_with_progress(
    stop_signal: Arc<AtomicBool>,
    on_progress: impl Fn(f32),
) -> Result<()> {
    let _package_guard = lock_sherpa_package();
    crate::unpack_dlls::ensure_ai_runtime_installed(
        stop_signal.clone(),
        crate::unpack_dlls::AiRuntimeUi::None,
    )?;
    cleanup_pending_delete_files(&sherpa_bin_dir());
    cleanup_retired_private_runtime(&sherpa_bin_dir());
    if is_sherpa_runtime_ready() {
        return Ok(());
    }

    let bin_dir = sherpa_bin_dir();
    std::fs::create_dir_all(&bin_dir)?;

    on_progress(0.05);

    let archive_path = bin_dir.join(format!(
        "sherpa-onnx-v{SHERPA_ONNX_VERSION}-win-x64-shared-MD-Release.tar.bz2"
    ));

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

    install_dlls_from_tree(&temp_dir, &bin_dir)?;
    cleanup_retired_private_runtime(&bin_dir);
    write_version_marker(&bin_dir)?;

    let _ = std::fs::remove_dir_all(&temp_dir);
    let _ = std::fs::remove_file(&archive_path);

    require_sherpa_runtime_ready()?;

    on_progress(1.0);
    Ok(())
}

pub fn download_sherpa_dlls(stop_signal: Arc<AtomicBool>, overlay_hwnd: HWND) -> Result<()> {
    let _package_guard = lock_sherpa_package();
    crate::unpack_dlls::ensure_ai_runtime_installed(
        stop_signal.clone(),
        crate::unpack_dlls::AiRuntimeUi::RealtimeOverlay,
    )?;
    cleanup_pending_delete_files(&sherpa_bin_dir());
    cleanup_retired_private_runtime(&sherpa_bin_dir());
    if is_sherpa_runtime_ready() {
        return Ok(());
    }
    let locale = super::sherpa_locale();

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
        state.download_title = locale.tool_runtime.sherpa_dll_downloading_title.to_string();
        state.download_message = locale
            .tool_runtime
            .sherpa_dll_downloading_runtime_fmt
            .replace("{}", SHERPA_ONNX_VERSION);
        state.download_progress = 0.0;
    }
    post_download_state();

    let result: Result<()> = (|| {
        let archive_path = bin_dir.join(format!(
            "sherpa-onnx-v{SHERPA_ONNX_VERSION}-win-x64-shared-MD-Release.tar.bz2"
        ));

        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.download_message = locale
                .tool_runtime
                .sherpa_dll_downloading_release_fmt
                .replace("{}", SHERPA_ONNX_VERSION);
            state.download_progress = 5.0;
        }
        post_download_state();
        super::super::utils::update_overlay_text(
            overlay_hwnd,
            locale.tool_runtime.sherpa_dll_downloading_overlay,
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
            state.download_message = locale.tool_runtime.sherpa_dll_extracting.to_string();
            state.download_progress = 75.0;
        }
        post_download_state();
        super::super::utils::update_overlay_text(
            overlay_hwnd,
            locale.tool_runtime.sherpa_dll_extracting_overlay,
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

        // Find and stage DLLs from any subfolder before touching the live runtime dir.
        install_dlls_from_tree(&temp_dir, &bin_dir)?;
        cleanup_retired_private_runtime(&bin_dir);
        write_version_marker(&bin_dir)?;

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_file(&archive_path);

        require_sherpa_runtime_ready()?;

        crate::log_info!("[Sherpa] DLLs installed to {:?}", bin_dir);
        Ok(())
    })();

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    post_download_state();

    result
}

fn install_dlls_from_tree(src_root: &std::path::Path, dest: &std::path::Path) -> Result<()> {
    let stage_dir = dest.join("_install_tmp");
    let _ = std::fs::remove_dir_all(&stage_dir);
    std::fs::create_dir_all(&stage_dir)?;

    let result = (|| -> Result<()> {
        copy_dlls_from_tree(src_root, &stage_dir)?;
        if !required_dlls_present(&stage_dir) {
            return Err(anyhow!(
                "sherpa-onnx DLLs not found after extraction — archive layout may have changed"
            ));
        }
        for name in REQUIRED_DLLS {
            std::fs::rename(stage_dir.join(name), dest.join(name))
                .or_else(|_| {
                    std::fs::copy(stage_dir.join(name), dest.join(name))?;
                    std::fs::remove_file(stage_dir.join(name))
                })
                .map_err(|err| anyhow!("Failed to install {name}: {err}"))?;
            crate::log_info!("[Sherpa] Installed {}", name);
        }
        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&stage_dir);
    result
}

fn required_dlls_present(dir: &std::path::Path) -> bool {
    REQUIRED_DLLS
        .iter()
        .all(|name| has_nonempty_file(&dir.join(name)))
}

fn installed_version_matches(dir: &std::path::Path) -> bool {
    let marker = dir.join(VERSION_MARKER);
    std::fs::read_to_string(marker)
        .map(|version| version.trim() == SHERPA_ONNX_VERSION)
        .unwrap_or(false)
}

fn write_version_marker(dir: &std::path::Path) -> Result<()> {
    std::fs::write(dir.join(VERSION_MARKER), SHERPA_ONNX_VERSION)
        .map_err(|err| anyhow!("Failed to write sherpa-onnx version marker: {err}"))
}

fn cleanup_pending_delete_files(dir: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(".delete-pending"))
                .unwrap_or(false)
            {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

fn cleanup_retired_private_runtime(dir: &std::path::Path) {
    for name in RETIRED_PRIVATE_RUNTIME_DLLS {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        if let Err(remove_error) = std::fs::remove_file(&path) {
            let pending = dir.join(format!("{name}.delete-pending"));
            let _ = std::fs::remove_file(&pending);
            if let Err(rename_error) = std::fs::rename(&path, &pending) {
                crate::log_info!(
                    "[Sherpa] Could not retire private {name}: remove failed ({remove_error}); rename failed ({rename_error})"
                );
            }
        }
    }
}

fn has_nonempty_file(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

/// Recursively walk the extracted tree and copy any DLL whose name matches REQUIRED_DLLS.
fn copy_dlls_from_tree(src_root: &std::path::Path, dest: &std::path::Path) -> Result<()> {
    let entries =
        std::fs::read_dir(src_root).map_err(|e| anyhow!("Failed to read extract dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            copy_dlls_from_tree(&path, dest)?;
        } else if path.is_file()
            && let Some(name) = path.file_name()
        {
            let name_str = name.to_string_lossy();
            if REQUIRED_DLLS.iter().any(|req| *req == name_str.as_ref()) {
                std::fs::copy(&path, dest.join(name))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sherpa_package_does_not_own_an_onnx_runtime() {
        assert!(
            REQUIRED_DLLS
                .iter()
                .all(|name| !name.starts_with("onnxruntime"))
        );
        assert_eq!(
            RETIRED_PRIVATE_RUNTIME_DLLS,
            &["onnxruntime.dll", "onnxruntime_providers_shared.dll"]
        );
    }

    #[test]
    fn runtime_readiness_requires_both_packages() {
        assert!(runtime_dependencies_ready(true, true));
        assert!(!runtime_dependencies_ready(true, false));
        assert!(!runtime_dependencies_ready(false, true));
        assert!(!runtime_dependencies_ready(false, false));
    }
}
