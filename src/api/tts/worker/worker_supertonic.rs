//! Supertonic 3 offline TTS worker.

use std::ffi::CString;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use unicode_normalization::UnicodeNormalization;

use super::super::manager::TtsManager;
use super::super::types::AudioEvent;
use super::open_weights::{fail_request, stream_pcm_samples};
use crate::APP;
use crate::api::realtime_audio::sherpa_onnx::{dlls, ffi_tts};
use crate::api::realtime_audio::supertonic_assets::{
    download_supertonic_model, get_supertonic_model_dir, is_supertonic_model_downloaded,
};
use crate::config::tts_catalog::{
    default_supertonic_voice_for_lang, normalize_supertonic_lang, supertonic_speaker_id_for_voice,
};

const PROVIDER: &str = "Supertonic";
const INIT_TIMEOUT: Duration = Duration::from_secs(12);

static SUPERTONIC_INIT_TIMED_OUT: AtomicBool = AtomicBool::new(false);

static SUPERTONIC_HANDLE: LazyLock<Mutex<Option<SupertonicSession>>> =
    LazyLock::new(|| Mutex::new(None));

struct SupertonicSession {
    handle: *const ffi_tts::SherpaOnnxOfflineTts,
    destroy_fn: unsafe extern "C" fn(*const ffi_tts::SherpaOnnxOfflineTts),
    num_threads: i32,
}

unsafe impl Send for SupertonicSession {}

impl Drop for SupertonicSession {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.destroy_fn)(self.handle) };
            self.handle = std::ptr::null();
        }
    }
}

