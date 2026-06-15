pub mod dlls;
pub mod ffi;
pub mod ffi_tts;
mod streaming;

use super::state::SharedRealtimeState;
use super::utils::update_overlay_text;
use crate::config::Preset;
use anyhow::{Result, anyhow};
use std::ffi::CString;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use windows::Win32::Foundation::HWND;

use self::streaming::{SherpaStreamingLoop, run_streaming_loop, start_audio_capture};

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
        matches!(
            self,
            Self::English | Self::Korean | Self::French | Self::German | Self::Spanish
        )
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
            Self::Russian => &[
                "encoder.onnx",
                "decoder.onnx",
                "joiner.onnx",
                "tokens.txt",
                "bpe.model",
            ],
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
            "all" => Self::English,
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
    lang.model_files()
        .iter()
        .all(|f| has_nonempty_file(&dir.join(f)))
}

fn has_nonempty_file(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
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
        if has_nonempty_file(&target) {
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
            .zipformer_downloading_title_fmt
            .replace("{}", lang.display_name());
        state.download_message = locale.zipformer_downloading_start.to_string();
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
    _preset: Preset,
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
            let msg = locale
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
                .zipformer_downloading_overlay_fmt
                .replace("{}", lang.display_name()),
        );
        download_model(lang, &stop_signal, overlay_hwnd)?;
        if stop_signal.load(Ordering::Relaxed) {
            return Ok(());
        }
    }

    update_overlay_text(
        overlay_hwnd,
        &locale
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

#[cfg(test)]
mod catalog_parity_tests {
    use super::ZipformerLanguage;
    use serde::Deserialize;

    const ALL: [ZipformerLanguage; 8] = [
        ZipformerLanguage::English,
        ZipformerLanguage::Korean,
        ZipformerLanguage::Chinese,
        ZipformerLanguage::French,
        ZipformerLanguage::German,
        ZipformerLanguage::Spanish,
        ZipformerLanguage::Russian,
        ZipformerLanguage::All8Lang,
    ];

    #[derive(Deserialize)]
    struct Catalog {
        languages: Vec<CatalogEntry>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CatalogEntry {
        code: String,
        model_name: String,
        download_base_url: String,
        has_native_punctuation: bool,
        model_files: Vec<String>,
    }

    /// The Windows-canonical Zipformer catalog must match the shared parity fixture
    /// asserted identically by the Android side. See .claude/parity/zipformer-catalog.md.
    #[test]
    fn windows_zipformer_catalog_matches_parity_fixture() {
        let catalog: Catalog = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/parity-fixtures/zipformer-catalog/catalog.json"
            ))
            .expect("fixture file"),
        )
        .expect("fixture json");

        assert_eq!(catalog.languages.len(), ALL.len());
        for lang in ALL {
            let entry = catalog
                .languages
                .iter()
                .find(|e| e.code == lang.code())
                .unwrap_or_else(|| panic!("no fixture entry for code {}", lang.code()));
            assert_eq!(lang.model_dir_name(), entry.model_name, "{}", lang.code());
            assert_eq!(
                lang.download_base_url(),
                entry.download_base_url,
                "{}",
                lang.code()
            );
            assert_eq!(
                lang.has_native_punctuation(),
                entry.has_native_punctuation,
                "{}",
                lang.code()
            );
            let expected_files: Vec<&str> = entry.model_files.iter().map(String::as_str).collect();
            assert_eq!(
                lang.model_files(),
                expected_files.as_slice(),
                "{}",
                lang.code()
            );
        }
    }
}
