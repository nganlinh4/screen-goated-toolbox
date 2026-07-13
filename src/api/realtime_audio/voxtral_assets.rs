//! On-disk assets for Mistral Voxtral 4B TTS (`mistralai/Voxtral-4B-TTS-2603`).
//! Open weights under CC BY-NC 4.0. Files land in
//! `dirs::data_dir()/screen-goated-toolbox/models/voxtral_tts_2603/`.

use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use crate::api::realtime_audio::model_loader::download_file;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const HF_BASE: &str = "https://huggingface.co/mistralai/Voxtral-4B-TTS-2603/resolve/main";
const MODELSCOPE_BASE: &str =
    "https://modelscope.cn/models/mistralai/Voxtral-4B-TTS-2603/resolve/master";

const FILES: &[&str] = &[
    "params.json",
    "tekken.json",
    "consolidated.safetensors",
    "voice_embedding/ar_male.pt",
    "voice_embedding/casual_female.pt",
    "voice_embedding/casual_male.pt",
    "voice_embedding/cheerful_female.pt",
    "voice_embedding/de_female.pt",
    "voice_embedding/de_male.pt",
    "voice_embedding/es_female.pt",
    "voice_embedding/es_male.pt",
    "voice_embedding/fr_female.pt",
    "voice_embedding/fr_male.pt",
    "voice_embedding/hi_female.pt",
    "voice_embedding/hi_male.pt",
    "voice_embedding/it_female.pt",
    "voice_embedding/it_male.pt",
    "voice_embedding/neutral_female.pt",
    "voice_embedding/neutral_male.pt",
    "voice_embedding/nl_female.pt",
    "voice_embedding/nl_male.pt",
    "voice_embedding/pt_female.pt",
    "voice_embedding/pt_male.pt",
];

static LAST_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

fn set_notice(message: impl Into<String>) {
    *LAST_NOTICE.lock().unwrap() = Some(message.into());
}
fn clear_notice() {
    *LAST_NOTICE.lock().unwrap() = None;
}
fn post_state() {
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

fn has_nonempty(path: &Path) -> bool {
    fs::metadata(path)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

pub fn get_voxtral_model_dir() -> PathBuf {
    crate::paths::app_models_dir().join("voxtral_tts_2603")
}

pub fn is_voxtral_model_downloaded() -> bool {
    let dir = get_voxtral_model_dir();
    FILES.iter().all(|f| has_nonempty(&dir.join(f)))
}

fn dl_with_fallback(
    primary: &str,
    fallback: &str,
    path: &Path,
    stop: &AtomicBool,
    use_badge: bool,
) -> Result<()> {
    match download_file(primary, path, stop, use_badge) {
        Ok(()) => Ok(()),
        Err(p_err) => {
            if stop.load(Ordering::Relaxed) {
                return Err(p_err);
            }
            eprintln!("[Voxtral] HF failed ({p_err}); ModelScope");
            download_file(fallback, path, stop, use_badge)
                .map_err(|f_err| anyhow!("HF+MS both failed: hf={p_err} ms={f_err}"))
        }
    }
}

pub fn download_voxtral_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let dir = get_voxtral_model_dir();
    fs::create_dir_all(&dir)?;
    let loc = locale();
    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = loc.tool_runtime.voxtral_downloading_title.to_string();
        state.download_message = loc.tool_runtime.voxtral_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_notice();
    post_state();

    let result: Result<()> = (|| {
        for f in FILES {
            if stop.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled"));
            }
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = loc.tool_runtime.voxtral_downloading_file.replace("{}", f);
            }
            post_state();
            dl_with_fallback(
                &format!("{HF_BASE}/{f}"),
                &format!("{MODELSCOPE_BASE}/{f}"),
                &dir.join(f),
                &stop,
                use_badge,
            )?;
        }
        if !is_voxtral_model_downloaded() {
            return Err(anyhow!("Voxtral download finished with missing files"));
        }
        Ok(())
    })();
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    post_state();
    if let Err(err) = &result
        && !err.to_string().contains("cancelled")
    {
        set_notice(err.to_string());
    }
    if result.is_ok() {
        clear_notice();
    }
    result
}
