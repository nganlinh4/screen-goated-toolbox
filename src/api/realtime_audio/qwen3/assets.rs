use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use crate::api::realtime_audio::model_loader::download_file;
use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const QWEN3_REPO_TREE_URL: &str =
    "https://huggingface.co/api/models/Qwen/Qwen3-ASR-0.6B/tree/main?recursive=1";
const QWEN3_REPO_RESOLVE_BASE: &str = "https://huggingface.co/Qwen/Qwen3-ASR-0.6B/resolve/main";

lazy_static::lazy_static! {
    static ref LAST_QWEN3_MODEL_ACTION_ERROR: Mutex<Option<String>> = Mutex::new(None);
}

#[derive(Deserialize)]
struct HuggingFaceTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
}

fn set_qwen3_model_action_error(message: impl Into<String>) {
    *LAST_QWEN3_MODEL_ACTION_ERROR.lock().unwrap() = Some(message.into());
}

fn clear_qwen3_model_action_error() {
    *LAST_QWEN3_MODEL_ACTION_ERROR.lock().unwrap() = None;
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

fn qwen3_locale() -> crate::gui::locale::LocaleText {
    let app = crate::APP.lock().unwrap();
    crate::gui::locale::LocaleText::get(&app.config.ui_language)
}

fn wanted_huggingface_file(path: &str) -> bool {
    path.ends_with(".safetensors")
        || matches!(
            path,
            "config.json"
                | "generation_config.json"
                | "preprocessor_config.json"
                | "processor_config.json"
                | "special_tokens_map.json"
                | "tokenizer.json"
                | "tokenizer_config.json"
                | "model.safetensors.index.json"
                | "merges.txt"
                | "vocab.json"
                | "added_tokens.json"
        )
}

fn list_qwen3_repo_files() -> Result<Vec<String>> {
    let response = ureq::get(QWEN3_REPO_TREE_URL)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|err| anyhow!("Failed to fetch Qwen3-ASR manifest: {err}"))?;
    let mut reader = response.into_body().into_reader();
    let mut body = String::new();
    reader.read_to_string(&mut body)?;

    let mut files: Vec<String> = serde_json::from_str::<Vec<HuggingFaceTreeEntry>>(&body)?
        .into_iter()
        .filter(|entry| entry.kind == "file" && wanted_huggingface_file(&entry.path))
        .map(|entry| entry.path)
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(anyhow!(
            "Qwen3-ASR manifest did not contain any downloadable model files"
        ));
    }
    Ok(files)
}

pub fn current_qwen3_model_notice() -> Option<String> {
    LAST_QWEN3_MODEL_ACTION_ERROR.lock().unwrap().clone()
}

pub fn get_qwen3_model_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("models")
        .join("qwen3_asr_0_6b")
}

pub fn is_qwen3_model_downloaded() -> bool {
    let dir = get_qwen3_model_dir();
    dir.join("config.json").exists()
        && dir.join("vocab.json").exists()
        && dir.join("merges.txt").exists()
        && dir.join("tokenizer_config.json").exists()
        && fs::read_dir(&dir).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "safetensors")
            })
        })
}

pub fn remove_qwen3_model() -> Result<()> {
    let dir = get_qwen3_model_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| anyhow!("Failed to remove '{}': {}", dir.display(), err))?;
    }
    clear_qwen3_model_action_error();
    Ok(())
}

pub fn download_qwen3_model(stop_signal: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let dir = get_qwen3_model_dir();
    let locale = qwen3_locale();

    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = locale.qwen3_downloading_title.to_string();
        state.download_message = locale.qwen3_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_qwen3_model_action_error();

    if use_badge {
        crate::overlay::auto_copy_badge::show_progress_notification(
            locale.qwen3_downloading_title,
            locale.qwen3_downloading_message,
            0.0,
        );
    }

    post_download_state();

    let result: Result<()> = (|| {
        let files = list_qwen3_repo_files()?;
        for path in files {
            if stop_signal.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled"));
            }

            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = locale.qwen3_downloading_file.replace("{}", &path);
            }
            post_download_state();

            let url = format!("{QWEN3_REPO_RESOLVE_BASE}/{path}");
            download_file(&url, &dir.join(&path), &stop_signal, use_badge)?;
        }

        Ok(())
    })();

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    if use_badge {
        crate::overlay::auto_copy_badge::hide_progress_notification();
    }
    post_download_state();

    if let Err(err) = &result {
        if !err.to_string().contains("cancelled") {
            set_qwen3_model_action_error(err.to_string());
        }
    } else {
        clear_qwen3_model_action_error();
    }

    result
}
