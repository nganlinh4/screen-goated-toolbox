pub mod dlls;
pub mod ffi;

use super::capture::{start_device_loopback_capture, start_mic_capture, start_per_app_capture};
use super::state::SharedRealtimeState;
use super::utils::update_overlay_text;
use super::{REALTIME_RMS, WM_VOLUME_UPDATE};
use crate::config::Preset;
use anyhow::{Result, anyhow};
use std::ffi::CString;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW};

const BATCH_SAMPLES: usize = 16000 / 2; // 500ms worth at 16kHz

/// Zipformer language variant for Windows.
/// Mirrors `ZipformerLanguage` from Android.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZipformerLanguage {
    English,
    Korean,
    Chinese,
    French,
    German,
    Spanish,
    Russian,
    All8Lang,
}

impl ZipformerLanguage {
    pub fn code(&self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Korean => "ko",
            Self::Chinese => "zh",
            Self::French => "fr",
            Self::German => "de",
            Self::Spanish => "es",
            Self::Russian => "ru",
            Self::All8Lang => "all-8",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Korean => "Korean",
            Self::Chinese => "Chinese",
            Self::French => "French",
            Self::German => "German",
            Self::Spanish => "Spanish",
            Self::Russian => "Russian",
            Self::All8Lang => "AR,EN,ID,JA,RU,TH,VI,ZH",
        }
    }

    pub fn model_dir_name(&self) -> &'static str {
        match self {
            Self::English => "streaming-zipformer-en-kroko",
            Self::Korean => "streaming-zipformer-korean",
            Self::Chinese => "streaming-zipformer-zh",
            Self::French => "streaming-zipformer-fr-kroko",
            Self::German => "streaming-zipformer-de-kroko",
            Self::Spanish => "streaming-zipformer-es-kroko",
            Self::Russian => "streaming-zipformer-small-ru-vosk",
            Self::All8Lang => "streaming-zipformer-multilingual-8lang",
        }
    }

    pub fn download_base_url(&self) -> &'static str {
        match self {
            Self::English => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06/resolve/main"
            }
            Self::Korean => {
                "https://modelscope.cn/models/k2-fsa/sherpa-onnx-streaming-zipformer-korean-2024-06-16/resolve/master"
            }
            Self::Chinese => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-zh-int8-2025-06-30/resolve/main"
            }
            Self::French => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-fr-kroko-2025-08-06/resolve/main"
            }
            Self::German => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-de-kroko-2025-08-06/resolve/main"
            }
            Self::Spanish => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-es-kroko-2025-08-06/resolve/main"
            }
            Self::Russian => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-small-ru-vosk-2025-08-16/resolve/main"
            }
            Self::All8Lang => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10/resolve/main"
            }
        }
    }

    /// True = model emits native punctuation in streaming output → Cases 1 & 2 apply.
    /// False = no native punctuation → Case 3 silence-based commit with appended period.
    ///
    /// Confirmed by live testing on all 8 languages (2026-04):
    ///   EN ✓  KO ✓  FR ✓  DE ✓  ES ✓  — native punctuation
    ///   ZH ✗  RU ✗  All8 ✗            — no native punctuation
    pub fn has_native_punctuation(&self) -> bool {
        matches!(self, Self::English | Self::Korean | Self::French | Self::German | Self::Spanish)
    }

    /// sherpa-onnx model type hint. Empty = auto-detect from ONNX metadata (safest).
    /// Only set explicitly for models whose embedded metadata is confirmed.
    pub fn sherpa_model_type(&self) -> &'static str {
        match self {
            // Kroko models are confirmed zipformer2
            Self::English | Self::French | Self::German | Self::Spanish => "zipformer2",
            // All others: auto-detect from ONNX file metadata
            _ => "",
        }
    }

    pub fn model_files(&self) -> &'static [&'static str] {
        match self {
            Self::English | Self::French | Self::German | Self::Spanish => {
                &["encoder.onnx", "decoder.onnx", "joiner.onnx", "tokens.txt"]
            }
            Self::Korean => &[
                "encoder-epoch-99-avg-1.int8.onnx",
                "decoder-epoch-99-avg-1.onnx",
                "joiner-epoch-99-avg-1.int8.onnx",
                "tokens.txt",
                "bpe.model",
            ],
            Self::Chinese => &[
                "encoder.int8.onnx",
                "decoder.onnx",
                "joiner.int8.onnx",
                "tokens.txt",
            ],
            Self::Russian => &["encoder.onnx", "decoder.onnx", "joiner.onnx", "tokens.txt", "bpe.model"],
            Self::All8Lang => &[
                "encoder-epoch-75-avg-11-chunk-16-left-128.int8.onnx",
                "decoder-epoch-75-avg-11-chunk-16-left-128.onnx",
                "joiner-epoch-75-avg-11-chunk-16-left-128.int8.onnx",
                "tokens.txt",
                "bpe.model",
            ],
        }
    }

    fn encoder_file(&self) -> &'static str {
        self.model_files()
            .iter()
            .find(|f| f.contains("encoder"))
            .unwrap()
    }

    fn decoder_file(&self) -> &'static str {
        self.model_files()
            .iter()
            .find(|f| f.contains("decoder"))
            .unwrap()
    }

    fn joiner_file(&self) -> &'static str {
        self.model_files()
            .iter()
            .find(|f| f.contains("joiner"))
            .unwrap()
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "en" => Self::English,
            "ko" => Self::Korean,
            "zh" => Self::Chinese,
            "fr" => Self::French,
            "de" => Self::German,
            "es" => Self::Spanish,
            "ru" => Self::Russian,
            "all-8" => Self::All8Lang,
            _ => Self::English,
        }
    }
}

