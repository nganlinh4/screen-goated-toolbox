use anyhow::Result;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::api::realtime_audio::state::SharedRealtimeState;
use crate::api::realtime_audio::utils::update_overlay_text;
use crate::api::realtime_audio::websocket::{
    connect_websocket, parse_input_transcription, send_audio_chunk, send_setup_message,
    set_socket_nonblocking,
};
use crate::api::realtime_audio::{DEVICE_RECONNECT_REQUESTED, WM_VOLUME_UPDATE};

/// Audio mode state machine for silence injection.
#[derive(Clone, Copy, PartialEq)]
enum AudioMode {
    Normal,
    Silence,
    CatchUp,
}

impl AudioMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Silence => "silence",
            Self::CatchUp => "catchup",
        }
    }
}

fn samples_to_ms(samples: usize) -> usize {
    samples.saturating_mul(1_000) / 16_000
}

fn compute_i16_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64 / 32768.0).powi(2)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

pub(super) struct RealtimeMainLoop<'a> {
    pub(super) socket: tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    pub(super) audio_buffer: Arc<Mutex<Vec<i16>>>,
    pub(super) stop_signal: Arc<AtomicBool>,
    pub(super) overlay_hwnd: HWND,
    pub(super) state: SharedRealtimeState,
    pub(super) gemini_live_model: &'a str,
    pub(super) gemini_api_key: &'a str,
    pub(super) capture_label: &'static str,
}

