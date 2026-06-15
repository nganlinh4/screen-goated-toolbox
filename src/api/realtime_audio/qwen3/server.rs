use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use crate::api::realtime_audio::model_loader::download_file;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const QWEN3_SERVER_BINARY_NAME: &str = "asr-server.exe";
const DEFAULT_QWEN3_SERVER_URL: &str = "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/native/qwen3_reference_sidecar/dist/asr-server.exe";

static LAST_QWEN3_SERVER_NOTICE: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

fn set_qwen3_server_notice(message: impl Into<String>) {
    *LAST_QWEN3_SERVER_NOTICE.lock().unwrap() = Some(message.into());
}

fn clear_qwen3_server_notice() {
    *LAST_QWEN3_SERVER_NOTICE.lock().unwrap() = None;
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

pub fn current_qwen3_server_notice() -> Option<String> {
    LAST_QWEN3_SERVER_NOTICE.lock().unwrap().clone()
}

pub fn get_qwen3_server_dir() -> PathBuf {
    crate::paths::app_data_dir()
        .join("bin")
        .join("qwen3_asr_reference")
}

pub fn get_qwen3_server_path() -> PathBuf {
    get_qwen3_server_dir().join("asr-server.exe")
}

pub fn is_qwen3_server_managed() -> bool {
    has_nonempty_file(&get_qwen3_server_path())
}

pub fn get_active_qwen3_server_path() -> Option<PathBuf> {
    local_sidecar_candidate_paths()
        .into_iter()
        .find(|path| has_nonempty_file(path))
        .or_else(|| has_nonempty_file(&get_qwen3_server_path()).then(get_qwen3_server_path))
}

fn has_nonempty_file(path: &std::path::Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

pub fn remove_qwen3_server() -> Result<()> {
    let dir = get_qwen3_server_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| anyhow!("Failed to remove '{}': {}", dir.display(), err))?;
    }
    clear_qwen3_server_notice();
    Ok(())
}

pub fn download_qwen3_server(stop_signal: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    if is_qwen3_server_managed() {
        return Ok(());
    }

    let locale = qwen3_locale();
    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = locale.qwen3_server_downloading_title.to_string();
        state.download_message = locale.qwen3_server_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_qwen3_server_notice();

    post_download_state();

    let result: Result<()> = (|| {
        let download_url = qwen3_server_download_url();
        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.download_message = locale
                .qwen3_server_downloading_file
                .replace("{}", QWEN3_SERVER_BINARY_NAME);
        }
        post_download_state();

        let server_path = get_qwen3_server_path();
        let _ = fs::create_dir_all(get_qwen3_server_dir());
        download_file(&download_url, &server_path, &stop_signal, use_badge).map_err(|err| {
            anyhow!(
                "Failed to download Qwen3 reference server executable from '{}': {}. Commit and push '{}' to a git-tracked URL, or set SGT_QWEN3_ASR_SERVER_URL.",
                download_url,
                err,
                "native/qwen3_reference_sidecar/dist/asr-server.exe"
            )
        })?;

        if !is_qwen3_server_managed() {
            return Err(anyhow!(
                "Downloaded Qwen3 reference server executable was not saved successfully"
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
            set_qwen3_server_notice(err.to_string());
        }
    } else {
        clear_qwen3_server_notice();
    }

    result
}

fn qwen3_server_download_url() -> String {
    std::env::var("SGT_QWEN3_ASR_SERVER_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("SGT_QWEN3_ASR_SERVER_BUNDLE_URL")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| DEFAULT_QWEN3_SERVER_URL.to_string())
}

pub fn local_sidecar_candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(repo_root) = repo_root() {
        candidates.push(
            repo_root
                .join("third_party")
                .join("qwen3-asr-rs")
                .join("target")
                .join("release")
                .join("asr-server.exe"),
        );
        candidates.push(
            repo_root
                .join("dist")
                .join("qwen3-asr-reference-windows-x64")
                .join(QWEN3_SERVER_BINARY_NAME),
        );
        candidates.push(
            repo_root
                .join("native")
                .join("qwen3_reference_sidecar")
                .join("dist")
                .join(QWEN3_SERVER_BINARY_NAME),
        );
    }

    candidates
}

fn repo_root() -> Result<PathBuf> {
    let mut seeds = Vec::new();
    if let Ok(dir) = std::env::current_dir() {
        seeds.push(dir);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        seeds.push(parent.to_path_buf());
    }

    for seed in seeds {
        let mut dir = seed;
        loop {
            if dir.join("Cargo.toml").exists() && dir.join(".claude").exists() {
                return Ok(dir);
            }
            if !dir.pop() {
                break;
            }
        }
    }

    Err(anyhow!(
        "Could not locate Screen Goated Toolbox repository root"
    ))
}
