pub mod assets;
pub mod reference;
pub mod runtime;
pub mod server;

use super::capture::{start_device_loopback_capture, start_mic_capture, start_per_app_capture};
use super::state::{SharedRealtimeState, TranscriptionMethod};
use super::utils::update_overlay_text;
use super::{REALTIME_RMS, WM_VOLUME_UPDATE};
use crate::config::Preset;
use anyhow::{Result, anyhow};
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
const MAX_SESSION_REPLAY_SAMPLES: usize = 16_000 * 30;

pub fn run_qwen3_transcription(
    _preset: Preset,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    state: SharedRealtimeState,
) -> Result<()> {
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(TranscriptionMethod::Qwen3TurboQuant);
    }

    if !assets::is_qwen3_model_downloaded() {
        assets::download_qwen3_model(stop_signal.clone(), false)?;
    }

    if stop_signal.load(Ordering::Relaxed) {
        return Ok(());
    }

    if !reference::has_discoverable_server() {
        server::download_qwen3_server(stop_signal.clone(), false)?;
    }

    let model_dir = assets::get_qwen3_model_dir();
    let mut reference_server = reference::QwenReferenceServer::start(&model_dir)?;
    let mut session_id = reference_server.create_streaming_session(
        STREAMING_CHUNK_MS,
        STREAMING_UNFIXED_CHUNKS,
        STREAMING_UNFIXED_TOKENS,
    )?;
    let audio_buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let pause_signal = Arc::new(AtomicBool::new(false));
    let _stream = start_audio_capture(audio_buffer.clone(), stop_signal.clone(), pause_signal)?;

    let mut committed_history = String::new();
    let mut session_audio_buffer = Vec::new();
    let mut session_sample_count = 0usize;
    let mut last_request_sample_count = 0usize;
    let mut last_request_at = Instant::now() - Duration::from_millis(TRANSCRIBE_INTERVAL_MS);
    let mut last_voice_activity = Instant::now();

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
                    let _ = PostMessageW(Some(overlay_hwnd), WM_VOLUME_UPDATE, WPARAM(0), LPARAM(0));
                }
            }
            session_audio_buffer.extend_from_slice(&new_samples);
            if session_audio_buffer.len() > MAX_SESSION_REPLAY_SAMPLES {
                let overflow = session_audio_buffer.len() - MAX_SESSION_REPLAY_SAMPLES;
                session_audio_buffer.drain(..overflow);
            }

            if let Err(first_error) = reference_server.append_streaming_audio(session_id, &new_samples)
            {
                reference_server = reference::QwenReferenceServer::start(&model_dir).map_err(
                    |restart_error| {
                        anyhow!(
                            "Qwen3 streaming audio append failed, and restarting the local ASR server also failed.\n\nFirst error:\n{first_error}\n\nRestart error:\n{restart_error}"
                        )
                    },
                )?;
                session_id = create_streaming_session(&mut reference_server).map_err(
                    |session_error| {
                        anyhow!(
                            "Qwen3 streaming audio append failed, the local ASR server restarted, but creating a new streaming session failed.\n\nFirst error:\n{first_error}\n\nSession error:\n{session_error}"
                        )
                    },
                )?;
                reference_server
                    .append_streaming_audio(session_id, &session_audio_buffer)
                    .map_err(|retry_error| {
                        anyhow!(
                            "Qwen3 streaming audio append failed, the local ASR server was restarted once, but replaying the buffered audio still failed.\n\nFirst error:\n{first_error}\n\nRetry error:\n{retry_error}"
                        )
                    })?;
            }
            session_sample_count += new_samples.len();
        }

        if should_commit_on_silence(
            !session_audio_buffer.is_empty(),
            last_voice_activity,
            session_sample_count,
        ) {
            let finalized = reference_server
                .transcribe_streaming_session(session_id, None, true)
                .map_err(|finalize_error| {
                    anyhow!("Qwen3 streaming finalize failed before silence reset: {finalize_error}")
                })?;
            append_history_segment(&mut committed_history, &finalized.text);
            session_audio_buffer.clear();
            session_sample_count = 0;
            last_request_sample_count = 0;
            reference_server.reset_streaming_session(session_id)?;
            publish_transcript(&state, overlay_hwnd, &committed_history, "");
        }

        let should_request = session_sample_count >= MIN_TRANSCRIBE_SAMPLES
            && session_sample_count != last_request_sample_count
            && last_request_at.elapsed() >= Duration::from_millis(TRANSCRIBE_INTERVAL_MS);

        if should_request {
            let transcript = match reference_server.transcribe_streaming_session(session_id, None, false)
            {
                Ok(transcript) => transcript,
                Err(first_error) => {
                    reference_server = reference::QwenReferenceServer::start(&model_dir).map_err(
                        |restart_error| {
                            anyhow!(
                                "Qwen3 streaming transcription failed, and restarting the local ASR server also failed.\n\nFirst error:\n{first_error}\n\nRestart error:\n{restart_error}"
                            )
                        },
                    )?;
                    session_id = create_streaming_session(&mut reference_server)?;
                    if !session_audio_buffer.is_empty() {
                        reference_server
                            .append_streaming_audio(session_id, &session_audio_buffer)
                            .map_err(|replay_error| {
                                anyhow!(
                                    "Qwen3 streaming transcription failed, the local ASR server restarted, but replaying the current streaming audio buffer failed.\n\nFirst error:\n{first_error}\n\nReplay error:\n{replay_error}"
                                )
                            })?;
                    }
                    reference_server
                        .transcribe_streaming_session(session_id, None, false)
                        .map_err(|retry_error| {
                            anyhow!(
                                "Qwen3 streaming transcription failed, the local ASR server was restarted once, but the retry still failed.\n\nFirst error:\n{first_error}\n\nRetry error:\n{retry_error}"
                            )
                        })?
                }
            };
            last_request_sample_count = session_sample_count;
            last_request_at = Instant::now();

            let _detected_language = &transcript.language;
            let live_committed = join_transcript_segments(&committed_history, &transcript.fixed_text);
            publish_transcript(&state, overlay_hwnd, &live_committed, &transcript.draft_text);
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    reference_server.shutdown();

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

fn append_history_segment(history: &mut String, segment: &str) {
    let segment = sanitize_transcript_segment(segment);
    if segment.is_empty() {
        return;
    }
    if history.is_empty() {
        history.push_str(segment.trim_start());
    } else {
        let combined = join_transcript_segments(history, &segment);
        history.clear();
        history.push_str(&combined);
    }
}

fn sanitize_transcript_segment(segment: &str) -> String {
    segment.replace('\n', " ").replace('\t', " ")
}

fn join_transcript_segments(left: &str, right: &str) -> String {
    let left = sanitize_transcript_segment(left);
    let right = sanitize_transcript_segment(right);
    match (left.is_empty(), right.is_empty()) {
        (true, true) => String::new(),
        (true, false) => right.trim_start().to_string(),
        (false, true) => left,
        (false, false) => {
            let left_has_space = left.chars().last().is_some_and(char::is_whitespace);
            let right_has_space = right.chars().next().is_some_and(char::is_whitespace);
            if left_has_space || right_has_space {
                format!("{left}{right}")
            } else {
                format!("{left} {right}")
            }
        }
    }
}

fn create_streaming_session(
    reference_server: &mut reference::QwenReferenceServer,
) -> Result<u64> {
    reference_server.create_streaming_session(
        STREAMING_CHUNK_MS,
        STREAMING_UNFIXED_CHUNKS,
        STREAMING_UNFIXED_TOKENS,
    )
}

fn publish_transcript(
    state: &SharedRealtimeState,
    overlay_hwnd: HWND,
    committed: &str,
    draft: &str,
) {
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(TranscriptionMethod::Qwen3TurboQuant);
        s.set_transcript_segments(committed, draft);
        let display = s.display_transcript.clone();
        update_overlay_text(overlay_hwnd, &display);
    }
}
