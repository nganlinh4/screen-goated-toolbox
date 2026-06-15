//! On-disk assets for Step Audio EditX.
//!
//! The app installs the official AWQ 4-bit EditX checkpoint plus the separate
//! Step-Audio-Tokenizer repo. This is the practical customer path for 16 GB
//! VRAM machines; the bf16 checkpoint is larger and less forgiving.

use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const EDITX_HF_BASE: &str =
    "https://huggingface.co/stepfun-ai/Step-Audio-EditX-AWQ-4bit/resolve/main";
const EDITX_MODELSCOPE_BASE: &str =
    "https://modelscope.cn/models/stepfun-ai/Step-Audio-EditX-AWQ-4bit/resolve/master";
const TOKENIZER_HF_BASE: &str =
    "https://huggingface.co/stepfun-ai/Step-Audio-Tokenizer/resolve/main";
const TOKENIZER_MODELSCOPE_BASE: &str =
    "https://modelscope.cn/models/stepfun-ai/Step-Audio-Tokenizer/resolve/master";

#[derive(Clone, Copy)]
struct StepAsset {
    relative_path: &'static str,
    expected_bytes: u64,
    primary_base: &'static str,
    fallback_base: &'static str,
}

const ASSETS: &[StepAsset] = &[
    StepAsset {
        relative_path: "editx_awq/config.json",
        expected_bytes: 1_412,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/configuration_step1.py",
        expected_bytes: 2_087,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/generation_config.json",
        expected_bytes: 132,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/model.safetensors",
        expected_bytes: 2_502_127_032,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/recipe.yaml",
        expected_bytes: 1_353,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/tokenizer.model",
        expected_bytes: 1_264_044,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/tokenizer_config.json",
        expected_bytes: 757,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/CosyVoice-300M-25Hz/FLOW_VERSION",
        expected_bytes: 42,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/CosyVoice-300M-25Hz/campplus.onnx",
        expected_bytes: 28_303_423,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/CosyVoice-300M-25Hz/cosyvoice.yaml",
        expected_bytes: 1_980,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/CosyVoice-300M-25Hz/flow.pt",
        expected_bytes: 615_274_252,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/CosyVoice-300M-25Hz/hift.pt",
        expected_bytes: 117_228_443,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "editx_awq/CosyVoice-300M-25Hz/speech_tokenizer_v1.onnx",
        expected_bytes: 522_625_011,
        primary_base: EDITX_HF_BASE,
        fallback_base: EDITX_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "tokenizer/linguistic_tokenizer.npy",
        expected_bytes: 2_097_280,
        primary_base: TOKENIZER_HF_BASE,
        fallback_base: TOKENIZER_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "tokenizer/speech_tokenizer_v1.onnx",
        expected_bytes: 522_625_011,
        primary_base: TOKENIZER_HF_BASE,
        fallback_base: TOKENIZER_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "tokenizer/dengcunqin/speech_paraformer-large_asr_nat-zh-cantonese-en-16k-vocab8501-online/am.mvn",
        expected_bytes: 11_203,
        primary_base: TOKENIZER_HF_BASE,
        fallback_base: TOKENIZER_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "tokenizer/dengcunqin/speech_paraformer-large_asr_nat-zh-cantonese-en-16k-vocab8501-online/config.yaml",
        expected_bytes: 2_940,
        primary_base: TOKENIZER_HF_BASE,
        fallback_base: TOKENIZER_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "tokenizer/dengcunqin/speech_paraformer-large_asr_nat-zh-cantonese-en-16k-vocab8501-online/configuration.json",
        expected_bytes: 482,
        primary_base: TOKENIZER_HF_BASE,
        fallback_base: TOKENIZER_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "tokenizer/dengcunqin/speech_paraformer-large_asr_nat-zh-cantonese-en-16k-vocab8501-online/model.pt",
        expected_bytes: 881_120_125,
        primary_base: TOKENIZER_HF_BASE,
        fallback_base: TOKENIZER_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "tokenizer/dengcunqin/speech_paraformer-large_asr_nat-zh-cantonese-en-16k-vocab8501-online/seg_dict",
        expected_bytes: 8_287_834,
        primary_base: TOKENIZER_HF_BASE,
        fallback_base: TOKENIZER_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "tokenizer/dengcunqin/speech_paraformer-large_asr_nat-zh-cantonese-en-16k-vocab8501-online/tokens.json",
        expected_bytes: 99_450,
        primary_base: TOKENIZER_HF_BASE,
        fallback_base: TOKENIZER_MODELSCOPE_BASE,
    },
    StepAsset {
        relative_path: "tokenizer/dengcunqin/speech_paraformer-large_asr_nat-zh-cantonese-en-16k-vocab8501-online/tokens.txt",
        expected_bytes: 39_940,
        primary_base: TOKENIZER_HF_BASE,
        fallback_base: TOKENIZER_MODELSCOPE_BASE,
    },
];

static LAST_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

static STEP_AUDIO_MODEL_DOWNLOADING: AtomicBool = AtomicBool::new(false);

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

