//! On-disk asset management for Supertonic 3 TTS.
//!
//! sherpa-onnx publishes a ready-to-run int8 Supertonic 3 bundle as one
//! `.tar.bz2` containing the four ONNX graphs, tokenizer config, unicode
//! indexer, and voice style table needed by OfflineTts.

use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use crate::api::realtime_audio::model_loader::download_file;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const GITHUB_RELEASE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/sherpa-onnx-supertonic-3-tts-int8-2026-05-11.tar.bz2";
const ARCHIVE_FILENAME: &str = "sherpa-onnx-supertonic-3-tts-int8-2026-05-11.tar.bz2";
const ARCHIVE_TOP_DIR: &str = "sherpa-onnx-supertonic-3-tts-int8-2026-05-11";

static LAST_SUPERTONIC_ACTION_ERROR: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

fn set_supertonic_action_error(message: impl Into<String>) {
    *LAST_SUPERTONIC_ACTION_ERROR.lock().unwrap() = Some(message.into());
}

fn clear_supertonic_action_error() {
    *LAST_SUPERTONIC_ACTION_ERROR.lock().unwrap() = None;
}

pub fn current_supertonic_model_notice() -> Option<String> {
    LAST_SUPERTONIC_ACTION_ERROR.lock().unwrap().clone()
}

fn post_download_state() {
    use crate::overlay::realtime_webview::state::REALTIME_HWND;
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

fn has_nonempty_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

pub fn get_supertonic_model_dir() -> PathBuf {
    crate::paths::app_models_dir().join("supertonic_3")
}

pub fn is_supertonic_model_downloaded() -> bool {
    let dir = get_supertonic_model_dir();
    [
        "duration_predictor.int8.onnx",
        "text_encoder.int8.onnx",
        "vector_estimator.int8.onnx",
        "vocoder.int8.onnx",
        "tts.json",
        "unicode_indexer.bin",
        "voice.bin",
    ]
    .iter()
    .all(|name| has_nonempty_file(&dir.join(name)))
}

pub fn remove_supertonic_model() -> Result<()> {
    let dir = get_supertonic_model_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| anyhow!("Failed to remove '{}': {}", dir.display(), err))?;
    }
    clear_supertonic_action_error();
    Ok(())
}

fn extract_tar_bz2_flat(archive: &Path, dest_dir: &Path) -> Result<()> {
    fs::create_dir_all(dest_dir)?;
    let staging = dest_dir.join(".supertonic_extract_staging");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let status = std::process::Command::new("tar")
        .arg("-xjf")
        .arg(archive)
        .arg("-C")
        .arg(&staging)
        .status()
        .map_err(|e| anyhow!("Failed to run tar: {e}"))?;
    if !status.success() {
        let _ = fs::remove_dir_all(&staging);
        return Err(anyhow!("tar extraction failed (exit {:?})", status.code()));
    }

    let top = staging.join(ARCHIVE_TOP_DIR);
    let source = if top.is_dir() { top } else { staging.clone() };
    for entry in fs::read_dir(&source).map_err(|e| anyhow!("read staging: {e}"))? {
        let entry = entry.map_err(|e| anyhow!("read staging entry: {e}"))?;
        let dst = dest_dir.join(entry.file_name());
        let _ = fs::remove_dir_all(&dst);
        let _ = fs::remove_file(&dst);
        fs::rename(entry.path(), &dst)
            .map_err(|e| anyhow!("rename {}: {e}", entry.path().display()))?;
    }
    let _ = fs::remove_dir_all(&staging);
    Ok(())
}

pub fn download_supertonic_model(stop_signal: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let dir = get_supertonic_model_dir();
    fs::create_dir_all(&dir)?;

    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = "Downloading Supertonic 3".to_string();
        state.download_message = "Preparing Supertonic 3 model download...".to_string();
        state.download_progress = 0.0;
    }
    clear_supertonic_action_error();
    post_download_state();

    let result: Result<()> = (|| {
        if stop_signal.load(Ordering::Relaxed) {
            return Err(anyhow!("Download cancelled"));
        }
        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.download_message = format!("Downloading {ARCHIVE_FILENAME}");
        }
        post_download_state();

        let archive = dir.join(ARCHIVE_FILENAME);
        download_file(GITHUB_RELEASE_URL, &archive, &stop_signal, use_badge)?;

        if stop_signal.load(Ordering::Relaxed) {
            return Err(anyhow!("Download cancelled"));
        }
        extract_tar_bz2_flat(&archive, &dir)?;
        let _ = fs::remove_file(&archive);

        if !is_supertonic_model_downloaded() {
            return Err(anyhow!(
                "Supertonic extraction finished but expected files missing under {}",
                dir.display()
            ));
        }
        Ok(())
    })();

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    post_download_state();

    if let Err(err) = &result
        && !err.to_string().contains("cancelled")
    {
        set_supertonic_action_error(err.to_string());
    }
    if result.is_ok() {
        clear_supertonic_action_error();
    }

    result
}
