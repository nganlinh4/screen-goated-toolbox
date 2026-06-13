pub mod assets;
pub mod runtime;
pub mod server;

use super::capture::{
    start_device_loopback_capture_resilient, start_mic_capture_resilient, start_per_app_capture,
};
use super::state::{SharedRealtimeState, TranscriptionMethod};
use super::transcript_state::MonotonicTranscriptState;
use super::utils::update_overlay_text;
use super::{REALTIME_RMS, WM_VOLUME_UPDATE};
use crate::config::Preset;
use anyhow::Result;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW};

const STREAMING_CHUNK_MS: u32 = 2_000;
const STREAMING_UNFIXED_CHUNKS: usize = 2;
const STREAMING_UNFIXED_TOKENS: usize = 5;
const TRANSCRIBE_INTERVAL_MS: u64 = 500;
const SILENCE_COMMIT_MS: u64 = 1_200;
const MIN_TRANSCRIBE_SAMPLES: usize = 8_000;
const VOICE_ACTIVITY_RMS: f32 = 0.015;
/// Qwen3-ASR model variant (0.6B or 1.7B)
#[derive(Clone, Copy, Debug)]
pub enum Qwen3ModelVariant {
    Small, // 0.6B
    Large, // 1.7B
}

pub fn run_qwen3_transcription_variant(
    _preset: Preset,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    state: SharedRealtimeState,
    variant: Qwen3ModelVariant,
) -> Result<()> {
    let locale = {
        let ui_language = crate::APP
            .lock()
            .map(|app| app.config.ui_language.clone())
            .unwrap_or_else(|_| "en".to_string());
        crate::gui::locale::LocaleText::get(&ui_language)
    };
    let capability = crate::runtime_support::supports_qwen3_local_runtime();
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return Err(anyhow::anyhow!(capability.details));
    }

    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(TranscriptionMethod::Qwen3Local);
    }

    let (is_downloaded, download_label) = match variant {
        Qwen3ModelVariant::Small => (
            assets::is_qwen3_model_downloaded(),
            locale.qwen3_model_downloading_small_overlay,
        ),
        Qwen3ModelVariant::Large => (
            assets::is_qwen3_1_7b_model_downloaded(),
            locale.qwen3_model_downloading_large_overlay,
        ),
    };

    if !is_downloaded {
        update_overlay_text(overlay_hwnd, download_label);
        match variant {
            Qwen3ModelVariant::Small => assets::download_qwen3_model(stop_signal.clone(), true)?,
            Qwen3ModelVariant::Large => {
                assets::download_qwen3_1_7b_model(stop_signal.clone(), true)?
            }
        }
    }

    if stop_signal.load(Ordering::Relaxed) {
        return Ok(());
    }

    if !runtime::is_qwen3_runtime_managed_installed() {
        update_overlay_text(overlay_hwnd, locale.qwen3_runtime_installing_overlay);
        runtime::download_qwen3_runtime(stop_signal.clone(), true)?;
    }

    if stop_signal.load(Ordering::Relaxed) {
        return Ok(());
    }

    let load_label = match variant {
        Qwen3ModelVariant::Small => locale.qwen3_model_loading_small_overlay,
        Qwen3ModelVariant::Large => locale.qwen3_model_loading_large_overlay,
    };
    update_overlay_text(overlay_hwnd, load_label);
    let model_dir = match variant {
        Qwen3ModelVariant::Small => assets::get_qwen3_model_dir(),
        Qwen3ModelVariant::Large => assets::get_qwen3_1_7b_model_dir(),
    };
    let runtime = runtime::Qwen3Runtime::load(&model_dir)?;
    update_overlay_text(overlay_hwnd, "");
    let mut session = runtime.create_session(
        STREAMING_CHUNK_MS,
        STREAMING_UNFIXED_CHUNKS,
        STREAMING_UNFIXED_TOKENS,
    )?;
    let audio_buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let pause_signal = Arc::new(AtomicBool::new(false));
    let _stream = start_audio_capture(audio_buffer.clone(), stop_signal.clone(), pause_signal)?;

    let mut transcript_state = MonotonicTranscriptState::default();
    let mut session_sample_count = 0usize;
    let mut last_request_sample_count = 0usize;
    let mut last_request_at = Instant::now() - Duration::from_millis(TRANSCRIBE_INTERVAL_MS);
    let mut last_voice_activity = Instant::now();
    let mut last_draft_change = Instant::now();
    let mut last_draft_text = String::new();
    // After 3s of no new draft text, append a visual period to signal sentence boundary
    // to the translation system. Not a real commit — model may still rewrite.
    const DRAFT_STALE_MS: u64 = 3_000;

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

        let new_samples = drain_audio_samples(&audio_buffer);
        if !new_samples.is_empty() {
            let rms = compute_rms(&new_samples);
            REALTIME_RMS.store(rms.to_bits(), Ordering::Relaxed);
            crate::overlay::recording::update_audio_viz(rms);
            if rms > 0.001 {
                crate::overlay::recording::AUDIO_WARMUP_COMPLETE.store(true, Ordering::SeqCst);
            }
            if rms > VOICE_ACTIVITY_RMS {
                last_voice_activity = Instant::now();
            }
            if !overlay_hwnd.is_invalid() {
                unsafe {
                    let _ =
                        PostMessageW(Some(overlay_hwnd), WM_VOLUME_UPDATE, WPARAM(0), LPARAM(0));
                }
            }
            session.append_pcm16(&new_samples, false)?;
            session_sample_count += new_samples.len();
        }

        if should_commit_on_silence(
            session_sample_count > 0,
            last_voice_activity,
            session_sample_count,
        ) {
            session.append_pcm16(&[], true)?;
            let finalized = session.step()?;
            let finalized_snapshot = transcript_state.ingest(&finalized);
            session_sample_count = 0;
            last_request_sample_count = 0;
            last_draft_text.clear();
            last_draft_change = Instant::now();
            drop(session);
            session = runtime.create_session(
                STREAMING_CHUNK_MS,
                STREAMING_UNFIXED_CHUNKS,
                STREAMING_UNFIXED_TOKENS,
            )?;
            publish_transcript(&state, overlay_hwnd, &finalized_snapshot.committed_text, "");
        }

        let should_request = session_sample_count >= MIN_TRANSCRIBE_SAMPLES
            && session_sample_count != last_request_sample_count
            && last_request_at.elapsed() >= Duration::from_millis(TRANSCRIBE_INTERVAL_MS);

        if should_request {
            let transcript = session.step()?;
            let _runtime_metadata = (
                transcript.latency_ms,
                transcript.audio_samples,
                transcript.is_final,
            );
            last_request_sample_count = session_sample_count;
            last_request_at = Instant::now();

            let _detected_language = &transcript.language;
            let snapshot = transcript_state.ingest(&transcript);
            let draft_text = snapshot.draft_text.clone();
            if draft_text != last_draft_text {
                last_draft_change = Instant::now();
                last_draft_text = draft_text.clone();
            }
            // If draft hasn't changed for DRAFT_STALE_MS, append a visual period to
            // signal a sentence boundary to the translation system.
            // Not a real commit — model may still rewrite the draft.
            let draft_to_publish = if !draft_text.is_empty()
                && last_draft_change.elapsed() >= Duration::from_millis(DRAFT_STALE_MS)
            {
                format!("{}.", draft_text.trim_end())
            } else {
                draft_text
            };
            publish_transcript(
                &state,
                overlay_hwnd,
                &snapshot.committed_text,
                &draft_to_publish,
            );
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    // Explicitly drop session and runtime to release GPU memory before
    // switching to another transcription method.
    drop(session);
    drop(runtime);
    crate::log_info!("[Qwen3Runtime] Runtime and session dropped, GPU memory released");

    Ok(())
}

