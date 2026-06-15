use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use crate::api::realtime_audio::model_loader::download_file;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const PARAKEET_TDT_REPO_RESOLVE_BASE: &str =
    "https://huggingface.co/maxkulish/parakeet-tdt-0.6b-v3/resolve/main";
const REQUIRED_FILES: &[&str] = &[
    "encoder-model.onnx",
    "encoder-model.onnx.data",
    "decoder_joint-model.onnx",
    "vocab.txt",
];

static LAST_PARAKEET_TDT_ACTION_ERROR: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

fn set_parakeet_tdt_action_error(message: impl Into<String>) {
    *LAST_PARAKEET_TDT_ACTION_ERROR.lock().unwrap() = Some(message.into());
}

fn clear_parakeet_tdt_action_error() {
    *LAST_PARAKEET_TDT_ACTION_ERROR.lock().unwrap() = None;
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
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

fn model_files_present(dir: &Path) -> bool {
    REQUIRED_FILES
        .iter()
        .all(|name| has_nonempty_file(&dir.join(name)))
}

pub fn current_parakeet_tdt_model_notice() -> Option<String> {
    LAST_PARAKEET_TDT_ACTION_ERROR.lock().unwrap().clone()
}

pub fn get_parakeet_tdt_model_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("models")
        .join("parakeet_tdt_0_6b_v3")
}

pub fn is_parakeet_tdt_model_downloaded() -> bool {
    model_files_present(&get_parakeet_tdt_model_dir())
}

pub fn remove_parakeet_tdt_model() -> Result<()> {
    let dir = get_parakeet_tdt_model_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| anyhow!("Failed to remove '{}': {}", dir.display(), err))?;
    }
    clear_parakeet_tdt_action_error();
    Ok(())
}

pub fn download_parakeet_tdt_model(stop_signal: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let dir = get_parakeet_tdt_model_dir();
    let locale = locale();

    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = locale.parakeet_tdt_downloading_title.to_string();
        state.download_message = locale.parakeet_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_parakeet_tdt_action_error();
    post_download_state();

    let result: Result<()> = (|| {
        for filename in REQUIRED_FILES {
            if stop_signal.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled"));
            }

            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = locale.parakeet_downloading_file.replace("{}", filename);
            }
            post_download_state();

            let url = format!("{PARAKEET_TDT_REPO_RESOLVE_BASE}/{filename}");
            download_file(&url, &dir.join(filename), &stop_signal, use_badge)?;
        }

        if !model_files_present(&dir) {
            return Err(anyhow!(
                "Parakeet TDT model download finished with missing files"
            ));
        }
        Ok(())
    })();

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    post_download_state();

    if let Err(err) = &result {
        if !err.to_string().contains("cancelled") {
            set_parakeet_tdt_action_error(err.to_string());
        }
    } else {
        clear_parakeet_tdt_action_error();
    }

    result
}
