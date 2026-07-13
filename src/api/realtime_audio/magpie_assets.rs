//! On-disk assets for NVIDIA Magpie-Multilingual 357M TTS.
//!
//! Magpie is a NeMo checkpoint and requires NVIDIA NanoCodec at inference
//! time. The app treats both `.nemo` files as one user-facing model install.

use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const MAGPIE_HF_BASE: &str =
    "https://huggingface.co/nvidia/magpie_tts_multilingual_357m/resolve/main";
const MAGPIE_MODELSCOPE_BASE: &str =
    "https://modelscope.cn/models/nvidia/magpie_tts_multilingual_357m/resolve/master";
const CODEC_HF_BASE: &str =
    "https://huggingface.co/nvidia/nemo-nano-codec-22khz-1.89kbps-21.5fps/resolve/main";
const CODEC_MODELSCOPE_BASE: &str =
    "https://modelscope.cn/models/nvidia/nemo-nano-codec-22khz-1.89kbps-21.5fps/resolve/master";

const MAGPIE_MODEL_FILE: &str = "magpie_tts_multilingual_357m.nemo";
const NANOCODEC_FILE: &str = "nemo-nano-codec-22khz-1.89kbps-21.5fps.nemo";
const MAGPIE_MODEL_BYTES: u64 = 1_208_883_200;
const NANOCODEC_BYTES: u64 = 425_021_440;

struct MagpieAsset {
    filename: &'static str,
    expected_bytes: u64,
    primary_base: &'static str,
    fallback_base: &'static str,
}

const ASSETS: &[MagpieAsset] = &[
    MagpieAsset {
        filename: MAGPIE_MODEL_FILE,
        expected_bytes: MAGPIE_MODEL_BYTES,
        primary_base: MAGPIE_HF_BASE,
        fallback_base: MAGPIE_MODELSCOPE_BASE,
    },
    MagpieAsset {
        filename: NANOCODEC_FILE,
        expected_bytes: NANOCODEC_BYTES,
        primary_base: CODEC_HF_BASE,
        fallback_base: CODEC_MODELSCOPE_BASE,
    },
];

static LAST_NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

static MAGPIE_MODEL_DOWNLOADING: AtomicBool = AtomicBool::new(false);

fn set_notice(message: impl Into<String>) {
    *LAST_NOTICE.lock().unwrap() = Some(message.into());
}
fn clear_notice() {
    *LAST_NOTICE.lock().unwrap() = None;
}
pub fn current_magpie_notice() -> Option<String> {
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

pub fn get_magpie_model_dir() -> PathBuf {
    crate::paths::app_local_data_dir()
        .join("models")
        .join("magpie_multilingual_357m")
}

pub fn is_magpie_model_downloading() -> bool {
    MAGPIE_MODEL_DOWNLOADING.load(Ordering::Relaxed)
}

pub fn is_magpie_model_downloaded() -> bool {
    let dir = get_magpie_model_dir();
    ASSETS
        .iter()
        .all(|asset| has_expected_file(&dir.join(asset.filename), asset.expected_bytes))
}

pub fn get_magpie_checkpoint_path() -> PathBuf {
    get_magpie_model_dir().join(MAGPIE_MODEL_FILE)
}

pub fn get_magpie_codec_path() -> PathBuf {
    get_magpie_model_dir().join(NANOCODEC_FILE)
}

pub fn remove_magpie_model() -> Result<()> {
    let dir = get_magpie_model_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| anyhow!("remove {}: {e}", dir.display()))?;
    }
    clear_notice();
    Ok(())
}

fn dl_with_fallback(primary: &str, fallback: &str, path: &Path, stop: &AtomicBool) -> Result<()> {
    match crate::api::realtime_audio::model_loader::download_file(primary, path, stop, false) {
        Ok(()) => Ok(()),
        Err(p_err) => {
            if stop.load(Ordering::Relaxed) {
                return Err(p_err);
            }
            eprintln!("[Magpie] HF failed ({p_err}); ModelScope");
            crate::api::realtime_audio::model_loader::download_file(fallback, path, stop, false)
                .map_err(|f_err| anyhow!("HF+MS both failed: hf={p_err} ms={f_err}"))
        }
    }
}

pub fn download_magpie_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    if is_magpie_model_downloaded() {
        return Ok(());
    }
    if MAGPIE_MODEL_DOWNLOADING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        while is_magpie_model_downloading() {
            if stop.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled while waiting"));
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        return if is_magpie_model_downloaded() {
            Ok(())
        } else {
            Err(anyhow!(
                "Magpie model download did not complete successfully"
            ))
        };
    }

    let result = download_magpie_model_inner(stop, use_badge);
    MAGPIE_MODEL_DOWNLOADING.store(false, Ordering::SeqCst);
    post_state();
    result
}

fn download_magpie_model_inner(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let dir = get_magpie_model_dir();
    fs::create_dir_all(&dir)?;
    let loc = locale();
    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = loc.tool_runtime.magpie_downloading_title.to_string();
        state.download_message = loc.tool_runtime.magpie_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_notice();
    post_state();
    if use_badge {
        crate::overlay::auto_copy_badge::show_progress_notification(
            loc.tool_runtime.magpie_downloading_title,
            loc.tool_runtime.magpie_downloading_message,
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
                    .tool_runtime
                    .magpie_downloading_file
                    .replace("{}", asset.filename);
            }
            post_state();
            let target = dir.join(asset.filename);
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
                    loc.tool_runtime.magpie_downloading_title,
                    &loc.tool_runtime
                        .magpie_downloading_file
                        .replace("{}", asset.filename),
                    progress,
                );
            }
            dl_with_fallback(
                &format!("{}/{}", asset.primary_base, asset.filename),
                &format!("{}/{}", asset.fallback_base, asset.filename),
                &target,
                &stop,
            )?;
            if !has_expected_file(&target, asset.expected_bytes) {
                return Err(anyhow!(
                    "{} downloaded with unexpected size; reinstall Magpie",
                    asset.filename
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
                    loc.tool_runtime.magpie_downloading_title,
                    &loc.tool_runtime
                        .magpie_downloading_file
                        .replace("{}", asset.filename),
                    progress,
                );
            }
        }
        if !is_magpie_model_downloaded() {
            return Err(anyhow!("Magpie download finished with missing files"));
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