fn start_audio_capture(
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    pause_signal: Arc<AtomicBool>,
) -> Result<Option<cpal::Stream>> {
    let (audio_source, check_per_app) = {
        let app = crate::APP.lock().unwrap();
        (app.config.realtime_audio_source.clone(), true)
    };

    use crate::overlay::realtime_webview::{REALTIME_TTS_ENABLED, SELECTED_APP_PID};
    let tts_enabled = REALTIME_TTS_ENABLED.load(Ordering::SeqCst);
    let selected_pid = SELECTED_APP_PID.load(Ordering::SeqCst);
    let using_per_app_capture =
        check_per_app && audio_source == "device" && tts_enabled && selected_pid > 0;

    if using_per_app_capture {
        #[cfg(target_os = "windows")]
        {
            start_per_app_capture(selected_pid, audio_buffer, stop_signal, pause_signal)?;
            Ok(None)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Ok(None)
        }
    } else if audio_source == "mic" {
        Ok(Some(start_mic_capture_resilient(
            audio_buffer,
            stop_signal,
            pause_signal,
        )?))
    } else if audio_source == "device" && tts_enabled && selected_pid == 0 {
        Ok(None)
    } else {
        Ok(Some(start_device_loopback_capture_resilient(
            audio_buffer,
            stop_signal,
            pause_signal,
        )?))
    }
}

fn drain_audio_samples(audio_buffer: &Arc<Mutex<Vec<i16>>>) -> Vec<i16> {
    let mut buffer = audio_buffer.lock().unwrap();
    if buffer.is_empty() {
        Vec::new()
    } else {
        buffer.drain(..).collect()
    }
}

fn compute_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_sq: f64 = samples
        .iter()
        .map(|&sample| (sample as f64 / 32768.0).powi(2))
        .sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

fn should_commit_on_silence(
    has_pending_session_audio: bool,
    last_voice_activity: Instant,
    current_audio_len: usize,
) -> bool {
    has_pending_session_audio
        && current_audio_len > 0
        && last_voice_activity.elapsed() >= Duration::from_millis(SILENCE_COMMIT_MS)
}

fn publish_transcript(
    state: &SharedRealtimeState,
    overlay_hwnd: HWND,
    committed: &str,
    draft: &str,
) {
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(TranscriptionMethod::Qwen3Local);
        s.set_transcript_segments(committed, draft);
        let display = s.display_transcript.clone();
        update_overlay_text(overlay_hwnd, &display);
    }
}
