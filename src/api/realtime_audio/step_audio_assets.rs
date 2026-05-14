//! On-disk assets for Step Audio EditX. Open-weights, 3B-param PyTorch model
//! from StepFun-AI. Files land in
//! `dirs::data_dir()/screen-goated-toolbox/models/step_audio_editx/`.
//! Primary: HuggingFace `stepfun-ai/Step-Audio-EditX`. Fallback: ModelScope.

use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use crate::api::realtime_audio::model_loader::download_file;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const HF_BASE: &str = "https://huggingface.co/stepfun-ai/Step-Audio-EditX/resolve/main";
const MODELSCOPE_BASE: &str =
    "https://modelscope.cn/models/stepfun-ai/Step-Audio-EditX/resolve/master";

const FILES: &[&str] = &[
    "config.json",
    "configuration_step1.py",
    "modeling_step1.py",
    "tokenizer.model",
    "tokenizer_config.json",
    "model.safetensors.index.json",
    "model-00001.safetensors",
    "CosyVoice-300M-25Hz/FLOW_VERSION",
    "CosyVoice-300M-25Hz/campplus.onnx",
    "CosyVoice-300M-25Hz/cosyvoice.yaml",
    "CosyVoice-300M-25Hz/flow.pt",
    "CosyVoice-300M-25Hz/hift.pt",
    "CosyVoice-300M-25Hz/speech_tokenizer_v1.onnx",
];

lazy_static::lazy_static! {
    static ref LAST_NOTICE: Mutex<Option<String>> = Mutex::new(None);
}

fn set_notice(message: impl Into<String>) {
    *LAST_NOTICE.lock().unwrap() = Some(message.into());
}
fn clear_notice() {
    *LAST_NOTICE.lock().unwrap() = None;
}
pub fn current_step_audio_notice() -> Option<String> {
    LAST_NOTICE.lock().unwrap().clone()
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

pub fn get_step_audio_model_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("models")
        .join("step_audio_editx")
}

pub fn is_step_audio_model_downloaded() -> bool {
    let dir = get_step_audio_model_dir();
    FILES.iter().all(|f| has_nonempty(&dir.join(f)))
}

pub fn remove_step_audio_model() -> Result<()> {
    let dir = get_step_audio_model_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| anyhow!("remove {}: {e}", dir.display()))?;
    }
    clear_notice();
    Ok(())
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
            eprintln!("[StepAudio] HF failed ({p_err}); ModelScope");
            download_file(fallback, path, stop, use_badge)
                .map_err(|f_err| anyhow!("HF+MS both failed: hf={p_err} ms={f_err}"))
        }
    }
}

pub fn download_step_audio_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let dir = get_step_audio_model_dir();
    fs::create_dir_all(&dir)?;
    let loc = locale();
    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = loc.step_audio_downloading_title.to_string();
        state.download_message = loc.step_audio_downloading_message.to_string();
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
                state.download_message = loc.step_audio_downloading_file.replace("{}", f);
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
        if !is_step_audio_model_downloaded() {
            return Err(anyhow!("Step Audio download finished with missing files"));
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