pub(super) fn handle_supertonic_tts(
    manager: Arc<TtsManager>,
    request: super::super::types::QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    let hwnd = request.req.hwnd;

    if !dlls::is_sherpa_runtime_ready() {
        let stop = Arc::new(AtomicBool::new(false));
        if let Err(err) = dlls::download_sherpa_dlls_with_progress(stop, |progress| {
            crate::overlay::auto_copy_badge::show_progress_notification(
                "Downloading sherpa-onnx runtime",
                "Required for Supertonic 3 offline TTS",
                progress * 100.0,
            );
        }) {
            crate::overlay::auto_copy_badge::hide_progress_notification();
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                format!("Failed to install sherpa-onnx DLLs for Supertonic 3: {err}"),
            );
            return;
        }
        crate::overlay::auto_copy_badge::hide_progress_notification();
    }

    if !is_supertonic_model_downloaded() {
        let stop = Arc::new(AtomicBool::new(false));
        if let Err(err) = download_supertonic_model(stop, true) {
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                format!("Failed to install Supertonic 3 model: {err}"),
            );
            return;
        }
    }

    let mut settings = if let Some(profile) = request.req.profile.as_ref() {
        profile.supertonic_settings.clone()
    } else {
        match APP.lock() {
            Ok(app) => app.config.supertonic_settings.clone(),
            Err(e) => {
                fail_request(
                    PROVIDER,
                    hwnd,
                    &tx,
                    format!("Failed to lock APP for Supertonic settings: {e:?}"),
                );
                return;
            }
        }
    };
    settings.num_threads = settings.num_threads.max(1);

    let text_for_detection = normalize_supertonic_text(&request.req.text);
    let explicit_language_override = request
        .req
        .profile
        .as_ref()
        .and_then(|profile| profile.language_code_override.as_deref())
        .map(str::to_string);
    let language_override =
        explicit_language_override.or_else(|| detect_supertonic_language(&text_for_detection));
    let lang = language_override
        .as_deref()
        .and_then(normalize_supertonic_lang)
        .or_else(|| normalize_supertonic_lang(&settings.lang))
        .unwrap_or_else(|| "en".to_string());
    let text_for_tts = normalize_supertonic_text_for_lang(&request.req.text, &lang);
    let voice_id = resolve_supertonic_voice_for_language(&settings, &lang);
    let speaker_id = supertonic_speaker_id_for_voice(&voice_id);

    if SUPERTONIC_INIT_TIMED_OUT.load(Ordering::SeqCst) {
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            "Supertonic initialisation previously timed out while loading sherpa-onnx. Restart the app after reinstalling sherpa-onnx from Settings > Downloaded Tools.",
        );
        return;
    }

    let needs_session = match SUPERTONIC_HANDLE.lock() {
        Ok(g) => g
            .as_ref()
            .is_none_or(|session| session.num_threads != settings.num_threads),
        Err(e) => {
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                format!("Supertonic session lock poisoned: {e:?}"),
            );
            return;
        }
    };

    if needs_session {
        match build_session_with_timeout(settings.clone()) {
            Ok(session) => match SUPERTONIC_HANDLE.lock() {
                Ok(mut guard) => {
                    if guard
                        .as_ref()
                        .is_none_or(|existing| existing.num_threads != settings.num_threads)
                    {
                        *guard = Some(session);
                    }
                }
                Err(e) => {
                    fail_request(
                        PROVIDER,
                        hwnd,
                        &tx,
                        format!("Supertonic session lock poisoned: {e:?}"),
                    );
                    return;
                }
            },
            Err(e) => {
                fail_request(
                    PROVIDER,
                    hwnd,
                    &tx,
                    format!("Failed to initialise Supertonic 3: {e}"),
                );
                return;
            }
        }
    }

    let session_lock = match SUPERTONIC_HANDLE.lock() {
        Ok(g) => g,
        Err(e) => {
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                format!("Supertonic session lock poisoned: {e:?}"),
            );
            return;
        }
    };
    let Some(session) = session_lock.as_ref() else {
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            "Supertonic session was not initialised.",
        );
        return;
    };

    let text_cstr = match CString::new(text_for_tts.as_str()) {
        Ok(s) => s,
        Err(_) => {
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                "Text contains an embedded NUL byte; Supertonic cannot process it.",
            );
            return;
        }
    };

    let extra_json = serde_json::json!({
        "lang": lang,
        "speed": settings.speed.clamp(0.5, 2.0),
        "num_steps": settings.num_steps.clamp(1, 20),
        "silence_duration": settings.silence_duration.clamp(0.0, 2.0),
        "seed": settings.seed,
    })
    .to_string();
    let extra_cstr = match CString::new(extra_json) {
        Ok(s) => s,
        Err(_) => {
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                "Invalid Supertonic generation options.",
            );
            return;
        }
    };

    let lib = match ffi_tts::load() {
        Ok(l) => l,
        Err(e) => {
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                format!("Failed to load sherpa-onnx OfflineTts: {e}"),
            );
            return;
        }
    };

    let mut gen_config = ffi_tts::SherpaOnnxGenerationConfig::zeroed();
    gen_config.silence_scale = 1.0;
    gen_config.speed = settings.speed.clamp(0.5, 2.0);
    gen_config.sid = speaker_id;
    gen_config.num_steps = settings.num_steps.clamp(1, 20);
    gen_config.extra = extra_cstr.as_ptr();

    eprintln!(
        "[TTS {PROVIDER}] synth voice={} speaker={} lang={} speed={} steps={} chars={}",
        voice_id,
        gen_config.sid,
        lang,
        gen_config.speed,
        gen_config.num_steps,
        text_for_tts.chars().count()
    );

    let generated_ptr = unsafe {
        (lib.generate_with_config)(
            session.handle,
            text_cstr.as_ptr(),
            &gen_config,
            None,
            std::ptr::null_mut(),
        )
    };
    if generated_ptr.is_null() {
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            "Supertonic synthesis returned no audio. Check language and reinstall the model if it persists.",
        );
        return;
    }

    let (samples_i16, sample_rate) = unsafe {
        let g = &*generated_ptr;
        let n = g.n.max(0) as usize;
        let mut samples = Vec::with_capacity(n);
        if n > 0 && !g.samples.is_null() {
            let slice = std::slice::from_raw_parts(g.samples, n);
            for &s in slice {
                let clamped = s.clamp(-1.0, 1.0);
                samples.push((clamped * i16::MAX as f32) as i16);
            }
        }
        let rate = g.sample_rate.max(0) as u32;
        (lib.destroy_generated)(generated_ptr);
        (samples, rate)
    };

    stream_pcm_samples(&manager, &request, &tx, samples_i16, sample_rate);
}

