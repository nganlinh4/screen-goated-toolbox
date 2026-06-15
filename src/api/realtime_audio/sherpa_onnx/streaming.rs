use super::super::capture::{
    start_device_loopback_capture_resilient, start_mic_capture_resilient, start_per_app_capture,
};
use super::super::offline_asr_commit::{OfflineAsrCommitState, offline_asr_commit_step};
use super::super::state::{SharedRealtimeState, TranscriptionMethod};
use super::super::utils::update_overlay_text;
use super::super::{REALTIME_RMS, WM_VOLUME_UPDATE};
use super::ffi;
use anyhow::Result;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW};

const BATCH_SAMPLES: usize = 16000 / 2; // 500ms worth at 16kHz

pub(super) struct SherpaStreamingLoop<'a> {
    pub(super) lib: &'a ffi::SherpaLib,
    pub(super) recognizer: *const ffi::SherpaOnnxOnlineRecognizer,
    pub(super) stream: *const ffi::SherpaOnnxOnlineStream,
    pub(super) audio_buffer: Arc<Mutex<Vec<i16>>>,
    pub(super) stop_signal: &'a AtomicBool,
    pub(super) overlay_hwnd: HWND,
    pub(super) state: &'a SharedRealtimeState,
    pub(super) has_native_punctuation: bool,
    pub(super) session_id: u64,
}

#[derive(Clone, Copy)]
struct RecognizerContext<'a> {
    lib: &'a ffi::SherpaLib,
    recognizer: *const ffi::SherpaOnnxOnlineRecognizer,
    stream: *const ffi::SherpaOnnxOnlineStream,
}

pub(super) fn run_streaming_loop(params: SherpaStreamingLoop<'_>) -> Result<()> {
    let SherpaStreamingLoop {
        lib,
        recognizer,
        stream,
        audio_buffer,
        stop_signal,
        overlay_hwnd,
        state,
        has_native_punctuation,
        session_id,
    } = params;
    let recognizer_context = RecognizerContext {
        lib,
        recognizer,
        stream,
    };
    let mut transcript_state = OfflineAsrCommitState::default();
    let loop_start = Instant::now();
    let mut pending_f32: Vec<f32> = Vec::new();

    while !stop_signal.load(Ordering::Relaxed) && !is_stale_session(session_id) {
        if !overlay_hwnd.is_invalid() && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            break;
        }
        if super::super::DEVICE_RECONNECT_REQUESTED.load(Ordering::SeqCst) {
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
            for &sample in &new_samples {
                pending_f32.push(sample as f32 / 32768.0);
            }
        }

        if pending_f32.len() >= BATCH_SAMPLES {
            process_pending_audio(
                &mut pending_f32,
                recognizer_context,
                stop_signal,
                session_id,
            );

            if stop_signal.load(Ordering::Relaxed) || is_stale_session(session_id) {
                break;
            }

            handle_recognizer_result(
                recognizer_context,
                state,
                overlay_hwnd,
                has_native_punctuation,
                &mut transcript_state,
                loop_start.elapsed().as_millis() as u64,
            );
        } else {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    Ok(())
}

fn process_pending_audio(
    pending_f32: &mut Vec<f32>,
    recognizer_context: RecognizerContext<'_>,
    stop_signal: &AtomicBool,
    session_id: u64,
) {
    let RecognizerContext {
        lib,
        recognizer,
        stream,
    } = recognizer_context;
    let max_buffer = 16000 * 10;
    if pending_f32.len() > max_buffer {
        let drop = pending_f32.len() - max_buffer;
        pending_f32.drain(..drop);
    }

    let batch: Vec<f32> = std::mem::take(pending_f32);
    unsafe {
        if stop_signal.load(Ordering::Relaxed) || is_stale_session(session_id) {
            return;
        }
        (lib.accept_waveform)(stream, 16000, batch.as_ptr(), batch.len() as i32);
    }
    while unsafe { (lib.is_ready)(recognizer, stream) } != 0 {
        if stop_signal.load(Ordering::Relaxed) || is_stale_session(session_id) {
            break;
        }
        unsafe { (lib.decode)(recognizer, stream) };
    }
}

fn handle_recognizer_result(
    recognizer_context: RecognizerContext<'_>,
    state: &SharedRealtimeState,
    overlay_hwnd: HWND,
    has_native_punctuation: bool,
    transcript_state: &mut OfflineAsrCommitState,
    now_ms: u64,
) {
    let RecognizerContext {
        lib,
        recognizer,
        stream,
    } = recognizer_context;
    let result_ptr = unsafe { (lib.get_result_json)(recognizer, stream) };
    if result_ptr.is_null() {
        return;
    }

    let result_cstr = unsafe { std::ffi::CStr::from_ptr(result_ptr) };
    let result_str = result_cstr.to_string_lossy();
    let text = parse_result_text(&result_str);
    unsafe { (lib.destroy_result_json)(result_ptr) };

    // Canonical commit machine — shared with the Android port via golden fixtures.
    let active = offline_asr_commit_step(transcript_state, &text, has_native_punctuation, now_ms);
    publish_transcript(
        state,
        overlay_hwnd,
        &transcript_state.committed_history,
        &active,
    );
}

fn is_stale_session(session_id: u64) -> bool {
    crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst)
        != session_id
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
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(TranscriptionMethod::SherpaZipformer);
        s.set_transcript_segments(committed, draft);
        let display = s.display_transcript.clone();
        update_overlay_text(overlay_hwnd, &display);
    }
}

pub(super) fn start_audio_capture(
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