pub(super) fn run_main_loop(params: RealtimeMainLoop<'_>) -> Result<()> {
    let RealtimeMainLoop {
        mut socket,
        audio_buffer,
        stop_signal,
        overlay_hwnd,
        state,
        gemini_live_model,
        gemini_api_key,
        capture_label,
    } = params;
    let mut last_send = Instant::now();
    let send_interval = Duration::from_millis(100);

    let mut audio_mode = AudioMode::Normal;
    let mut mode_start = Instant::now();
    let mut silence_buffer: Vec<i16> = Vec::new();

    const NORMAL_DURATION: Duration = Duration::from_secs(20);
    const SILENCE_DURATION: Duration = Duration::from_secs(2);
    const SAMPLES_PER_100MS: usize = 1600;

    let mut last_transcription_time = Instant::now();
    let mut consecutive_empty_reads: u32 = 0;
    const NO_RESULT_THRESHOLD_SECS: u64 = 8;
    const EMPTY_READ_CHECK_COUNT: u32 = 50;

    let session_started = Instant::now();
    let mut last_health_log = Instant::now();
    let mut sent_chunks = 0u64;
    let mut sent_samples = 0usize;
    let mut active_sent_samples = 0usize;
    let mut active_samples_since_transcript = 0usize;
    let mut transcript_updates = 0u64;
    let mut transcript_chars = 0usize;
    let mut reconnect_count = 0u32;

    while !stop_signal.load(Ordering::Relaxed) {
        if overlay_hwnd.0 != 0 as _ && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            stop_signal.store(true, Ordering::SeqCst);
            break;
        }

        {
            use crate::overlay::realtime_webview::{
                AUDIO_SOURCE_CHANGE, TRANSCRIPTION_MODEL_CHANGE,
            };
            if AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
                || TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
                || DEVICE_RECONNECT_REQUESTED.load(Ordering::SeqCst)
            {
                break;
            }
        }

        match audio_mode {
            AudioMode::Normal => {
                if mode_start.elapsed() >= NORMAL_DURATION {
                    audio_mode = AudioMode::Silence;
                    mode_start = Instant::now();
                    silence_buffer.clear();
                }
            }
            AudioMode::Silence => {
                if mode_start.elapsed() >= SILENCE_DURATION {
                    audio_mode = AudioMode::CatchUp;
                    mode_start = Instant::now();
                }
            }
            AudioMode::CatchUp => {
                if silence_buffer.is_empty() {
                    audio_mode = AudioMode::Normal;
                    mode_start = Instant::now();
                }
            }
        }

        if last_send.elapsed() >= send_interval {
            let real_audio: Vec<i16> = {
                let mut buf = audio_buffer.lock().unwrap();
                std::mem::take(&mut *buf)
            };
            match audio_mode {
                AudioMode::Normal => {
                    if !real_audio.is_empty() {
                        let is_active = compute_i16_rms(&real_audio) >= ACTIVE_AUDIO_RMS_THRESHOLD;
                        if send_audio_chunk(&mut socket, &real_audio).is_err() {
                            break;
                        }
                        sent_chunks += 1;
                        sent_samples += real_audio.len();
                        if is_active {
                            active_sent_samples += real_audio.len();
                            active_samples_since_transcript += real_audio.len();
                        }
                    }
                }
                AudioMode::Silence => {
                    silence_buffer.extend(real_audio);
                    let silence: Vec<i16> = vec![0i16; SAMPLES_PER_100MS];
                    if send_audio_chunk(&mut socket, &silence).is_err() {
                        break;
                    }
                    sent_chunks += 1;
                    sent_samples += silence.len();
                }
                AudioMode::CatchUp => {
                    silence_buffer.extend(real_audio);
                    let chunk_size = SAMPLES_PER_100MS * 2;
                    let to_send: Vec<i16> = if silence_buffer.len() >= chunk_size {
                        silence_buffer.drain(..chunk_size).collect()
                    } else if !silence_buffer.is_empty() {
                        std::mem::take(&mut silence_buffer)
                    } else {
                        Vec::new()
                    };
                    if !to_send.is_empty() && send_audio_chunk(&mut socket, &to_send).is_err() {
                        break;
                    }
                    if !to_send.is_empty() {
                        let is_active = compute_i16_rms(&to_send) >= ACTIVE_AUDIO_RMS_THRESHOLD;
                        sent_chunks += 1;
                        sent_samples += to_send.len();
                        if is_active {
                            active_sent_samples += to_send.len();
                            active_samples_since_transcript += to_send.len();
                        }
                    }
                }
            }
            last_send = Instant::now();
            unsafe {
                let _ = PostMessageW(Some(overlay_hwnd), WM_VOLUME_UPDATE, WPARAM(0), LPARAM(0));
            }
        }

        match socket.read() {
            Ok(tungstenite::Message::Text(msg)) => {
                let msg = msg.as_str();
                if let Some(transcript) = parse_input_transcription(msg)
                    && !transcript.is_empty()
                {
                    last_transcription_time = Instant::now();
                    active_samples_since_transcript = 0;
                    consecutive_empty_reads = 0;
                    transcript_updates += 1;
                    transcript_chars += transcript.chars().count();
                    let display_text = if let Ok(mut s) = state.lock() {
                        s.append_transcript(&transcript);
                        s.display_transcript.clone()
                    } else {
                        String::new()
                    };
                    if !display_text.is_empty() {
                        update_overlay_text(overlay_hwnd, &display_text);
                    }
                }
            }
            Ok(tungstenite::Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec())
                    && let Some(transcript) = parse_input_transcription(&text)
                    && !transcript.is_empty()
                {
                    last_transcription_time = Instant::now();
                    active_samples_since_transcript = 0;
                    consecutive_empty_reads = 0;
                    transcript_updates += 1;
                    transcript_chars += transcript.chars().count();
                    let display_text = if let Ok(mut s) = state.lock() {
                        s.append_transcript(&transcript);
                        s.display_transcript.clone()
                    } else {
                        String::new()
                    };
                    if !display_text.is_empty() {
                        update_overlay_text(overlay_hwnd, &display_text);
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => {
                crate::log_info!(
                    "[RealtimeGeminiLiveHealth] reconnect-start reason=close empty_reads={} since_transcript_ms={} mode={}",
                    consecutive_empty_reads,
                    last_transcription_time.elapsed().as_millis(),
                    audio_mode.as_str()
                );
                if !try_reconnect(ReconnectContext {
                    socket: &mut socket,
                    api_key: gemini_api_key,
                    model: gemini_live_model,
                    audio_buffer: &audio_buffer,
                    silence_buffer: &mut silence_buffer,
                    audio_mode: &mut audio_mode,
                    mode_start: &mut mode_start,
                    last_transcription_time: &mut last_transcription_time,
                    consecutive_empty_reads: &mut consecutive_empty_reads,
                }) {
                    crate::log_info!("[RealtimeGeminiLiveHealth] reconnect-failed reason=close");
                    break;
                }
                reconnect_count += 1;
                crate::log_info!(
                    "[RealtimeGeminiLiveHealth] reconnect-ok reason=close count={} catchup_ms={}",
                    reconnect_count,
                    samples_to_ms(silence_buffer.len())
                );
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                consecutive_empty_reads += 1;
                if consecutive_empty_reads >= EMPTY_READ_CHECK_COUNT
                    && last_transcription_time.elapsed()
                        > Duration::from_secs(NO_RESULT_THRESHOLD_SECS)
                    && samples_to_ms(active_samples_since_transcript)
                        >= NO_RESULT_ACTIVE_AUDIO_THRESHOLD_MS
                {
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-start reason=no-results empty_reads={} since_transcript_ms={} active_since_transcript_ms={} mode={}",
                        consecutive_empty_reads,
                        last_transcription_time.elapsed().as_millis(),
                        samples_to_ms(active_samples_since_transcript),
                        audio_mode.as_str()
                    );
                    if !try_reconnect(ReconnectContext {
                        socket: &mut socket,
                        api_key: gemini_api_key,
                        model: gemini_live_model,
                        audio_buffer: &audio_buffer,
                        silence_buffer: &mut silence_buffer,
                        audio_mode: &mut audio_mode,
                        mode_start: &mut mode_start,
                        last_transcription_time: &mut last_transcription_time,
                        consecutive_empty_reads: &mut consecutive_empty_reads,
                    }) {
                        crate::log_info!(
                            "[RealtimeGeminiLiveHealth] reconnect-failed reason=no-results"
                        );
                        break;
                    }
                    reconnect_count += 1;
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-ok reason=no-results count={} catchup_ms={}",
                        reconnect_count,
                        samples_to_ms(silence_buffer.len())
                    );
                }
            }
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("reset")
                    || error_str.contains("closed")
                    || error_str.contains("broken")
                {
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-start reason=socket-error error={} empty_reads={} since_transcript_ms={} mode={}",
                        error_str,
                        consecutive_empty_reads,
                        last_transcription_time.elapsed().as_millis(),
                        audio_mode.as_str()
                    );
                    if !try_reconnect(ReconnectContext {
                        socket: &mut socket,
                        api_key: gemini_api_key,
                        model: gemini_live_model,
                        audio_buffer: &audio_buffer,
                        silence_buffer: &mut silence_buffer,
                        audio_mode: &mut audio_mode,
                        mode_start: &mut mode_start,
                        last_transcription_time: &mut last_transcription_time,
                        consecutive_empty_reads: &mut consecutive_empty_reads,
                    }) {
                        crate::log_info!(
                            "[RealtimeGeminiLiveHealth] reconnect-failed reason=socket-error error={}",
                            error_str
                        );
                        break;
                    }
                    reconnect_count += 1;
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-ok reason=socket-error count={} catchup_ms={}",
                        reconnect_count,
                        samples_to_ms(silence_buffer.len())
                    );
                } else {
                    break;
                }
            }
        }

        if last_health_log.elapsed() >= Duration::from_secs(5) {
            crate::log_info!(
                "[RealtimeGeminiLiveHealth] uptime_ms={} model={} capture={} mode={} mode_ms={} sent_chunks={} sent_ms={} active_ms={} active_since_transcript_ms={} catchup_ms={} transcript_updates={} transcript_chars={} empty_reads={} since_transcript_ms={} reconnects={}",
                session_started.elapsed().as_millis(),
                gemini_live_model,
                capture_label,
                audio_mode.as_str(),
                mode_start.elapsed().as_millis(),
                sent_chunks,
                samples_to_ms(sent_samples),
                samples_to_ms(active_sent_samples),
                samples_to_ms(active_samples_since_transcript),
                samples_to_ms(silence_buffer.len()),
                transcript_updates,
                transcript_chars,
                consecutive_empty_reads,
                last_transcription_time.elapsed().as_millis(),
                reconnect_count
            );
            last_health_log = Instant::now();
            sent_chunks = 0;
            sent_samples = 0;
            active_sent_samples = 0;
            transcript_updates = 0;
            transcript_chars = 0;
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    let _ = socket.close(None);
    Ok(())
}

