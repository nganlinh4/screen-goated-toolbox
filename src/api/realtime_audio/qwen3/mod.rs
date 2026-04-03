pub mod assets;
pub mod runtime;

use super::capture::{start_device_loopback_capture, start_mic_capture, start_per_app_capture};
use super::state::{SharedRealtimeState, TranscriptionMethod};
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
const MAX_SEGMENT_SAMPLES: usize = 20 * 16_000; // 20 seconds max before forced commit
pub fn run_qwen3_transcription(
    _preset: Preset,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    state: SharedRealtimeState,
) -> Result<()> {
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(TranscriptionMethod::Qwen3Local);
    }

    if !assets::is_qwen3_model_downloaded() {
        update_overlay_text(overlay_hwnd, "Downloading Qwen3-ASR model...");
        assets::download_qwen3_model(stop_signal.clone(), true)?;
    }

    if stop_signal.load(Ordering::Relaxed) {
        return Ok(());
    }

    if !runtime::is_qwen3_runtime_managed_installed() {
        update_overlay_text(overlay_hwnd, "Installing Qwen3-ASR CUDA runtime...");
        runtime::download_qwen3_runtime(stop_signal.clone(), true)?;
    }

    if stop_signal.load(Ordering::Relaxed) {
        return Ok(());
    }

    update_overlay_text(overlay_hwnd, "Loading Qwen3-ASR model...");
    let model_dir = assets::get_qwen3_model_dir();
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

    let mut last_appended_text = String::new();
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
                    let _ =
                        PostMessageW(Some(overlay_hwnd), WM_VOLUME_UPDATE, WPARAM(0), LPARAM(0));
                }
            }
            session.append_pcm16(&new_samples, false)?;
            session_sample_count += new_samples.len();
        }

        let force_commit = session_sample_count >= MAX_SEGMENT_SAMPLES;
        if force_commit || should_commit_on_silence(
            session_sample_count > 0,
            last_voice_activity,
            session_sample_count,
        ) {
            session.append_pcm16(&[], true)?;
            let finalized = session.step()?;
            let finalized_text = runtime_final_text(&finalized);
            // Append only the NEW portion that wasn't appended yet
            if finalized_text.len() > last_appended_text.len()
                && finalized_text.starts_with(&last_appended_text)
            {
                let new_part = &finalized_text[last_appended_text.len()..];
                if let Ok(mut s) = state.lock() {
                    s.append_transcript(new_part);
                }
            } else if !finalized_text.is_empty() && finalized_text != last_appended_text {
                // Text diverged — append the finalized text as new segment
                let separator = if last_appended_text.is_empty() { "" } else { " " };
                if let Ok(mut s) = state.lock() {
                    s.append_transcript(&format!("{separator}{finalized_text}"));
                }
            }
            last_appended_text.clear();
            session_sample_count = 0;
            last_request_sample_count = 0;
            session.reset()?;
            notify_overlay_update(overlay_hwnd);
        }

        let should_request = session_sample_count >= MIN_TRANSCRIBE_SAMPLES
            && session_sample_count != last_request_sample_count
            && last_request_at.elapsed() >= Duration::from_millis(TRANSCRIBE_INTERVAL_MS);

        if should_request {
            let transcript = session.step()?;
            last_request_sample_count = session_sample_count;
            last_request_at = Instant::now();

            // Use the full text as the "current draft" — append_transcript handles
            // committed/uncommitted split via the shared state
            let current_text = if !transcript.text.is_empty() {
                transcript.text.clone()
            } else {
                join_transcript_segments(&transcript.fixed_text, &transcript.draft_text)
            };

            // Find what's new since last append
            if current_text.len() > last_appended_text.len()
                && current_text.starts_with(&last_appended_text)
            {
                let new_part = &current_text[last_appended_text.len()..];
                if !new_part.is_empty() {
                    if let Ok(mut s) = state.lock() {
                        s.append_transcript(new_part);
                    }
                }
            } else if current_text != last_appended_text && !current_text.is_empty() {
                // Text diverged from previous — this is normal for Qwen re-encoding.
                // Only append the truly new suffix to avoid breaking committed text.
                let shared = shared_prefix_len(&last_appended_text, &current_text);
                let new_part = &current_text[shared..];
                if !new_part.is_empty() {
                    if let Ok(mut s) = state.lock() {
                        // Trim back to the shared prefix, then append new
                        let full = s.full_transcript.clone();
                        if full.len() > shared && shared > 0 {
                            s.full_transcript.truncate(
                                full.len() - (last_appended_text.len() - shared)
                            );
                        }
                        s.append_transcript(new_part);
                    }
                }
            }
            last_appended_text = current_text;
            notify_overlay_update(overlay_hwnd);
        }

        std::thread::sleep(Duration::from_millis(100));
    }

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

fn notify_overlay_update(overlay_hwnd: HWND) {
    if !overlay_hwnd.is_invalid() {
        unsafe {
            let _ = PostMessageW(
                Some(overlay_hwnd),
                super::WM_REALTIME_UPDATE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

fn shared_prefix_len(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .take_while(|((_, ac), bc)| ac == bc)
        .last()
        .map(|((i, c), _)| i + c.len_utf8())
        .unwrap_or(0)
}

fn runtime_live_segments(result: &runtime::RuntimeTranscriptionResult) -> (String, String) {
    if result.fixed_text.is_empty() && result.draft_text.is_empty() {
        (String::new(), result.text.clone())
    } else {
        (result.fixed_text.clone(), result.draft_text.clone())
    }
}

fn runtime_final_text(result: &runtime::RuntimeTranscriptionResult) -> String {
    if !result.text.is_empty() {
        result.text.clone()
    } else {
        join_transcript_segments(&result.fixed_text, &result.draft_text)
    }
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