fn resolve_supertonic_voice_for_language(
    settings: &crate::config::SupertonicSettings,
    lang: &str,
) -> String {
    settings
        .voice_configs
        .iter()
        .find(|config| normalize_supertonic_lang(&config.language_code).as_deref() == Some(lang))
        .map(|config| config.voice_id.clone())
        .unwrap_or_else(|| default_supertonic_voice_for_lang(lang).to_string())
}

fn detect_supertonic_language(text: &str) -> Option<String> {
    if contains_vietnamese_marker(text) {
        return Some("vi".to_string());
    }
    crate::lang_detect::detect_language(text)
}

fn normalize_supertonic_text(text: &str) -> String {
    clean_supertonic_text(text.nfc())
}

fn clean_supertonic_text(chars: impl Iterator<Item = char>) -> String {
    chars
        .map(|ch| if ch == '\u{00a0}' { ' ' } else { ch })
        .filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control())
        .collect()
}

fn normalize_supertonic_text_for_lang(text: &str, lang: &str) -> String {
    let text = strip_unverified_supertonic_tags(text);
    let text = collapse_supertonic_input_whitespace(&text);
    let text = if lang == "vi" && should_lowercase_supertonic_vietnamese(&text) {
        text.to_lowercase()
    } else {
        text
    };
    if lang == "vi" {
        clean_supertonic_text(text.nfd())
    } else {
        normalize_supertonic_text(&text)
    }
}