fn model_dir(lang: ZipformerLanguage) -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("models")
        .join(lang.model_dir_name())
}

pub fn is_model_downloaded(lang: ZipformerLanguage) -> bool {
    let dir = model_dir(lang);
    lang.model_files().iter().all(|f| dir.join(f).exists())
}

/// Downloads all files for `lang`.
/// `on_progress(p)` is called continuously with p in 0.0..=1.0 (byte-level within each file).
/// Returns Ok(()) on success (already-downloaded files are skipped).
pub fn download_model_with_progress(
    lang: ZipformerLanguage,
    stop_signal: &AtomicBool,
    on_progress: impl Fn(f32),
) -> Result<()> {
    let dir = model_dir(lang);
    std::fs::create_dir_all(&dir)?;

    let base_url = lang.download_base_url();
    let files = lang.model_files();
    let total = files.len() as f32;

    on_progress(0.0);

    for (i, filename) in files.iter().enumerate() {
        if stop_signal.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("Download cancelled"));
        }
        let target = dir.join(filename);
        if target.exists() {
            on_progress((i + 1) as f32 / total);
            continue;
        }
        let file_start = i as f32 / total;
        let file_end = (i + 1) as f32 / total;
        let url = format!("{base_url}/{filename}");
        crate::log_info!("[Sherpa] Downloading {filename} from {url}");
        crate::api::realtime_audio::model_loader::download_file_with_progress(
            &url,
            &target,
            stop_signal,
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
    stop_signal: &AtomicBool,
    overlay_hwnd: HWND,
) -> Result<()> {
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
        state.download_title = format!("Downloading Zipformer {}", lang.display_name());
        state.download_message = "Starting download...".to_string();
        state.download_progress = 0.0;
    }
    post_download_state();

    let result = download_model_with_progress(lang, stop_signal, |pct| {
        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.download_progress = pct * 100.0;
        }
        post_download_state();
        update_overlay_text(overlay_hwnd, &format!("Downloading Zipformer {}...", lang.display_name()));
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
    _preset: Preset,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    state: SharedRealtimeState,
) -> Result<()> {
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(super::state::TranscriptionMethod::SherpaZipformer);
    }

    // Download sherpa-onnx DLLs on first use
    if !dlls::is_sherpa_dlls_installed() {
        dlls::download_sherpa_dlls(stop_signal.clone(), overlay_hwnd)?;
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(());
        }
    }

    // Load DLL
    let lib = match ffi::load() {
        Ok(lib) => lib,
        Err(e) => {
            let msg = format!("Zipformer requires sherpa-onnx DLLs: {e}");
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
            &format!("Downloading Zipformer {}...", lang.display_name()),
        );
        download_model(lang, &stop_signal, overlay_hwnd)?;
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(());
        }
    }

    update_overlay_text(
        overlay_hwnd,
        &format!("Loading Zipformer {}...", lang.display_name()),
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
    let _audio_stream =
        start_audio_capture(audio_buffer.clone(), stop_signal.clone(), pause_signal)?;

    let result = run_streaming_loop(
        lib,
        recognizer,
        stream,
        audio_buffer,
        &stop_signal,
        overlay_hwnd,
        &state,
        lang.has_native_punctuation(),
    );

    unsafe {
        (lib.destroy_stream)(stream);
        (lib.destroy)(recognizer);
    }
    crate::log_info!("[Sherpa] Session ended");

    result
}

