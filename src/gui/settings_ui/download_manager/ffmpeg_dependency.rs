use super::run::{FFMPEG_RELEASE_MARKER_FILE, fetch_btbn_release_label, ffmpeg_download_url};
use super::types::InstallStatus;
use super::utils::{download_file, extract_ffmpeg};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub fn ffmpeg_exe_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or(PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
        .join("ffmpeg.exe")
}

pub fn ensure_ffmpeg_with_badge() -> Result<PathBuf, String> {
    ensure_ffmpeg_with_badge_message("")
}

pub fn ensure_ffmpeg_with_badge_message(download_message: &str) -> Result<PathBuf, String> {
    let ffmpeg_path = ffmpeg_exe_path();
    if ffmpeg_path.exists() {
        return Ok(ffmpeg_path);
    }

    let mut locale = localized_badge_text();
    if !download_message.trim().is_empty() {
        locale.downloading = download_message.to_string();
    }

    let bin_dir = ffmpeg_path
        .parent()
        .map(|path| path.to_path_buf())
        .ok_or_else(|| "Could not resolve FFmpeg install directory".to_string())?;
    fs::create_dir_all(&bin_dir).map_err(|err| err.to_string())?;

    let status = Arc::new(Mutex::new(InstallStatus::Downloading(0.0)));
    let cancel = Arc::new(AtomicBool::new(false));
    let progress_status = status.clone();
    let progress_cancel = cancel.clone();
    let progress_locale = locale.clone();
    let progress_thread = std::thread::spawn(move || {
        loop {
            let current = progress_status.lock().ok().map(|guard| guard.clone());
            match current {
                Some(InstallStatus::Downloading(progress)) => {
                    crate::overlay::auto_copy_badge::show_progress_notification(
                        &progress_locale.installing,
                        &progress_locale.downloading,
                        progress * 100.0,
                    );
                }
                Some(InstallStatus::Extracting) => {
                    crate::overlay::auto_copy_badge::show_progress_notification(
                        &progress_locale.installing,
                        &progress_locale.extracting,
                        95.0,
                    );
                }
                _ => break,
            }
            if progress_cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    });

    let result = install_ffmpeg(&bin_dir, &status, &cancel);
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = progress_thread.join();
    crate::overlay::auto_copy_badge::hide_progress_notification();

    match result {
        Ok(()) => {
            crate::overlay::auto_copy_badge::show_notification(&locale.installed);
            Ok(ffmpeg_path)
        }
        Err(error) => {
            crate::overlay::auto_copy_badge::show_error_notification(&locale.failed);
            Err(error)
        }
    }
}

#[derive(Clone)]
struct FfmpegBadgeText {
    installing: String,
    downloading: String,
    extracting: String,
    installed: String,
    failed: String,
}

fn localized_badge_text() -> FfmpegBadgeText {
    let ui_language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let text = crate::gui::locale::LocaleText::get(&ui_language);
    FfmpegBadgeText {
        installing: text.tts_playground_ffmpeg_installing.to_string(),
        downloading: text.tts_playground_ffmpeg_downloading.to_string(),
        extracting: text.tts_playground_ffmpeg_extracting.to_string(),
        installed: text.tts_playground_ffmpeg_installed.to_string(),
        failed: text.tts_playground_ffmpeg_failed.to_string(),
    }
}

fn install_ffmpeg(
    bin_dir: &std::path::Path,
    status: &Arc<Mutex<InstallStatus>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let remote_release = fetch_btbn_release_label().ok();
    let zip_path = bin_dir.join("ffmpeg.zip");
    download_file(ffmpeg_download_url(), &zip_path, status, cancel)?;

    *status.lock().unwrap() = InstallStatus::Extracting;
    extract_ffmpeg(&zip_path, bin_dir)?;
    let _ = fs::remove_file(&zip_path);

    if let Some(label) = remote_release {
        let _ = fs::write(bin_dir.join(FFMPEG_RELEASE_MARKER_FILE), label);
    }

    *status.lock().unwrap() = InstallStatus::Installed;
    Ok(())
}
