use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::api::realtime_audio::websocket::{
    connect_websocket, parse_input_transcription, send_audio_chunk, send_setup_message,
    set_socket_nonblocking,
};
use crate::config::Preset;
use crate::overlay::result::update_window_text;

#[derive(Clone, Copy, PartialEq)]
enum AudioMode {
    Normal,
    Silence,
    CatchUp,
}

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
    stop_signal: &'a Arc<AtomicBool>,
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
        stop_signal,
    } = context;
    let mut reconnect_buffer: Vec<i16> = Vec::new();
    let _ = socket.close(None);

    loop {
        if stop_signal.load(Ordering::Relaxed) {
            println!("[GeminiLiveStream] Stop signal received during reconnection.");
            return false;
        }

        {
            let mut buf = audio_buffer.lock().unwrap();
            reconnect_buffer.extend(std::mem::take(&mut *buf));
        }

        match connect_websocket(api_key) {
            Ok(mut new_socket) => {
                if send_setup_message(&mut new_socket, model).is_err() {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
                if set_socket_nonblocking(&mut new_socket).is_err() {
                    let _ = new_socket.close(None);
                    std::thread::sleep(Duration::from_millis(500));
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
            Err(e) => {
                println!(
                    "[GeminiLiveStream] Reconnection failed: {}. Retrying in 1s...",
                    e
                );
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

/// Main streaming loop - sends audio and receives transcriptions.
pub(super) struct StreamingLoopContext<'a, F> {
    pub(super) preset: &'a Preset,
    pub(super) socket: &'a mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    pub(super) api_key: &'a str,
    pub(super) model: &'a str,
    pub(super) audio_buffer: &'a Arc<Mutex<Vec<i16>>>,
    pub(super) accumulated_text: &'a Arc<Mutex<String>>,
    pub(super) stop_signal: &'a Arc<AtomicBool>,
    pub(super) pause_signal: &'a Arc<AtomicBool>,
    pub(super) abort_signal: &'a Arc<AtomicBool>,
    pub(super) overlay_hwnd: HWND,
    pub(super) update_stream_text: &'a F,
}

pub(super) fn run_streaming_loop<F>(context: StreamingLoopContext<'_, F>)
where
    F: Fn(&str),
{
    let StreamingLoopContext {
        preset,
        socket,
        api_key,
        model,
        audio_buffer,
        accumulated_text,
        stop_signal,
        pause_signal,
        abort_signal,
        overlay_hwnd,
        update_stream_text,
    } = context;
    const CHUNK_SIZE: usize = 1600;
    const NORMAL_DURATION: Duration = Duration::from_secs(20);
    const SILENCE_DURATION: Duration = Duration::from_secs(2);
    const SAMPLES_PER_100MS: usize = 1600;
    const NO_RESULT_THRESHOLD_SECS: u64 = 8;
    const EMPTY_READ_CHECK_COUNT: u32 = 50;

    let mut last_send = Instant::now();
    let send_interval = Duration::from_millis(100);
    let auto_stop = preset.auto_stop_recording;
    let mut has_spoken = false;
    let mut first_speech: Option<Instant> = None;
    let mut last_active = Instant::now();

    let mut audio_mode = AudioMode::Normal;
    let mut mode_start = Instant::now();
    let mut silence_buffer: Vec<i16> = Vec::new();
    let mut last_transcription_time = Instant::now();
    let mut consecutive_empty_reads: u32 = 0;

    while !stop_signal.load(Ordering::SeqCst) && !abort_signal.load(Ordering::SeqCst) {
        if !preset.hide_recording_ui && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            break;
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
                    if !real_audio.is_empty() && !pause_signal.load(Ordering::Relaxed) {
                        for chunk in real_audio.chunks(CHUNK_SIZE) {
                            if send_audio_chunk(socket, chunk).is_err() {
                                break;
                            }
                        }
                    }
                }
                AudioMode::Silence => {
                    silence_buffer.extend(real_audio);
                    let silence: Vec<i16> = vec![0i16; SAMPLES_PER_100MS];
                    if send_audio_chunk(socket, &silence).is_err() {
                        break;
                    }
                }
                AudioMode::CatchUp => {
                    silence_buffer.extend(real_audio);
                    let double_chunk = SAMPLES_PER_100MS * 2;
                    let to_send: Vec<i16> = if silence_buffer.len() >= double_chunk {
                        silence_buffer.drain(..double_chunk).collect()
                    } else if !silence_buffer.is_empty() {
                        std::mem::take(&mut silence_buffer)
                    } else {
                        Vec::new()
                    };
                    if !to_send.is_empty() && send_audio_chunk(socket, &to_send).is_err() {
                        break;
                    }
                }
            }
            last_send = Instant::now();
        }

        loop {
            match socket.read() {
                Ok(tungstenite::Message::Text(msg)) => {
                    if let Some(t) = parse_input_transcription(msg.as_str())
                        && !t.is_empty()
                    {
                        last_transcription_time = Instant::now();
                        consecutive_empty_reads = 0;
                        if let Ok(mut txt) = accumulated_text.lock() {
                            txt.push_str(&t);
                            update_stream_text(&txt);
                        }
                        if preset.auto_paste {
                            crate::overlay::utils::type_text_to_window(None, &t);
                        }
                    }
                }
                Ok(tungstenite::Message::Binary(data)) => {
                    if let Ok(s) = String::from_utf8(data.to_vec())
                        && let Some(t) = parse_input_transcription(&s)
                        && !t.is_empty()
                    {
                        last_transcription_time = Instant::now();
                        consecutive_empty_reads = 0;
                        if let Ok(mut txt) = accumulated_text.lock() {
                            txt.push_str(&t);
                            update_stream_text(&txt);
                        }
                        if preset.auto_paste {
                            crate::overlay::utils::type_text_to_window(None, &t);
                        }
                    }
                }
                Ok(tungstenite::Message::Close(_)) => {
                    if !try_reconnect(ReconnectContext {
                        socket,
                        api_key,
                        model,
                        audio_buffer,
                        silence_buffer: &mut silence_buffer,
                        audio_mode: &mut audio_mode,
                        mode_start: &mut mode_start,
                        last_transcription_time: &mut last_transcription_time,
                        consecutive_empty_reads: &mut consecutive_empty_reads,
                        stop_signal,
                    }) {
                        return;
                    }
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
                        && !try_reconnect(ReconnectContext {
                            socket,
                            api_key,
                            model,
                            audio_buffer,
                            silence_buffer: &mut silence_buffer,
                            audio_mode: &mut audio_mode,
                            mode_start: &mut mode_start,
                            last_transcription_time: &mut last_transcription_time,
                            consecutive_empty_reads: &mut consecutive_empty_reads,
                            stop_signal,
                        })
                    {
                        return;
                    }
                    break;
                }
                Err(e) => {
                    let error_str = e.to_string();
                    if error_str.contains("reset")
                        || error_str.contains("closed")
                        || error_str.contains("broken")
                    {
                        if !try_reconnect(ReconnectContext {
                            socket,
                            api_key,
                            model,
                            audio_buffer,
                            silence_buffer: &mut silence_buffer,
                            audio_mode: &mut audio_mode,
                            mode_start: &mut mode_start,
                            last_transcription_time: &mut last_transcription_time,
                            consecutive_empty_reads: &mut consecutive_empty_reads,
                            stop_signal,
                        }) {
                            return;
                        }
                    } else {
                        return;
                    }
                }
            }
        }

        if auto_stop && !pause_signal.load(Ordering::Relaxed) {
            let rms =
                f32::from_bits(crate::overlay::recording::CURRENT_RMS.load(Ordering::Relaxed));
            if rms > 0.015 {
                if !has_spoken {
                    first_speech = Some(Instant::now());
                }
                has_spoken = true;
                last_active = Instant::now();
            } else if has_spoken
                && first_speech.map(|t| t.elapsed().as_millis()).unwrap_or(0) >= 2000
                && last_active.elapsed().as_millis() > 800
            {
                stop_signal.store(true, Ordering::SeqCst);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Wait for final transcriptions after recording stops.
pub(super) fn wait_for_final_transcriptions(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    accumulated_text: &Arc<Mutex<String>>,
    preset: &Preset,
    streaming_hwnd: Option<HWND>,
) {
    let mut conclude_end = Instant::now() + Duration::from_millis(1200);
    let max_stop_time = Instant::now() + Duration::from_millis(5000);
    let extension = Duration::from_millis(700);

    println!("[GeminiLiveStream] Waiting for tail...");

    while Instant::now() < conclude_end && Instant::now() < max_stop_time {
        match socket.read() {
            Ok(tungstenite::Message::Text(msg)) => {
                if let Some(t) = parse_input_transcription(msg.as_str())
                    && !t.is_empty()
                {
                    if let Ok(mut txt) = accumulated_text.lock() {
                        txt.push_str(&t);
                        if let Some(h) = streaming_hwnd {
                            update_window_text(h, &txt);
                        }
                    }
                    conclude_end = Instant::now() + extension;
                }
            }
            Ok(tungstenite::Message::Binary(data)) => {
                if let Ok(s) = String::from_utf8(data.to_vec())
                    && let Some(t) = parse_input_transcription(&s)
                    && !t.is_empty()
                {
                    if let Ok(mut txt) = accumulated_text.lock() {
                        txt.push_str(&t);
                    }
                    if preset.auto_paste {
                        crate::overlay::utils::type_text_to_window(None, &t);
                    }
                    conclude_end = Instant::now() + extension;
                }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
    }
}
