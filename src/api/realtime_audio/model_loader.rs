use anyhow::{Result, anyhow};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(not(feature = "recorder-worker"))]
use std::sync::LazyLock;

#[cfg(not(feature = "recorder-worker"))]
static LAST_PARAKEET_ACTION_ERROR: LazyLock<std::sync::Mutex<Option<String>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));

#[cfg(not(feature = "recorder-worker"))]
fn set_parakeet_action_error(message: impl Into<String>) {
    *LAST_PARAKEET_ACTION_ERROR.lock().unwrap() = Some(message.into());
}

#[cfg(not(feature = "recorder-worker"))]
fn clear_parakeet_action_error() {
    *LAST_PARAKEET_ACTION_ERROR.lock().unwrap() = None;
}

#[cfg(not(feature = "recorder-worker"))]
pub fn current_parakeet_model_notice() -> Option<String> {
    LAST_PARAKEET_ACTION_ERROR
        .lock()
        .unwrap()
        .clone()
        .or_else(|| {
            super::local_asr_worker::model_notice(super::local_asr_worker::ModelKind::RealtimeEou)
        })
}

// Helper function to download file or read local file
#[cfg(not(feature = "recorder-worker"))]
pub fn download_file(
    url: &str,
    path: &Path,
    stop_signal: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    download_file_with_progress(url, path, stop_signal, |_, _| {})
}

/// Like `download_file` but calls `on_progress(downloaded_bytes, total_bytes)` every ~100ms.
/// `total_bytes` is 0 if Content-Length is not available.
pub fn download_file_with_progress(
    url: &str,
    path: &Path,
    stop_signal: &std::sync::atomic::AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    if path.exists() {
        let usable_existing = fs::metadata(path)
            .map(|metadata| metadata.is_file() && metadata.len() > 0)
            .unwrap_or(false);
        if usable_existing {
            return Ok(());
        }
        let _ = fs::remove_file(path);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("tmp");
    let _ = fs::remove_file(&temp_path);

    use std::io::Write;

    let result = (|| -> Result<()> {
        println!("Downloading file from: {}", url);
        let response = crate::api::client::UREQ_DOWNLOAD_AGENT
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
            .call()
            .map_err(|e| anyhow!("Download failed: {}", e))?;

        let total_size = response
            .headers()
            .get("content-length")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let mut reader = response.into_body().into_reader();
        let mut file = fs::File::create(&temp_path)?;

        let mut buffer = [0; 8192];
        let mut downloaded: u64 = 0;

        let update_interval = std::time::Duration::from_millis(100);
        let mut last_update = std::time::Instant::now();

        loop {
            if stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled"));
            }

            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            file.write_all(&buffer[..bytes_read])?;
            downloaded += bytes_read as u64;

            if last_update.elapsed() >= update_interval {
                on_progress(downloaded, total_size);

                if total_size > 0 {
                    let progress = (downloaded as f32 / total_size as f32) * 100.0;
                    use crate::overlay::realtime_webview::state::REALTIME_STATE;
                    if let Ok(mut state) = REALTIME_STATE.lock() {
                        state.download_progress = progress;
                    }
                }
                last_update = std::time::Instant::now();

                use super::WM_DOWNLOAD_PROGRESS;
                use crate::overlay::realtime_webview::state::REALTIME_HWND;
                use windows::Win32::Foundation::{LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

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
        }

        file.flush()?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp_path, path)?;
        on_progress(total_size.max(downloaded), total_size.max(downloaded));
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }

    result
}

#[derive(Clone, Copy)]
pub(crate) struct FileContract {
    pub(crate) name: &'static str,
    pub(crate) url: &'static str,
    pub(crate) size_bytes: u64,
    pub(crate) sha256: &'static str,
}

pub(crate) fn verified_file_present(path: &Path, contract: FileContract) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() != contract.size_bytes {
        return false;
    }
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let Ok(read) = file.read(&mut buffer) else {
            return false;
        };
        if read == 0 {
            break;
        }
        use sha2::Digest as _;
        hasher.update(&buffer[..read]);
    }
    use sha2::Digest as _;
    format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(contract.sha256)
}

pub(crate) fn contract_file_present(path: &Path, contract: FileContract) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| {
            metadata.is_file()
                && !is_reparse_point(&metadata)
                && metadata.len() == contract.size_bytes
        })
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

