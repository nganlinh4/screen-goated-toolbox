use super::{
    LIBTORCH_URL, QWEN3_RUNTIME_DLL, RUNTIME_DLL_URL, clear_runtime_notice, runtime_locale,
    set_runtime_notice,
};
use anyhow::anyhow;

fn sync_runtime_badge(progress: f32) {
    use crate::overlay::realtime_webview::state::REALTIME_STATE;

    let (title, message) = if let Ok(state) = REALTIME_STATE.lock() {
        let title = if state.download_title.trim().is_empty() {
            runtime_locale().realtime_download_default_title.to_string()
        } else {
            state.download_title.clone()
        };
        (title, state.download_message.clone())
    } else {
        (
            runtime_locale().realtime_download_default_title.to_string(),
            String::new(),
        )
    };

    crate::overlay::auto_copy_badge::show_progress_notification(&title, &message, progress);
}

mod manifest;

use manifest::{
    fetch_runtime_download_manifest, qwen3_libtorch_required_files_present, runtime_manifest_path,
    verify_runtime_dll_against_manifest, verify_runtime_dll_file_against_manifest,
};
pub use manifest::{
    is_qwen3_runtime_managed_installed, qwen3_runtime_installed_size, remove_qwen3_runtime,
};

pub(super) fn qwen3_runtime_dir_is_usable(dir: &std::path::Path) -> bool {
    manifest::qwen3_runtime_dir_is_usable(dir)
}