fn has_expected_file(path: &Path, expected_bytes: u64) -> bool {
    fs::metadata(path)
        .map(|m| m.is_file() && m.len() == expected_bytes)
        .unwrap_or(false)
}

pub fn get_step_audio_model_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("models")
        .join("step_audio_editx")
}

pub fn get_step_audio_editx_dir() -> PathBuf {
    get_step_audio_model_dir().join("editx_awq")
}

pub fn get_step_audio_tokenizer_dir() -> PathBuf {
    get_step_audio_model_dir().join("tokenizer")
}

pub fn is_step_audio_model_downloading() -> bool {
    STEP_AUDIO_MODEL_DOWNLOADING.load(Ordering::Relaxed)
}

pub fn is_step_audio_model_downloaded() -> bool {
    let dir = get_step_audio_model_dir();
    ASSETS
        .iter()
        .all(|asset| has_expected_file(&dir.join(asset.relative_path), asset.expected_bytes))
}

pub fn remove_step_audio_model() -> Result<()> {
    let dir = get_step_audio_model_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| anyhow!("remove {}: {e}", dir.display()))?;
    }
    clear_notice();
    Ok(())
}

fn source_relative_path(relative_path: &str) -> &str {
    relative_path
        .strip_prefix("editx_awq/")
        .or_else(|| relative_path.strip_prefix("tokenizer/"))
        .unwrap_or(relative_path)
}

fn dl_with_fallback(primary: &str, fallback: &str, path: &Path, stop: &AtomicBool) -> Result<()> {
    match crate::api::realtime_audio::model_loader::download_file(primary, path, stop, false) {
        Ok(()) => Ok(()),
        Err(p_err) => {
            if stop.load(Ordering::Relaxed) {
                return Err(p_err);
            }
            eprintln!("[StepAudio] HF failed ({p_err}); ModelScope");
            crate::api::realtime_audio::model_loader::download_file(fallback, path, stop, false)
                .map_err(|f_err| anyhow!("HF+MS both failed: hf={p_err} ms={f_err}"))
        }
    }
}

pub fn download_step_audio_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    if is_step_audio_model_downloaded() {
        return Ok(());
    }
    if STEP_AUDIO_MODEL_DOWNLOADING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        while is_step_audio_model_downloading() {
            if stop.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled while waiting"));
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        return if is_step_audio_model_downloaded() {
            Ok(())
        } else {
            Err(anyhow!(
                "Step Audio EditX download did not complete successfully"
            ))
        };
    }

    let result = download_step_audio_model_inner(stop, use_badge);
    STEP_AUDIO_MODEL_DOWNLOADING.store(false, Ordering::SeqCst);
    post_state();
    result
}

fn download_step_audio_model_inner(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
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
    if use_badge {
        crate::overlay::auto_copy_badge::show_progress_notification(
            loc.step_audio_downloading_title,
            loc.step_audio_downloading_message,
            0.0,
        );
    }

    let result: Result<()> = (|| {
        let total_bytes: u64 = ASSETS.iter().map(|asset| asset.expected_bytes).sum();
        let mut installed_bytes = 0_u64;
        for asset in ASSETS {
            if stop.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled"));
            }
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = loc
                    .step_audio_downloading_file
                    .replace("{}", asset.relative_path);
            }
            post_state();
            let target = dir.join(asset.relative_path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            if target.exists() && !has_expected_file(&target, asset.expected_bytes) {
                let _ = fs::remove_file(&target);
            }
            if use_badge {
                let progress = if total_bytes > 0 {
                    (installed_bytes as f32 / total_bytes as f32) * 100.0
                } else {
                    0.0
                };
                crate::overlay::auto_copy_badge::show_progress_notification(
                    loc.step_audio_downloading_title,
                    &loc.step_audio_downloading_file
                        .replace("{}", asset.relative_path),
                    progress,
                );
            }
            let source_path = source_relative_path(asset.relative_path);
            dl_with_fallback(
                &format!("{}/{}", asset.primary_base, source_path),
                &format!("{}/{}", asset.fallback_base, source_path),
                &target,
                &stop,
            )?;
            if !has_expected_file(&target, asset.expected_bytes) {
                return Err(anyhow!(
                    "{} downloaded with unexpected size; reinstall Step Audio EditX",
                    asset.relative_path
                ));
            }
            installed_bytes = installed_bytes.saturating_add(asset.expected_bytes);
            if use_badge {
                let progress = if total_bytes > 0 {
                    (installed_bytes as f32 / total_bytes as f32) * 100.0
                } else {
                    100.0
                };
                crate::overlay::auto_copy_badge::show_progress_notification(
                    loc.step_audio_downloading_title,
                    &loc.step_audio_downloading_file
                        .replace("{}", asset.relative_path),
                    progress,
                );
            }
        }
        if !is_step_audio_model_downloaded() {
            return Err(anyhow!(
                "Step Audio EditX download finished with missing files"
            ));
        }
        Ok(())
    })();
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    post_state();
    if use_badge {
        crate::overlay::auto_copy_badge::hide_progress_notification();
    }
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
