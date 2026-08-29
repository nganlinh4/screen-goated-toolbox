mod catalog;
pub mod dlls;
pub mod ffi;
pub mod ffi_tts;
mod streaming;
mod success_cache;

pub use catalog::ZipformerLanguage;

use super::state::SharedRealtimeState;
use super::utils::update_overlay_text;
use anyhow::{Result, anyhow};
use std::ffi::CString;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use windows::Win32::Foundation::HWND;

use self::streaming::{SherpaStreamingLoop, run_streaming_loop, start_audio_capture};

pub(super) fn sherpa_locale() -> crate::gui::locale::LocaleText {
    let ui_language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&ui_language)
}

fn model_dir(lang: ZipformerLanguage) -> std::path::PathBuf {
    crate::paths::app_models_dir().join(lang.model_dir_name())
}

pub fn is_model_downloaded(lang: ZipformerLanguage) -> bool {
    let dir = model_dir(lang);
    lang.model_files().iter().all(|file| {
        crate::api::realtime_audio::model_loader::verified_file_present(
            &dir.join(file.name),
            file.contract(),
        )
    })
}

pub fn is_model_payload_present(lang: ZipformerLanguage) -> bool {
    let dir = model_dir(lang);
    lang.model_files().iter().all(|file| {
        crate::api::realtime_audio::model_loader::contract_file_present(
            &dir.join(file.name),
            file.contract(),
        )
    })
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_model(lang: ZipformerLanguage) -> Result<()> {
    let _owners = crate::overlay::component_removal::stop_audio_owners()?;
    let dir = model_dir(lang);
    let Ok(metadata) = std::fs::symlink_metadata(&dir) else {
        return Ok(());
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(anyhow!("Zipformer model directory is unsafe"));
    }

    let mut preserved = Vec::new();
    for file in lang.model_files() {
        let target = dir.join(file.name);
        match std::fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                std::fs::remove_file(&target)?;
            }
            Ok(_) => preserved.push(target),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        for extension in ["tmp", "verified-download", "unverified-backup"] {
            let temporary = dir.join(file.name).with_extension(extension);
            remove_regular_temporary(&temporary, &mut preserved)?;
        }
    }

    if std::fs::read_dir(&dir)?.next().is_none() {
        std::fs::remove_dir(&dir)?;
    }
    if preserved.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "Zipformer model contains {} modified or unsafe managed file(s)",
            preserved.len()
        ))
    }
}

#[cfg(not(feature = "recorder-worker"))]
fn remove_regular_temporary(
    path: &std::path::Path,
    preserved: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.is_file() && !metadata.file_type().is_symlink() {
        std::fs::remove_file(path)?;
    } else {
        preserved.push(path.to_path_buf());
    }
    Ok(())
}

/// Downloads all files for `lang`.
/// `on_progress(p)` is called continuously with p in 0.0..=1.0 (byte-level within each file).
/// Returns Ok(()) on success (already-downloaded files are skipped).
pub fn download_model_with_progress(
    lang: ZipformerLanguage,
    stop_signal: Arc<AtomicBool>,
    on_progress: impl Fn(f32),
) -> Result<()> {
    let _activity = crate::install_activity::register(stop_signal.clone())?;
    let dir = model_dir(lang);
    std::fs::create_dir_all(&dir)?;

    let base_url = lang.download_base_url();
    let files = lang.model_files();
    let total = files.len() as f32;

    on_progress(0.0);

    for (i, file) in files.iter().enumerate() {
        if stop_signal.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("Download cancelled"));
        }
        let target = dir.join(file.name);
        if crate::api::realtime_audio::model_loader::verified_file_present(&target, file.contract())
        {
            on_progress((i + 1) as f32 / total);
            continue;
        }
        let file_start = i as f32 / total;
        let file_end = (i + 1) as f32 / total;
        let url = format!("{base_url}/{}", file.name);
        crate::log_info!("[Sherpa] Downloading {} from {url}", file.name);
        crate::api::realtime_audio::model_loader::download_verified_file_with_progress(
            file.contract(),
            &url,
            &target,
            &stop_signal,
            |downloaded, total_bytes| {
                let file_frac = if total_bytes > 0 {
                    (downloaded as f32 / total_bytes as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                on_progress(file_start + file_frac * (file_end - file_start));
            },
        )?;
        on_progress(file_end);
    }
    on_progress(1.0);
    Ok(())
}

pub fn download_model(
    lang: ZipformerLanguage,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
) -> Result<()> {
    let _activity = crate::install_activity::register(stop_signal.clone())?;
    let locale = sherpa_locale();

    fn post_download_state() {
        use crate::overlay::realtime_webview::state::REALTIME_HWND;
        unsafe {
            if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(REALTIME_HWND),
                    super::WM_DOWNLOAD_PROGRESS,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }
    }

    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = locale
            .tool_runtime
            .zipformer_downloading_title_fmt
            .replace("{}", lang.display_name());
        state.download_message = locale.tool_runtime.zipformer_downloading_start.to_string();
        state.download_progress = 0.0;
    }
    post_download_state();

    let result = download_model_with_progress(lang, stop_signal, |pct| {
        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.download_progress = pct * 100.0;
        }
        post_download_state();
        update_overlay_text(
            overlay_hwnd,
            &locale
                .tool_runtime
                .zipformer_downloading_overlay_fmt
                .replace("{}", lang.display_name()),
        );
    });

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    post_download_state();

    result
}

