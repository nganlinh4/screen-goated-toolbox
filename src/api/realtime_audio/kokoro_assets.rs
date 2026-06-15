//! On-disk asset management for the Kokoro 82M v1.0 offline TTS model.
//!
//! sherpa-onnx publishes a pre-packaged Kokoro v1.0 bundle as a single
//! `.tar.bz2` on GitHub Releases — the HuggingFace mirror is gated (401),
//! so we fetch from there instead. The archive expands into the model dir
//! and includes everything sherpa-onnx OfflineTts needs: `model.onnx`,
//! `voices.bin`, `tokens.txt`, lexicons, plus the `espeak-ng-data/`
//! phonemizer dataset.
//!
//! Files land in `dirs::data_dir()/screen-goated-toolbox/models/kokoro_v1/`.

use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use crate::api::realtime_audio::model_loader::download_file;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// Single archive that bundles model + tokenizer + voice table + espeak data.
const GITHUB_RELEASE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-multi-lang-v1_0.tar.bz2";
const ARCHIVE_FILENAME: &str = "kokoro-multi-lang-v1_0.tar.bz2";
/// The archive expands into this top-level directory; we flatten it into the
/// model dir so paths inside the bundle (`model.onnx`, `espeak-ng-data/...`)
/// land directly where sherpa-onnx looks for them.
const ARCHIVE_TOP_DIR: &str = "kokoro-multi-lang-v1_0";

static LAST_KOKORO_ACTION_ERROR: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

fn set_kokoro_action_error(message: impl Into<String>) {
    *LAST_KOKORO_ACTION_ERROR.lock().unwrap() = Some(message.into());
}

fn clear_kokoro_action_error() {
    *LAST_KOKORO_ACTION_ERROR.lock().unwrap() = None;
}

pub fn current_kokoro_model_notice() -> Option<String> {
    LAST_KOKORO_ACTION_ERROR.lock().unwrap().clone()
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

fn locale() -> crate::gui::locale::LocaleText {
    let app = crate::APP.lock().unwrap();
    crate::gui::locale::LocaleText::get(&app.config.ui_language)
}

fn has_nonempty_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

pub fn get_kokoro_model_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("models")
        .join("kokoro_v1")
}

pub fn get_kokoro_espeak_data_dir() -> PathBuf {
    get_kokoro_model_dir().join("espeak-ng-data")
}

pub fn get_kokoro_lexicon_paths() -> Vec<PathBuf> {
    let dir = get_kokoro_model_dir();
    ["lexicon-us-en.txt", "lexicon-zh.txt"]
        .iter()
        .map(|name| dir.join(name))
        .collect()
}

pub fn get_kokoro_rule_fst_paths() -> Vec<PathBuf> {
    let dir = get_kokoro_model_dir();
    ["date-zh.fst", "number-zh.fst"]
        .iter()
        .map(|name| dir.join(name))
        .collect()
}

fn espeak_data_present(dir: &Path) -> bool {
    has_nonempty_file(&dir.join("phontab"))
}

pub fn is_kokoro_model_downloaded() -> bool {
    let dir = get_kokoro_model_dir();
    has_nonempty_file(&dir.join("model.onnx"))
        && has_nonempty_file(&dir.join("voices.bin"))
        && has_nonempty_file(&dir.join("tokens.txt"))
        && get_kokoro_lexicon_paths()
            .iter()
            .all(|path| has_nonempty_file(path))
        && espeak_data_present(&get_kokoro_espeak_data_dir())
}

pub fn remove_kokoro_model() -> Result<()> {
    let dir = get_kokoro_model_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| anyhow!("Failed to remove '{}': {}", dir.display(), err))?;
    }
    clear_kokoro_action_error();
    Ok(())
}

fn extract_tar_bz2_flat(archive: &Path, dest_dir: &Path) -> Result<()> {
    fs::create_dir_all(dest_dir)?;
    // First extract into a temp parent so we can flatten the leading dir.
    let staging = dest_dir.join(".kokoro_extract_staging");
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
    // Move contents of staging/<ARCHIVE_TOP_DIR>/* into dest_dir/*.
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

pub fn download_kokoro_model(stop_signal: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let dir = get_kokoro_model_dir();
    fs::create_dir_all(&dir)?;

    let locale = locale();

    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = locale.kokoro_downloading_title.to_string();
        state.download_message = locale.kokoro_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_kokoro_action_error();
    post_download_state();

    let result: Result<()> = (|| {
        if stop_signal.load(Ordering::Relaxed) {
            return Err(anyhow!("Download cancelled"));
        }
        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.download_message = locale
                .kokoro_downloading_file
                .replace("{}", ARCHIVE_FILENAME);
        }
        post_download_state();

        let archive = dir.join(ARCHIVE_FILENAME);
        download_file(GITHUB_RELEASE_URL, &archive, &stop_signal, use_badge)?;

        if stop_signal.load(Ordering::Relaxed) {
            return Err(anyhow!("Download cancelled"));
        }
        extract_tar_bz2_flat(&archive, &dir)?;
        let _ = fs::remove_file(&archive);

        if !is_kokoro_model_downloaded() {
            return Err(anyhow!(
                "Kokoro extraction finished but expected files missing under {}",
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
        set_kokoro_action_error(err.to_string());
    }
    if result.is_ok() {
        clear_kokoro_action_error();
    }

    result
}
