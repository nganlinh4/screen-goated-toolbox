//! Kokoro 82M v1.0 — offline TTS worker.
//!
//! Synthesis path:
//!   1. Ensure sherpa-onnx DLLs are present (download on first use).
//!   2. Ensure the Kokoro model bundle is on disk (the downloaded_tools UI
//!      surfaces a manual install button; this worker also kicks the download
//!      automatically if a request arrives with files missing, but only if a
//!      manual install isn't already in progress).
//!   3. Load sherpa-onnx OfflineTts with the Kokoro config (cached on first
//!      successful load — model loading takes ~2s).
//!   4. Look up the voice id → speaker index from the bundle's voices.bin
//!      manifest (`voices.txt` shipped alongside) — the sherpa-onnx C API
//!      uses a numeric `sid`, while Kokoro voices are named.
//!   5. Synthesise. sherpa-onnx returns float32 samples at the model's native
//!      24 kHz rate, which matches what [`crate::api::tts::player`] expects.
//!   6. Convert f32 → i16 LE and stream through the AudioEvent channel.

use std::ffi::CString;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use super::super::manager::TtsManager;
use super::super::types::AudioEvent;
use super::open_weights::{fail_request, stream_pcm_samples};
use crate::APP;
use crate::api::realtime_audio::kokoro_assets::{
    download_kokoro_model, get_kokoro_espeak_data_dir, get_kokoro_lexicon_paths,
    get_kokoro_model_dir, get_kokoro_rule_fst_paths, is_kokoro_model_downloaded,
};
use crate::api::realtime_audio::sherpa_onnx::{dlls, ffi_tts};
use crate::config::tts_catalog::{
    KOKORO_VOICES, default_kokoro_voice_for_lang, normalize_kokoro_lang, resolve_kokoro_lang,
};

const PROVIDER: &str = "Kokoro";

const INIT_TIMEOUT: Duration = Duration::from_secs(12);

static KOKORO_INIT_TIMED_OUT: AtomicBool = AtomicBool::new(false);

// Cached OfflineTts handle. Loading the model takes ~1.5–3s the first time
// (ONNX session warm-up, voices.bin parse) so we hold the handle for the
// lifetime of the process and reuse it across requests. Wrapped in a Mutex
// because the underlying C API is not thread-safe for concurrent generate
// calls — and the TTS worker thread is single-threaded anyway.
static KOKORO_HANDLE: LazyLock<Mutex<Option<KokoroSession>>> = LazyLock::new(|| Mutex::new(None));

/// Wraps the raw OfflineTts pointer so we can keep a Drop impl that
/// frees it through the FFI when the static is torn down on shutdown.
struct KokoroSession {
    handle: *const ffi_tts::SherpaOnnxOfflineTts,
    destroy_fn: unsafe extern "C" fn(*const ffi_tts::SherpaOnnxOfflineTts),
    lang: String,
    num_threads: i32,
}

unsafe impl Send for KokoroSession {}

impl Drop for KokoroSession {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.destroy_fn)(self.handle) };
            self.handle = std::ptr::null();
        }
    }
}