/// Holds CStrings alive while config points into them.
struct ConfigStrings {
    encoder: CString,
    decoder: CString,
    joiner: CString,
    tokens: CString,
    model_type: CString,
    bpe_vocab: CString,
    provider: CString,
    decoding_method: CString,
}

fn build_recognizer_config(
    lang: ZipformerLanguage,
) -> Result<(ffi::SherpaOnnxOnlineRecognizerConfig, ConfigStrings)> {
    let dir = model_dir(lang);
    let dir_str = dir.to_string_lossy();

    let bpe_path = dir.join("bpe.model");
    let strings = ConfigStrings {
        encoder: CString::new(format!("{}/{}", dir_str, lang.encoder_file()))?,
        decoder: CString::new(format!("{}/{}", dir_str, lang.decoder_file()))?,
        joiner: CString::new(format!("{}/{}", dir_str, lang.joiner_file()))?,
        tokens: CString::new(format!("{}/tokens.txt", dir_str))?,
        model_type: CString::new(lang.sherpa_model_type())?,
        bpe_vocab: CString::new(if bpe_path.exists() {
            bpe_path.to_string_lossy().into_owned()
        } else {
            String::new()
        })?,
        provider: CString::new("cpu")?,
        decoding_method: CString::new("greedy_search")?,
    };

    let mut config = ffi::SherpaOnnxOnlineRecognizerConfig::zeroed();
    config.feat_config.sample_rate = 16000;
    config.feat_config.feature_dim = 80;
    config.model_config.transducer.encoder = strings.encoder.as_ptr();
    config.model_config.transducer.decoder = strings.decoder.as_ptr();
    config.model_config.transducer.joiner = strings.joiner.as_ptr();
    config.model_config.tokens = strings.tokens.as_ptr();
    // Only set model_type when explicitly known; empty = auto-detect from ONNX metadata.
    if !lang.sherpa_model_type().is_empty() {
        config.model_config.model_type = strings.model_type.as_ptr();
    }
    if !strings.bpe_vocab.to_bytes().is_empty() {
        config.model_config.bpe_vocab = strings.bpe_vocab.as_ptr();
    }
    config.model_config.provider = strings.provider.as_ptr();
    config.model_config.num_threads = 2;
    config.decoding_method = strings.decoding_method.as_ptr();
    config.enable_endpoint = 0;

    Ok((config, strings))
}

pub fn run_sherpa_transcription(
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    state: SharedRealtimeState,
    session_id: u64,
) -> Result<()> {
    let locale = sherpa_locale();
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(super::state::TranscriptionMethod::SherpaZipformer);
    }

    // Download sherpa-onnx DLLs on first use
    if !dlls::is_sherpa_runtime_ready() {
        dlls::download_sherpa_dlls(stop_signal.clone(), overlay_hwnd)?;
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(());
        }
    }

    // Load DLL
    let lib = match ffi::load() {
        Ok(lib) => lib,
        Err(e) => {
            let msg = locale
                .tool_runtime
                .zipformer_requires_dlls_fmt
                .replace("{}", &e.to_string());
            crate::log_info!("[Sherpa] {}", msg);
            update_overlay_text(overlay_hwnd, &msg);
            std::thread::sleep(Duration::from_secs(5));
            update_overlay_text(overlay_hwnd, "");
            return Ok(());
        }
    };

    // Get language from config
    let lang_code = {
        let app = crate::APP.lock().unwrap();
        app.config.realtime_transcription_language.clone()
    };
    let lang = ZipformerLanguage::from_code(&lang_code);
    crate::log_info!(
        "[Sherpa] Language: {} ({})",
        lang.display_name(),
        lang.code()
    );

    // Download model if needed
    if !is_model_downloaded(lang) {
        update_overlay_text(
            overlay_hwnd,
            &locale
                .tool_runtime
                .zipformer_downloading_overlay_fmt
                .replace("{}", lang.display_name()),
        );
        download_model(lang, stop_signal.clone(), overlay_hwnd)?;
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(());
        }
    }

    update_overlay_text(
        overlay_hwnd,
        &locale
            .tool_runtime
            .zipformer_loading_overlay_fmt
            .replace("{}", lang.display_name()),
    );

    let (config, _strings) = build_recognizer_config(lang)?;
    crate::log_info!(
        "[Sherpa] Creating recognizer for {} ({})",
        lang.display_name(),
        lang.sherpa_model_type()
    );

    let recognizer = unsafe { (lib.create)(&config) };
    if recognizer.is_null() {
        return Err(anyhow!("Failed to create sherpa-onnx recognizer"));
    }

    let stream = unsafe { (lib.create_stream)(recognizer) };
    if stream.is_null() {
        unsafe { (lib.destroy)(recognizer) };
        return Err(anyhow!("Failed to create sherpa-onnx stream"));
    }

    update_overlay_text(overlay_hwnd, "");
    crate::log_info!(
        "[Sherpa] Zipformer {} loaded, streaming",
        lang.display_name()
    );

    let audio_buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let pause_signal = Arc::new(AtomicBool::new(false));
    let audio_stream =
        start_audio_capture(audio_buffer.clone(), stop_signal.clone(), pause_signal)?;

    let result = run_streaming_loop(SherpaStreamingLoop {
        lib,
        recognizer,
        stream,
        audio_buffer,
        stop_signal: &stop_signal,
        overlay_hwnd,
        state: &state,
        has_native_punctuation: lang.has_native_punctuation(),
        session_id,
    });

    drop(audio_stream);

    unsafe {
        (lib.destroy_stream)(stream);
        (lib.destroy)(recognizer);
    }
    crate::log_info!("[Sherpa] Session ended");

    result
}