static RUNTIME_DOWNLOAD_IN_PROGRESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn is_qwen3_runtime_downloading() -> bool {
    RUNTIME_DOWNLOAD_IN_PROGRESS.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn download_qwen3_runtime(
    stop_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    use_badge: bool,
) -> anyhow::Result<()> {
    let capability = crate::runtime_support::supports_qwen3_local_runtime();
    if !capability.is_supported() {
        set_runtime_notice(capability.details.clone());
        if use_badge {
            crate::runtime_support::notify_capability_issue(&capability);
        }
        return Err(anyhow!(capability.details));
    }

    if is_qwen3_runtime_managed_installed() {
        return Ok(());
    }

    // If another thread is already downloading, show its progress and wait
    if RUNTIME_DOWNLOAD_IN_PROGRESS
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        while RUNTIME_DOWNLOAD_IN_PROGRESS.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(300));
            if stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled while waiting"));
            }
        }
        if is_qwen3_runtime_managed_installed() {
            return Ok(());
        }
        return Err(anyhow!("Runtime download did not complete successfully"));
    }

    let locale = runtime_locale();
    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = locale.qwen3_runtime_downloading_title.to_string();
        state.download_message = locale.qwen3_runtime_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_runtime_notice();
    if use_badge {
        sync_runtime_badge(0.0);
    }

    fn post_download_state() {
        use crate::overlay::realtime_webview::state::REALTIME_HWND;
        unsafe {
            if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(REALTIME_HWND),
                    crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }
    }

    post_download_state();

    let result: anyhow::Result<()> = (|| {
        let bin_dir = crate::unpack_dlls::private_bin_dir();
        std::fs::create_dir_all(&bin_dir)?;
        let runtime_dll_path = bin_dir.join(QWEN3_RUNTIME_DLL);
        let runtime_manifest = fetch_runtime_download_manifest()?;
        let local_manifest_path = runtime_manifest_path(&bin_dir);

        // Step 1: Download our DLL from the repo
        if verify_runtime_dll_file_against_manifest(&runtime_dll_path, &runtime_manifest).is_err() {
            let _ = std::fs::remove_file(&runtime_dll_path);
            let _ = std::fs::remove_file(&local_manifest_path);
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = locale
                    .qwen3_downloading_file
                    .replace("{}", QWEN3_RUNTIME_DLL);
                state.download_progress = 0.0;
            }
            post_download_state();
            if use_badge {
                sync_runtime_badge(0.0);
            }

            crate::api::realtime_audio::model_loader::download_file_with_progress(
                RUNTIME_DLL_URL,
                &runtime_dll_path,
                &stop_signal,
                |downloaded, total| {
                    let progress = if total > 0 {
                        (downloaded as f32 / total as f32) * 5.0
                    } else {
                        0.0
                    };
                    if let Ok(mut state) = REALTIME_STATE.lock() {
                        state.download_progress = progress;
                    }
                    if use_badge {
                        sync_runtime_badge(progress);
                    }
                },
            )?;
            verify_runtime_dll_file_against_manifest(&runtime_dll_path, &runtime_manifest)?;
            std::fs::write(
                &local_manifest_path,
                serde_json::to_vec_pretty(&runtime_manifest)?,
            )?;
        } else if !local_manifest_path.exists() {
            std::fs::write(
                &local_manifest_path,
                serde_json::to_vec_pretty(&runtime_manifest)?,
            )?;
        }

        if stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(anyhow!("Download cancelled"));
        }

        // Step 2: Download libtorch from pytorch.org if needed
        if !qwen3_libtorch_required_files_present(&bin_dir) {
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = locale.qwen3_runtime_downloading_libtorch.to_string();
            }
            post_download_state();

            // Use curl as a background process for the large libtorch download
            let libtorch_zip_path = bin_dir.join("libtorch-download.zip");
            let _ = std::fs::remove_file(&libtorch_zip_path);
            let mut curl_child = std::process::Command::new("curl.exe")
                .args([
                    "--fail",
                    "--location",
                    "--continue-at",
                    "-",
                    "--output",
                    &libtorch_zip_path.to_string_lossy(),
                    LIBTORCH_URL,
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|err| anyhow!("Failed to start curl for libtorch download: {err}"))?;

            // Poll curl and update progress based on file size
            let expected_size: u64 = 2_660_000_000; // ~2.5 GB
            loop {
                match curl_child.try_wait() {
                    Ok(Some(status)) => {
                        if !status.success() {
                            return Err(anyhow!(
                                "libtorch download failed (curl exit code {})",
                                status
                            ));
                        }
                        break;
                    }
                    Ok(None) => {
                        // Still running — update progress from file size
                        if stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                            let _ = curl_child.kill();
                            let _ = std::fs::remove_file(&libtorch_zip_path);
                            return Err(anyhow!("Download cancelled"));
                        }
                        let current_size = std::fs::metadata(&libtorch_zip_path)
                            .map(|m| m.len())
                            .unwrap_or(0);
                        let pct = (current_size as f64 / expected_size as f64 * 100.0).min(99.0);
                        let mb = current_size as f64 / 1_048_576.0;
                        let msg = locale
                            .qwen3_runtime_libtorch_progress_fmt
                            .replace("{}", &format!("{:.0}", mb));
                        let progress = pct as f32;
                        if let Ok(mut state) = REALTIME_STATE.lock() {
                            state.download_message = msg.clone();
                            state.download_progress = progress;
                        }
                        if use_badge {
                            sync_runtime_badge(progress);
                        }
                        post_download_state();
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                    Err(err) => return Err(anyhow!("Failed to check curl status: {err}")),
                }
            }

            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = locale.qwen3_runtime_extracting_libtorch.to_string();
            }
            if use_badge {
                sync_runtime_badge(80.0);
            }
            post_download_state();

            // Extract only DLLs from libtorch/lib/ into a staging dir first.
            // This avoids corrupting an existing runtime if extraction is
            // cancelled or interrupted halfway through.
            let libtorch_stage_dir = bin_dir.join("_qwen3_libtorch_extract_tmp");
            let _ = std::fs::remove_dir_all(&libtorch_stage_dir);
            std::fs::create_dir_all(&libtorch_stage_dir)?;
            let file = std::fs::File::open(&libtorch_zip_path)?;
            let mut zip = zip::ZipArchive::new(file)
                .map_err(|err| anyhow!("Failed to open libtorch archive: {err}"))?;
            let total_entries = zip.len();
            let mut extracted = 0usize;
            for idx in 0..total_entries {
                let mut entry = zip
                    .by_index(idx)
                    .map_err(|err| anyhow!("Failed to read libtorch archive entry: {err}"))?;
                let name = match entry.enclosed_name() {
                    Some(path) => path.to_path_buf(),
                    None => continue,
                };
                if entry.is_dir() {
                    continue;
                }
                let name_str = name.to_string_lossy();
                if (name_str.contains("/lib/") || name_str.contains("\\lib\\"))
                    && let Some(file_name) = name.file_name()
                    && file_name.to_string_lossy().ends_with(".dll")
                {
                    extracted += 1;
                    let msg = locale
                        .qwen3_runtime_extracting_dll_fmt
                        .replacen("{}", &extracted.to_string(), 1)
                        .replacen("{}", &file_name.to_string_lossy(), 1);
                    let progress = 80.0 + (extracted as f32 / 50.0) * 20.0;
                    if let Ok(mut state) = REALTIME_STATE.lock() {
                        state.download_message = msg.clone();
                        state.download_progress = progress;
                    }
                    if use_badge {
                        sync_runtime_badge(progress);
                    }
                    post_download_state();
                    let output_path = libtorch_stage_dir.join(file_name);
                    let mut output = std::fs::File::create(&output_path)?;
                    std::io::copy(&mut entry, &mut output)?;
                }
            }
            if !qwen3_libtorch_required_files_present(&libtorch_stage_dir) {
                let _ = std::fs::remove_dir_all(&libtorch_stage_dir);
                return Err(anyhow!(
                    "Extracted libtorch archive is missing required Qwen3 runtime DLLs"
                ));
            }
            for entry in std::fs::read_dir(&libtorch_stage_dir)? {
                let entry = entry?;
                let file_name = entry.file_name();
                let destination = bin_dir.join(&file_name);
                std::fs::rename(entry.path(), &destination).or_else(|_| {
                    std::fs::copy(entry.path(), &destination)?;
                    std::fs::remove_file(entry.path())
                })?;
            }
            let _ = std::fs::remove_dir_all(libtorch_stage_dir);
            let _ = std::fs::remove_file(libtorch_zip_path);
        }

        if !qwen3_runtime_dir_is_usable(&bin_dir)
            || verify_runtime_dll_against_manifest(&runtime_dll_path, &runtime_manifest).is_err()
        {
            return Err(anyhow!(
                "Qwen3 runtime install is incomplete or ABI-incompatible after download"
            ));
        }

        Ok(())
    })();

    RUNTIME_DOWNLOAD_IN_PROGRESS.store(false, std::sync::atomic::Ordering::SeqCst);

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    if use_badge {
        crate::overlay::auto_copy_badge::hide_progress_notification();
    }
    post_download_state();

    if let Err(err) = &result {
        if !err.to_string().contains("cancelled") {
            set_runtime_notice(err.to_string());
        }
    } else {
        clear_runtime_notice();
    }

    result
}