fn collapse_supertonic_input_whitespace(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if matches!(ch, '♪' | '♫' | '♩' | '♬' | '♭' | '♮' | '♯') {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn should_lowercase_supertonic_vietnamese(text: &str) -> bool {
    if !contains_vietnamese_marker(text) {
        return false;
    }

    let mut uppercase = 0usize;
    let mut lowercase = 0usize;
    for ch in text.chars().filter(|ch| ch.is_alphabetic()) {
        if ch.is_uppercase() {
            uppercase += 1;
        } else if ch.is_lowercase() {
            lowercase += 1;
        }
    }

    uppercase >= 3 && uppercase >= lowercase.saturating_mul(2).max(1)
}

fn strip_unverified_supertonic_tags(text: &str) -> String {
    const UNSAFE_TAGS: &[&str] = &[
        "<laugh>",
        "<breath>",
        "<sigh>",
        "<scream>",
        "<whisper>",
        "<gasp>",
        "<cough>",
        "<chuckle>",
        "<giggle>",
        "<yawn>",
    ];
    let mut cleaned = text.to_string();
    for tag in UNSAFE_TAGS {
        cleaned = cleaned.replace(tag, " ");
    }
    cleaned
}

fn contains_vietnamese_marker(text: &str) -> bool {
    const VIETNAMESE_MARKERS: &str = concat!(
        "ĂăÂâĐđÊêÔôƠơƯư",
        "ÀÁẢÃẠẦẤẨẪẬẰẮẲẴẶÈÉẺẼẸỀẾỂỄỆÌÍỈĨỊ",
        "ÒÓỎÕỌỒỐỔỖỘỜỚỞỠỢÙÚỦŨỤỪỨỬỮỰỲÝỶỸỴ",
        "àáảãạầấẩẫậằắẳẵặèéẻẽẹềếểễệìíỉĩị",
        "òóỏõọồốổỗộờớởỡợùúủũụừứửữựỳýỷỹỵ"
    );
    text.chars().any(|ch| VIETNAMESE_MARKERS.contains(ch))
}

fn build_session_with_timeout(
    settings: crate::config::SupertonicSettings,
) -> anyhow::Result<SupertonicSession> {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("supertonic-tts-init".to_string())
        .spawn(move || {
            let _ = tx.send(build_session(&settings));
        })?;

    match rx.recv_timeout(INIT_TIMEOUT) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            SUPERTONIC_INIT_TIMED_OUT.store(true, Ordering::SeqCst);
            Err(anyhow::anyhow!(
                "timed out after {}s while loading sherpa-onnx OfflineTts",
                INIT_TIMEOUT.as_secs()
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(anyhow::anyhow!(
            "Supertonic initialisation worker exited unexpectedly"
        )),
    }
}

fn build_session(
    settings: &crate::config::SupertonicSettings,
) -> anyhow::Result<SupertonicSession> {
    let lib = ffi_tts::load()?;
    let model_dir = get_supertonic_model_dir();
    let to_cstring =
        |p: &Path| -> anyhow::Result<CString> { Ok(CString::new(p.to_string_lossy().as_bytes())?) };
    let to_cstring_str = |s: &str| -> anyhow::Result<CString> { Ok(CString::new(s)?) };

    let duration_predictor = to_cstring(&model_dir.join("duration_predictor.int8.onnx"))?;
    let text_encoder = to_cstring(&model_dir.join("text_encoder.int8.onnx"))?;
    let vector_estimator = to_cstring(&model_dir.join("vector_estimator.int8.onnx"))?;
    let vocoder = to_cstring(&model_dir.join("vocoder.int8.onnx"))?;
    let tts_json = to_cstring(&model_dir.join("tts.json"))?;
    let unicode_indexer = to_cstring(&model_dir.join("unicode_indexer.bin"))?;
    let voice_style = to_cstring(&model_dir.join("voice.bin"))?;
    let provider = to_cstring_str("cpu")?;
    let empty = to_cstring_str("")?;

    let mut config = ffi_tts::SherpaOnnxOfflineTtsConfig::zeroed();
    config.model.supertonic.duration_predictor = duration_predictor.as_ptr();
    config.model.supertonic.text_encoder = text_encoder.as_ptr();
    config.model.supertonic.vector_estimator = vector_estimator.as_ptr();
    config.model.supertonic.vocoder = vocoder.as_ptr();
    config.model.supertonic.tts_json = tts_json.as_ptr();
    config.model.supertonic.unicode_indexer = unicode_indexer.as_ptr();
    config.model.supertonic.voice_style = voice_style.as_ptr();
    config.model.num_threads = settings.num_threads.max(1);
    config.model.debug = 0;
    config.model.provider = provider.as_ptr();
    config.rule_fsts = empty.as_ptr();
    config.rule_fars = empty.as_ptr();
    config.max_num_sentences = 1;
    config.silence_scale = 1.0;

    let handle = unsafe { (lib.create_tts)(&config) };
    if handle.is_null() {
        return Err(anyhow::anyhow!(
            "SherpaOnnxCreateOfflineTts returned null for Supertonic 3"
        ));
    }

    Ok(SupertonicSession {
        handle,
        destroy_fn: lib.destroy_tts,
        num_threads: settings.num_threads.max(1),
    })
}

#[cfg(test)]
mod tests {
    use super::{contains_vietnamese_marker, normalize_supertonic_text};

    #[test]
    fn normalizes_vietnamese_to_nfc() {
        let decomposed = "tie\u{302}\u{301}ng Vie\u{323}\u{302}t";
        assert_eq!(normalize_supertonic_text(decomposed), "tiếng Việt");
    }

    #[test]
    fn keeps_vietnamese_decomposed_for_supertonic() {
        let normalized = super::normalize_supertonic_text_for_lang("ọ ỗ ẫ", "vi");
        assert!(normalized.contains("o\u{323}"));
        assert!(normalized.contains("o\u{302}\u{303}"));
        assert!(normalized.contains("a\u{302}\u{303}"));
    }

    #[test]
    fn normalizes_supertonic_vietnamese_lyric_input() {
        let normalized = super::normalize_supertonic_text_for_lang(
            "♪ TÔI, TÔI VÀ LOUIE, CHÚNG TÔI\nSẼ CHẠY ĐẾN BÊN ♪",
            "vi",
        );
        assert!(normalized.contains("to\u{302}i, to\u{302}i va\u{300} louie"));
        assert!(!normalized.contains('\n'));
        assert!(!normalized.contains('♪'));
    }

    #[test]
    fn strips_supertonic_tags_that_are_spoken_by_local_runtime() {
        let normalized = super::normalize_supertonic_text_for_lang(
            "what <breath> <laugh> <sigh> <cough> poison",
            "en",
        );
        assert_eq!(normalized, "what poison");
    }

    #[test]
    fn detects_vietnamese_diacritics() {
        assert!(contains_vietnamese_marker("Xin chào tiếng Việt"));
        assert!(!contains_vietnamese_marker("Plain English text"));
    }
}