fn run_streaming_loop(
    lib: &ffi::SherpaLib,
    recognizer: *const ffi::SherpaOnnxOnlineRecognizer,
    stream: *const ffi::SherpaOnnxOnlineStream,
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: &AtomicBool,
    overlay_hwnd: HWND,
    state: &SharedRealtimeState,
    has_native_punctuation: bool,
) -> Result<()> {
    let mut committed_history = String::new();
    // Portion of current stream output already committed — advance but never reset mid-speech
    let mut stream_committed_prefix = String::new();
    let mut last_draft_text = String::new();
    let mut last_draft_change = Instant::now();
    let mut pending_f32: Vec<f32> = Vec::new();

    while !stop_signal.load(Ordering::Relaxed) {
        if !overlay_hwnd.is_invalid() && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            break;
        }
        if super::DEVICE_RECONNECT_REQUESTED.load(Ordering::SeqCst) {
            break;
        }
        if crate::overlay::realtime_webview::AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
            || crate::overlay::realtime_webview::TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
        {
            break;
        }

        let new_samples: Vec<i16> = {
            let mut buf = audio_buffer.lock().unwrap();
            if buf.is_empty() {
                Vec::new()
            } else {
                buf.drain(..).collect()
            }
        };

        if !new_samples.is_empty() {
            let rms = compute_rms(&new_samples);
            REALTIME_RMS.store(rms.to_bits(), Ordering::Relaxed);
            crate::overlay::recording::update_audio_viz(rms);
            if rms > 0.001 {
                crate::overlay::recording::AUDIO_WARMUP_COMPLETE.store(true, Ordering::SeqCst);
            }
            if !overlay_hwnd.is_invalid() {
                unsafe {
                    let _ =
                        PostMessageW(Some(overlay_hwnd), WM_VOLUME_UPDATE, WPARAM(0), LPARAM(0));
                }
            }
            for &s in &new_samples {
                pending_f32.push(s as f32 / 32768.0);
            }
        }

        if pending_f32.len() >= BATCH_SAMPLES {
            let max_buffer = 16000 * 10;
            if pending_f32.len() > max_buffer {
                let drop = pending_f32.len() - max_buffer;
                pending_f32.drain(..drop);
            }

            let batch: Vec<f32> = pending_f32.drain(..).collect();
            unsafe {
                (lib.accept_waveform)(stream, 16000, batch.as_ptr(), batch.len() as i32);
            }
            while unsafe { (lib.is_ready)(recognizer, stream) } != 0 {
                unsafe { (lib.decode)(recognizer, stream) };
            }

            let result_ptr = unsafe { (lib.get_result_json)(recognizer, stream) };
            if !result_ptr.is_null() {
                let result_cstr = unsafe { std::ffi::CStr::from_ptr(result_ptr) };
                let result_str = result_cstr.to_string_lossy();
                let text = parse_result_text(&result_str);
                unsafe { (lib.destroy_result_json)(result_ptr) };

                // Draft = everything the stream has output after our committed prefix
                let draft = if text.starts_with(&stream_committed_prefix) {
                    text[stream_committed_prefix.len()..].trim_start().to_string()
                } else {
                    text.clone()
                };

                // Track draft staleness for period-terminated sentences
                if draft != last_draft_text {
                    last_draft_text = draft.clone();
                    last_draft_change = Instant::now();
                }

                // Case 1 & 2: only for models that emit native punctuation
                // Case 1: period followed by next word → split immediately
                if has_native_punctuation
                    && let Some((before, after)) = super::utils::split_at_sentence_boundary(&draft)
                {
                    super::utils::append_history_segment(&mut committed_history, &before);
                    stream_committed_prefix = text[..text.len() - after.len()].trim_end().to_string();
                    last_draft_text.clear();
                    last_draft_change = Instant::now();
                    publish_transcript(state, overlay_hwnd, &committed_history, after.trim_start());
                // Case 2: draft ends with period, stable 600ms → done speaking sentence
                } else if has_native_punctuation
                    && draft.trim_end().ends_with(['.', '?', '!'])
                    && last_draft_change.elapsed().as_millis() >= 600
                {
                    super::utils::append_history_segment(&mut committed_history, &draft);
                    stream_committed_prefix = text.trim_end().to_string();
                    last_draft_text.clear();
                    last_draft_change = Instant::now();
                    publish_transcript(state, overlay_hwnd, &committed_history, "");
                // Case 3: fallback only for models without native punctuation.
                // Native-punctuation models (EN/KO/ZH/FR/DE/ES) rely solely on Cases 1 & 2 —
                // running Case 3 on them causes mid-word commits between audio batch gaps.
                } else if !has_native_punctuation {
                    let silence_ms = last_draft_change.elapsed().as_millis() as u64;
                    if let Some(committed) = super::state::check_draft_commit(&draft, silence_ms) {
                        let committed = format!("{}.", committed);
                        super::utils::append_history_segment(&mut committed_history, &committed);
                        stream_committed_prefix = text.trim_end().to_string();
                        last_draft_text.clear();
                        last_draft_change = Instant::now();
                        publish_transcript(state, overlay_hwnd, &committed_history, "");
                    } else {
                        publish_transcript(state, overlay_hwnd, &committed_history, &draft);
                    }
                } else {
                    publish_transcript(state, overlay_hwnd, &committed_history, &draft);
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    Ok(())
}


/// Parse text from sherpa-onnx result JSON: {"text": "hello world", ...}
fn parse_result_text(json_str: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
        v.get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    } else {
        String::new()
    }
}

fn publish_transcript(
    state: &SharedRealtimeState,
    overlay_hwnd: HWND,
    committed: &str,
    draft: &str,
) {
    crate::log_info!("[Sherpa] WHITE={:?} GRAY={:?}", committed, draft);
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(super::state::TranscriptionMethod::SherpaZipformer);
        s.set_transcript_segments(committed, draft);
        let display = s.display_transcript.clone();
        update_overlay_text(overlay_hwnd, &display);
    }
}

fn start_audio_capture(
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    pause_signal: Arc<AtomicBool>,
) -> Result<Option<cpal::Stream>> {
    let audio_source = {
        let app = crate::APP.lock().unwrap();
        app.config.realtime_audio_source.clone()
    };

    use crate::overlay::realtime_webview::{REALTIME_TTS_ENABLED, SELECTED_APP_PID};
    let tts_enabled = REALTIME_TTS_ENABLED.load(Ordering::SeqCst);
    let selected_pid = SELECTED_APP_PID.load(Ordering::SeqCst);
    let using_per_app = audio_source == "device" && tts_enabled && selected_pid > 0;

    if using_per_app {
        start_per_app_capture(selected_pid, audio_buffer, stop_signal, pause_signal)?;
        Ok(None)
    } else if audio_source == "mic" {
        Ok(Some(start_mic_capture(
            audio_buffer,
            stop_signal,
            pause_signal,
        )?))
    } else if audio_source == "device" && tts_enabled && selected_pid == 0 {
        Ok(None)
    } else {
        Ok(Some(start_device_loopback_capture(
            audio_buffer,
            stop_signal,
            pause_signal,
        )?))
    }
}

fn compute_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64 / 32768.0).powi(2)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}