pub(super) fn handle_kokoro_tts(
    manager: Arc<TtsManager>,
    request: super::super::types::QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    let hwnd = request.req.hwnd;

    if !dlls::is_sherpa_dlls_installed() {
        let stop = Arc::new(AtomicBool::new(false));
        if let Err(err) = dlls::download_sherpa_dlls_with_progress(stop, |progress| {
            crate::overlay::auto_copy_badge::show_progress_notification(
                "Downloading sherpa-onnx runtime",
                "Required for Kokoro offline TTS",
                progress * 100.0,
            );
        }) {
            crate::overlay::auto_copy_badge::hide_progress_notification();
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                format!("Failed to install sherpa-onnx DLLs for Kokoro: {err}"),
            );
            return;
        }
        crate::overlay::auto_copy_badge::hide_progress_notification();
    }

    if !is_kokoro_model_downloaded() {
        let stop = Arc::new(AtomicBool::new(false));
        if let Err(err) = download_kokoro_model(stop, true) {
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                format!("Failed to install Kokoro model: {err}"),
            );
            return;
        }
    }

    let mut settings = if let Some(profile) = request.req.profile.as_ref() {
        profile.kokoro_settings.clone()
    } else {
        match APP.lock() {
            Ok(app) => app.config.kokoro_settings.clone(),
            Err(e) => {
                fail_request(
                    PROVIDER,
                    hwnd,
                    &tx,
                    format!("Failed to lock APP for Kokoro settings: {e:?}"),
                );
                return;
            }
        }
    };

    let language_override = request
        .req
        .profile
        .as_ref()
        .and_then(|profile| profile.language_code_override.as_deref())
        .map(str::to_string)
        .or_else(|| crate::lang_detect::detect_language(&request.req.text));
    let voice_name = resolve_kokoro_voice_for_language(&settings, language_override.as_deref());
    settings.voice = voice_name.clone();
    settings.lang = resolve_kokoro_lang(
        &settings.lang,
        language_override.as_deref(),
        &settings.voice,
    );
    settings.num_threads = settings.num_threads.max(1);

    let sid = match resolve_voice_sid(&voice_name) {
        Ok(idx) => idx,
        Err(e) => {
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                format!("Voice '{voice_name}' not found in Kokoro bundle: {e}"),
            );
            return;
        }
    };

    if KOKORO_INIT_TIMED_OUT.load(Ordering::SeqCst) {
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            "Kokoro initialisation previously timed out while loading sherpa-onnx. Restart the app after reinstalling sherpa-onnx from Settings → Downloaded Tools.",
        );
        return;
    }

    let needs_session = match KOKORO_HANDLE.lock() {
        Ok(g) => g.as_ref().is_none_or(|session| {
            session.lang != settings.lang || session.num_threads != settings.num_threads
        }),
        Err(e) => {
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                format!("Kokoro session lock poisoned: {e:?}"),
            );
            return;
        }
    };

    if needs_session {
        match build_session_with_timeout(settings.clone()) {
            Ok(session) => match KOKORO_HANDLE.lock() {
                Ok(mut guard) => {
                    if guard.as_ref().is_none_or(|existing| {
                        existing.lang != settings.lang
                            || existing.num_threads != settings.num_threads
                    }) {
                        *guard = Some(session);
                    }
                }
                Err(e) => {
                    fail_request(
                        PROVIDER,
                        hwnd,
                        &tx,
                        format!("Kokoro session lock poisoned: {e:?}"),
                    );
                    return;
                }
            },
            Err(e) => {
                fail_request(
                    PROVIDER,
                    hwnd,
                    &tx,
                    format!("Failed to initialise Kokoro: {e}"),
                );
                return;
            }
        }
    }

    let session_lock = match KOKORO_HANDLE.lock() {
        Ok(g) => g,
        Err(e) => {
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                format!("Kokoro session lock poisoned: {e:?}"),
            );
            return;
        }
    };
    let Some(session) = session_lock.as_ref() else {
        fail_request(PROVIDER, hwnd, &tx, "Kokoro session was not initialised.");
        return;
    };

    // sherpa-onnx clamps `length_scale` internally; we forward the user's
    // speed as a multiplier in [0.5, 2.0]. length_scale is inverse of speed.
    let speed = settings.speed.clamp(0.5, 2.0);

    let text_cstr = match CString::new(request.req.text.as_str()) {
        Ok(s) => s,
        Err(_) => {
            fail_request(
                PROVIDER,
                hwnd,
                &tx,
                "Text contains an embedded NUL byte; Kokoro cannot process it.",
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

    eprintln!(
        "[TTS {PROVIDER}] synth voice={} sid={} speed={} chars={}",
        voice_name,
        sid,
        speed,
        request.req.text.chars().count()
    );

    let generated_ptr =
        unsafe { (lib.generate)(session.handle, text_cstr.as_ptr(), sid, speed as f32) };
    if generated_ptr.is_null() {
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            "Kokoro synthesis returned a null audio buffer.",
        );
        return;
    }

    // Read out the f32 sample buffer, convert to PCM16, then free.
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

fn resolve_kokoro_voice_for_language(
    settings: &crate::config::KokoroSettings,
    detected_language_code: Option<&str>,
) -> String {
    let Some(target_lang) = detected_language_code.and_then(normalize_kokoro_lang) else {
        return if settings.voice.trim().is_empty() {
            default_kokoro_voice_for_lang("").to_string()
        } else {
            settings.voice.clone()
        };
    };
    settings
        .voice_configs
        .iter()
        .find(|config| {
            normalize_kokoro_lang(&config.language_code).as_deref() == Some(&target_lang)
        })
        .map(|config| config.voice_id.clone())
        .unwrap_or_else(|| default_kokoro_voice_for_lang(&target_lang).to_string())
}

/// Load the bundled `voices.txt` (one voice id per line, in the same order as
/// the speaker embedding table inside `voices.bin`). Falls back to the
/// well-known Kokoro v1.0 v1.0 voice manifest if `voices.txt` is missing.
fn resolve_voice_sid(voice: &str) -> anyhow::Result<i32> {
    let dir = get_kokoro_model_dir();
    let manifest_path = dir.join("voices.txt");
    if manifest_path.exists() {
        let contents = fs::read_to_string(&manifest_path)?;
        for (idx, line) in contents.lines().enumerate() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && trimmed.eq_ignore_ascii_case(voice) {
                return Ok(idx as i32);
            }
        }
    }

    for (idx, option) in KOKORO_VOICES.iter().enumerate() {
        if option.id.eq_ignore_ascii_case(voice) {
            return Ok(idx as i32);
        }
    }
    Err(anyhow::anyhow!("unknown voice id"))
}

fn build_session_with_timeout(
    settings: crate::config::KokoroSettings,
) -> anyhow::Result<KokoroSession> {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("kokoro-tts-init".to_string())
        .spawn(move || {
            let _ = tx.send(build_session(&settings));
        })?;

    match rx.recv_timeout(INIT_TIMEOUT) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            KOKORO_INIT_TIMED_OUT.store(true, Ordering::SeqCst);
            Err(anyhow::anyhow!(
                "timed out after {}s while loading sherpa-onnx OfflineTts",
                INIT_TIMEOUT.as_secs()
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(anyhow::anyhow!(
            "Kokoro initialisation worker exited unexpectedly"
        )),
    }
}

fn build_session(settings: &crate::config::KokoroSettings) -> anyhow::Result<KokoroSession> {
    let lib = ffi_tts::load()?;

    let model_dir = get_kokoro_model_dir();
    let to_cstring =
        |p: &Path| -> anyhow::Result<CString> { Ok(CString::new(p.to_string_lossy().as_bytes())?) };
    let to_cstring_str = |s: &str| -> anyhow::Result<CString> { Ok(CString::new(s)?) };

    let model_path = to_cstring(&model_dir.join("model.onnx"))?;
    let voices_path = to_cstring(&model_dir.join("voices.bin"))?;
    let tokens_path = to_cstring(&model_dir.join("tokens.txt"))?;
    let data_dir = to_cstring(&get_kokoro_espeak_data_dir())?;
    let dict_dir = to_cstring_str("")?;
    let lexicon = to_cstring_str(&join_required_paths(
        &get_kokoro_lexicon_paths(),
        "Kokoro v1.0 requires lexicon files; reinstall the Kokoro model from Downloaded Tools",
    )?)?;
    let lang = to_cstring_str(settings.lang.trim())?;
    let rule_fsts = to_cstring_str(&join_optional_paths(&get_kokoro_rule_fst_paths()))?;
    let rule_fars = to_cstring_str("")?;
    let provider = to_cstring_str("cpu")?;

    let mut config = ffi_tts::SherpaOnnxOfflineTtsConfig::zeroed();
    config.model.kokoro.model = model_path.as_ptr();
    config.model.kokoro.voices = voices_path.as_ptr();
    config.model.kokoro.tokens = tokens_path.as_ptr();
    config.model.kokoro.data_dir = data_dir.as_ptr();
    config.model.kokoro.length_scale = 1.0;
    config.model.kokoro.dict_dir = dict_dir.as_ptr();
    config.model.kokoro.lexicon = lexicon.as_ptr();
    config.model.kokoro.lang = lang.as_ptr();
    config.model.num_threads = settings.num_threads.max(1);
    config.model.debug = 0;
    config.model.provider = provider.as_ptr();
    config.rule_fsts = rule_fsts.as_ptr();
    config.rule_fars = rule_fars.as_ptr();
    config.max_num_sentences = 1;
    config.silence_scale = 1.0;

    let handle = unsafe { (lib.create_tts)(&config) };
    if handle.is_null() {
        return Err(anyhow::anyhow!(
            "SherpaOnnxCreateOfflineTts returned null (check that model files exist and DLLs match)"
        ));
    }

    // Keep CStrings alive only for the duration of CreateOfflineTts;
    // sherpa-onnx copies the strings internally, so we can drop them here.
    drop(model_path);
    drop(voices_path);
    drop(tokens_path);
    drop(data_dir);
    drop(dict_dir);
    drop(lexicon);
    drop(lang);
    drop(rule_fsts);
    drop(rule_fars);
    drop(provider);

    Ok(KokoroSession {
        handle,
        destroy_fn: lib.destroy_tts,
        lang: settings.lang.clone(),
        num_threads: settings.num_threads.max(1),
    })
}

fn join_required_paths(
    paths: &[std::path::PathBuf],
    missing_message: &str,
) -> anyhow::Result<String> {
    let joined = join_optional_paths(paths);
    if joined.is_empty() {
        return Err(anyhow::anyhow!(missing_message.to_string()));
    }
    Ok(joined)
}

fn join_optional_paths(paths: &[std::path::PathBuf]) -> String {
    let mut present = Vec::with_capacity(paths.len());
    for path in paths {
        if path.exists() {
            present.push(path.to_string_lossy().to_string());
        }
    }
    present.join(",")
}