pub(crate) fn download_verified_file_with_progress(
    contract: FileContract,
    url: &str,
    path: &Path,
    stop_signal: &std::sync::atomic::AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<()> {
    if verified_file_present(path, contract) {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("model file has no parent directory"))?;
    fs::create_dir_all(parent)?;
    let temp_path = path.with_extension("verified-download");
    let _ = fs::remove_file(&temp_path);
    let result = (|| -> Result<()> {
        let response = crate::api::client::UREQ_DOWNLOAD_AGENT
            .get(url)
            .header("User-Agent", "ScreenGoatedToolbox")
            .call()
            .map_err(|error| anyhow!("Download failed for {}: {error}", contract.name))?;
        if response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|size| size != contract.size_bytes)
        {
            return Err(anyhow!(
                "Download size for {} does not match this build",
                contract.name
            ));
        }
        let mut reader = response.into_body().into_reader();
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)?;
        let mut hasher = sha2::Sha256::new();
        let mut downloaded = 0_u64;
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            if stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled"));
            }
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            downloaded = downloaded
                .checked_add(read as u64)
                .filter(|bytes| *bytes <= contract.size_bytes)
                .ok_or_else(|| anyhow!("Download for {} exceeds its limit", contract.name))?;
            output.write_all(&buffer[..read])?;
            use sha2::Digest as _;
            hasher.update(&buffer[..read]);
            on_progress(downloaded, contract.size_bytes);
            let progress = downloaded as f32 / contract.size_bytes as f32 * 100.0;
            if let Ok(mut state) = crate::overlay::realtime_webview::state::REALTIME_STATE.lock() {
                state.download_progress = progress;
            }
        }
        output.flush()?;
        output.sync_all()?;
        use sha2::Digest as _;
        let digest = format!("{:x}", hasher.finalize());
        if downloaded != contract.size_bytes || !digest.eq_ignore_ascii_case(contract.sha256) {
            return Err(anyhow!(
                "Downloaded {} failed integrity verification",
                contract.name
            ));
        }
        drop(output);
        replace_managed_file(&temp_path, path)?;
        on_progress(contract.size_bytes, contract.size_bytes);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn replace_managed_file(temp_path: &Path, path: &Path) -> Result<()> {
    if !path.exists() {
        fs::rename(temp_path, path)?;
        return Ok(());
    }

    let backup_path = path.with_extension("unverified-backup");
    if backup_path.exists() {
        fs::remove_file(&backup_path)?;
    }
    fs::rename(path, &backup_path)?;
    if let Err(error) = fs::rename(temp_path, path) {
        let _ = fs::rename(&backup_path, path);
        return Err(error.into());
    }
    fs::remove_file(backup_path)?;
    Ok(())
}

pub fn get_parakeet_model_dir() -> PathBuf {
    crate::paths::app_models_dir().join("parakeet")
}

#[cfg(not(feature = "recorder-worker"))]
pub fn is_model_downloaded() -> bool {
    let dir = get_parakeet_model_dir();
    PARAKEET_EOU_FILES
        .iter()
        .all(|contract| contract_file_present(&dir.join(contract.name), *contract))
}

#[cfg(not(feature = "recorder-worker"))]
const PARAKEET_EOU_REVISION: &str = "a61d2818df4659c956b9661a9447f46e98c15126";
const PARAKEET_EOU_FILES: &[FileContract] = &[
    FileContract {
        name: "encoder.onnx",
        url: concat!(
            "https://huggingface.co/altunenes/parakeet-rs/resolve/",
            "a61d2818df4659c956b9661a9447f46e98c15126/",
            "realtime_eou_120m-v1-onnx/encoder.onnx"
        ),
        size_bytes: 459_341_289,
        sha256: "d472887cc38a784a5bfc21c2dbe247639edc3b3f9992388d8ceceaec07256b5b",
    },
    FileContract {
        name: "decoder_joint.onnx",
        url: concat!(
            "https://huggingface.co/altunenes/parakeet-rs/resolve/",
            "a61d2818df4659c956b9661a9447f46e98c15126/",
            "realtime_eou_120m-v1-onnx/decoder_joint.onnx"
        ),
        size_bytes: 21_347_639,
        sha256: "9d2553ac043c2fc5f69e970769b0fb8ab9103fbfdeb7d26a1ea9729d4bd2dddd",
    },
    FileContract {
        name: "tokenizer.json",
        url: concat!(
            "https://huggingface.co/altunenes/parakeet-rs/resolve/",
            "a61d2818df4659c956b9661a9447f46e98c15126/",
            "realtime_eou_120m-v1-onnx/tokenizer.json"
        ),
        size_bytes: 20_053,
        sha256: "f6b0ad8690559351fa478116fe0985a203b76f7c040f3a9381f485c99c0325f8",
    },
];