const ACTIVE_AUDIO_RMS_THRESHOLD: f32 = 0.004;
const NO_RESULT_ACTIVE_AUDIO_THRESHOLD_MS: usize = 4_000;

struct ReconnectContext<'a> {
    socket: &'a mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    api_key: &'a str,
    model: &'a str,
    audio_buffer: &'a Arc<Mutex<Vec<i16>>>,
    silence_buffer: &'a mut Vec<i16>,
    audio_mode: &'a mut AudioMode,
    mode_start: &'a mut Instant,
    last_transcription_time: &'a mut Instant,
    consecutive_empty_reads: &'a mut u32,
}

fn try_reconnect(context: ReconnectContext<'_>) -> bool {
    let ReconnectContext {
        socket,
        api_key,
        model,
        audio_buffer,
        silence_buffer,
        audio_mode,
        mode_start,
        last_transcription_time,
        consecutive_empty_reads,
    } = context;
    let mut reconnect_buffer: Vec<i16> = Vec::new();
    let _ = socket.close(None);

    for _attempt in 1..=3 {
        {
            let mut buf = audio_buffer.lock().unwrap();
            reconnect_buffer.extend(std::mem::take(&mut *buf));
        }

        match connect_websocket(api_key) {
            Ok(mut new_socket) => {
                if send_setup_message(&mut new_socket, model).is_err() {
                    continue;
                }
                if set_socket_nonblocking(&mut new_socket).is_err() {
                    continue;
                }
                {
                    let mut buf = audio_buffer.lock().unwrap();
                    reconnect_buffer.extend(std::mem::take(&mut *buf));
                }
                silence_buffer.clear();
                silence_buffer.extend(reconnect_buffer);
                *audio_mode = AudioMode::CatchUp;
                *mode_start = Instant::now();
                *socket = new_socket;
                *last_transcription_time = Instant::now();
                *consecutive_empty_reads = 0;
                return true;
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    false
}