pub(crate) fn parakeet_model_contracts() -> &'static [FileContract] {
    PARAKEET_EOU_FILES
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_parakeet_model() -> Result<()> {
    let _owners = crate::overlay::component_removal::stop_audio_owners()?;
    let dir = get_parakeet_model_dir();
    clear_parakeet_action_error();
    super::local_asr_worker::request_model_remove(
        super::local_asr_worker::ModelKind::RealtimeEou,
        &dir,
    )?;
    Ok(())
}

#[cfg(not(feature = "recorder-worker"))]
pub fn redownload_parakeet_model(
    stop_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    use_badge: bool,
) -> Result<()> {
    let outcome = super::local_asr_worker::request_model_remove(
        super::local_asr_worker::ModelKind::RealtimeEou,
        &get_parakeet_model_dir(),
    )?;
    if outcome == super::local_asr_worker::ModelRemovalOutcome::Pending {
        return Err(anyhow!(
            "Parakeet model repair is pending until transcription stops"
        ));
    }
    download_parakeet_model(stop_signal, use_badge)
}

#[cfg(not(feature = "recorder-worker"))]
pub fn download_parakeet_model(
    stop_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    use_badge: bool,
) -> Result<()> {
    let _activity = crate::install_activity::register(stop_signal.clone())?;
    let dir = get_parakeet_model_dir();

    let locale = {
        let app = crate::APP.lock().unwrap();
        crate::gui::locale::LocaleText::get(&app.config.ui_language)
    };

    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = locale.tool_runtime.parakeet_downloading_title.to_string();
        state.download_message = locale.tool_runtime.parakeet_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_parakeet_action_error();
    let badge = use_badge.then(|| {
        crate::overlay::auto_copy_badge::DownloadProgressBadge::new(
            locale.auxiliary.managed_tools.tool_parakeet,
        )
    });

    use super::WM_DOWNLOAD_PROGRESS;
    use crate::overlay::realtime_webview::state::REALTIME_HWND;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

    println!("Parakeet model not found, starting download. Modal should appear now...");

    // Small delay to ensure WebView is ready to receive the message
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Send the message multiple times initially to ensure WebView receives it
    for _ in 0..3 {
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
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let result: Result<()> = (|| {
        crate::log_info!("[Parakeet] model revision {PARAKEET_EOU_REVISION}");
        let total_bytes = PARAKEET_EOU_FILES
            .iter()
            .map(|contract| contract.size_bytes)
            .sum::<u64>();
        let mut completed_bytes = 0_u64;
        for contract in PARAKEET_EOU_FILES {
            let filename = contract.name;
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = locale
                    .tool_runtime
                    .parakeet_downloading_file
                    .replace("{}", filename);
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

            download_verified_file_with_progress(
                *contract,
                contract.url,
                &dir.join(filename),
                &stop_signal,
                |done, _| {
                    if let Some(badge) = &badge {
                        badge.report(completed_bytes.saturating_add(done), total_bytes);
                    }
                },
            )?;
            completed_bytes = completed_bytes.saturating_add(contract.size_bytes);
        }

        Ok(())
    })();

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
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

    if let Err(err) = &result {
        if !err.to_string().contains("cancelled") {
            set_parakeet_action_error(err.to_string());
        }
    } else {
        clear_parakeet_action_error();
    }

    result
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod parakeet_contract_tests {
    use super::*;

    #[test]
    fn realtime_model_urls_are_commit_pinned_and_integrity_bounded() {
        assert_eq!(PARAKEET_EOU_REVISION.len(), 40);
        for file in PARAKEET_EOU_FILES {
            assert!(file.url.contains(PARAKEET_EOU_REVISION));
            assert!(file.size_bytes > 0);
            assert_eq!(file.sha256.len(), 64);
        }
    }
}
