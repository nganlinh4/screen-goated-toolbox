//! Real-time Gemini Live WebSocket streaming for audio transcription.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::utils::{
    calculate_result_rects, create_streaming_overlay, encode_wav, resample_to_16khz, WindowGuard,
};
use crate::api::realtime_audio::websocket::{
    connect_websocket, parse_input_transcription, send_audio_chunk, send_setup_message,
    set_socket_nonblocking, set_socket_short_timeout,
};
use crate::config::Preset;
use crate::overlay::recording::AUDIO_INITIALIZING;
use crate::overlay::result::update_window_text;
use crate::APP;

#[derive(Clone, Copy, PartialEq)]
enum AudioMode {
    Normal,
    Silence,
    CatchUp,
}

/// Attempt WebSocket reconnection with retry logic
fn try_reconnect(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    api_key: &str,
    audio_buffer: &Arc<Mutex<Vec<i16>>>,
    silence_buffer: &mut Vec<i16>,
    audio_mode: &mut AudioMode,
    mode_start: &mut Instant,
    last_transcription_time: &mut Instant,
    consecutive_empty_reads: &mut u32,
    stop_signal: &Arc<AtomicBool>,
) -> bool {
    let mut reconnect_buffer: Vec<i16> = Vec::new();
    let _ = socket.close(None);

    // Retry indefinitely until success or user stop
    loop {
        // Check if user stopped the recording while we were trying to reconnect
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
                if send_setup_message(&mut new_socket).is_err() {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
                if set_socket_nonblocking(&mut new_socket).is_err() {
                    let _ = new_socket.close(None);
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }

                // Final flush of buffer before resuming
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

/// Real-time record and stream to Gemini Live WebSocket.
/// Connects WebSocket FIRST, then streams audio in real-time during recording.
pub fn record_and_stream_gemini_live(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    pause_signal: Arc<AtomicBool>,
    abort_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    _target_window: Option<HWND>,
) {
    println!("[GeminiLiveStream] Starting real-time streaming...");

    // Create streaming overlay if enabled
    let streaming_hwnd = create_streaming_overlay(&preset);
    let _window_guard = streaming_hwnd.map(WindowGuard);

    let update_stream_text = |text: &str| {
        if let Some(h) = streaming_hwnd {
            update_window_text(h, text);
        }
    };

    let gemini_api_key = {
        let app = APP.lock().unwrap();
        app.config.gemini_api_key.clone()
    };

    if gemini_api_key.trim().is_empty() {
        eprintln!("[GeminiLiveStream] No API key");
        unsafe {
            let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        return;
    }

    // Connect WebSocket (Initializing state)
    AUDIO_INITIALIZING.store(true, Ordering::SeqCst);
    println!("[GeminiLiveStream] Connecting WebSocket...");

    let mut socket = match connect_websocket(&gemini_api_key) {
        Ok(s) => {
            println!("[GeminiLiveStream] Connected");
            s
        }
        Err(e) => {
            println!("[GeminiLiveStream] Connection failed: {}", e);
            AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
            unsafe {
                let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
    };

    if let Err(e) = send_setup_message(&mut socket) {
        println!("[GeminiLiveStream] Setup failed: {}", e);
        AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
        unsafe {
            let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        return;
    }

    let _ = set_socket_short_timeout(&mut socket);

    // Wait for setupComplete
    let setup_start = Instant::now();
    loop {
        match socket.read() {
            Ok(tungstenite::Message::Text(msg)) => {
                if msg.as_str().contains("setupComplete") {
                    break;
                }
            }
            Ok(tungstenite::Message::Binary(data)) => {
                if String::from_utf8(data.to_vec())
                    .map(|t| t.contains("setupComplete"))
                    .unwrap_or(false)
                {
                    break;
                }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if setup_start.elapsed() > Duration::from_secs(30) {
                    AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
                    unsafe {
                        let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
                unsafe {
                    let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                return;
            }
        }
    }

    let _ = set_socket_nonblocking(&mut socket);
    AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
    crate::overlay::recording::AUDIO_WARMUP_COMPLETE.store(true, Ordering::SeqCst);
    println!("[GeminiLiveStream] Setup complete, starting audio...");

    // Start audio capture
    let (device, config) = match setup_audio_device(&preset) {
        Some(d) => d,
        None => {
            let _ = socket.close(None);
            unsafe {
                let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
    };

    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;
    let audio_buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let full_audio_buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let accumulated_text: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let audio_buffer_clone = audio_buffer.clone();
    let full_buffer_clone = full_audio_buffer.clone();
    let pause_clone = pause_signal.clone();

    let stream = build_audio_stream(
        &device,
        &config,
        audio_buffer_clone,
        full_buffer_clone,
        pause_clone,
        sample_rate,
        channels,
    );

    let stream = match stream {
        Some(s) => s,
        None => {
            let _ = socket.close(None);
            unsafe {
                let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
    };

    if stream.play().is_err() {
        let _ = socket.close(None);
        unsafe {
            let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        return;
    }

    println!("[GeminiLiveStream] Streaming audio...");

    // Run the main streaming loop
    run_streaming_loop(
        &preset,
        &mut socket,
        &gemini_api_key,
        &audio_buffer,
        &accumulated_text,
        &stop_signal,
        &pause_signal,
        &abort_signal,
        overlay_hwnd,
        &update_stream_text,
    );

    drop(stream);
    println!("[GeminiLiveStream] Stopped, waiting for tail...");

    if !abort_signal.load(Ordering::SeqCst) {
        // Send remaining audio and wait for final transcriptions
        let remaining: Vec<i16> = std::mem::take(&mut *audio_buffer.lock().unwrap());
        if !remaining.is_empty() {
            let _ = send_audio_chunk(&mut socket, &remaining);
        }

        wait_for_final_transcriptions(
            &mut socket,
            &accumulated_text,
            &preset,
            streaming_hwnd,
        );
    }

    let _ = socket.close(None);
    let final_text = accumulated_text.lock().unwrap().clone();
    println!("[GeminiLiveStream] Result: '{}'", final_text);

    if abort_signal.load(Ordering::SeqCst) || final_text.is_empty() {
        unsafe {
            if IsWindow(Some(overlay_hwnd)).as_bool() {
                let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }

    // Save to history
    {
        let app = APP.lock().unwrap();
        app.history.save_audio(Vec::new(), final_text.clone());
    }

    let (rect, retrans) = calculate_result_rects(&preset);
    let final_wav = {
        let samples = full_audio_buffer.lock().unwrap();
        encode_wav(&samples, 16000, 1)
    };

    crate::overlay::process::show_audio_result(
        preset,
        final_text,
        final_wav,
        rect,
        retrans,
        overlay_hwnd,
        true, // is_streaming_result: disable auto-paste for Gemini Live
    );
}

/// Setup audio device based on preset configuration
fn setup_audio_device(preset: &Preset) -> Option<(cpal::Device, cpal::SupportedStreamConfig)> {
    #[cfg(target_os = "windows")]
    let host = if preset.audio_source == "device" {
        cpal::host_from_id(cpal::HostId::Wasapi).unwrap_or(cpal::default_host())
    } else {
        cpal::default_host()
    };
    #[cfg(not(target_os = "windows"))]
    let host = cpal::default_host();

    let device = if preset.audio_source == "device" {
        host.default_output_device()?
    } else {
        host.default_input_device()?
    };

    let config = if preset.audio_source == "device" {
        device
            .default_output_config()
            .or_else(|_| device.default_input_config())
    } else {
        device.default_input_config()
    };

    config.ok().map(|c| (device, c))
}

/// Build audio input stream for capturing
fn build_audio_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    full_buffer: Arc<Mutex<Vec<i16>>>,
    pause_signal: Arc<AtomicBool>,
    sample_rate: u32,
    channels: usize,
) -> Option<cpal::Stream> {
    match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let stream = device.build_input_stream(
                &config.clone().into(),
                move |data: &[f32], _: &_| {
                    if pause_signal.load(Ordering::Relaxed) {
                        return;
                    }
                    let mut rms = 0.0;
                    for &x in data {
                        rms += x * x;
                    }
                    rms = (rms / data.len() as f32).sqrt();
                    crate::overlay::recording::update_audio_viz(rms);

                    let mono: Vec<i16> = if channels > 1 {
                        data.chunks(channels)
                            .map(|c| {
                                ((c.iter().sum::<f32>() / channels as f32) * i16::MAX as f32) as i16
                            })
                            .collect()
                    } else {
                        data.iter().map(|&f| (f * i16::MAX as f32) as i16).collect()
                    };
                    let resampled = resample_to_16khz(&mono, sample_rate);
                    if let Ok(mut buf) = audio_buffer.lock() {
                        buf.extend(resampled.clone());
                    }
                    if let Ok(mut full) = full_buffer.lock() {
                        full.extend(resampled);
                    }
                },
                |e| eprintln!("Stream error: {}", e),
                None,
            );
            stream.ok()
        }
        cpal::SampleFormat::I16 => {
            let stream = device.build_input_stream(
                &config.clone().into(),
                move |data: &[i16], _: &_| {
                    if pause_signal.load(Ordering::Relaxed) {
                        return;
                    }
                    let mut rms = 0.0;
                    for &x in data {
                        let f = x as f32 / i16::MAX as f32;
                        rms += f * f;
                    }
                    rms = (rms / data.len() as f32).sqrt();
                    crate::overlay::recording::update_audio_viz(rms);

                    let mono: Vec<i16> = if channels > 1 {
                        data.chunks(channels)
                            .map(|c| {
                                (c.iter().map(|&s| s as i32).sum::<i32>() / c.len() as i32) as i16
                            })
                            .collect()
                    } else {
                        data.to_vec()
                    };
                    let resampled = resample_to_16khz(&mono, sample_rate);
                    if let Ok(mut buf) = audio_buffer.lock() {
                        buf.extend(resampled.clone());
                    }
                    if let Ok(mut full) = full_buffer.lock() {
                        full.extend(resampled);
                    }
                },
                |e| eprintln!("Stream error: {}", e),
                None,
            );
            stream.ok()
        }
        _ => None,
    }
}

/// Main streaming loop - sends audio and receives transcriptions
fn run_streaming_loop<F>(
    preset: &Preset,
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    api_key: &str,
    audio_buffer: &Arc<Mutex<Vec<i16>>>,
    accumulated_text: &Arc<Mutex<String>>,
    stop_signal: &Arc<AtomicBool>,
    pause_signal: &Arc<AtomicBool>,
    abort_signal: &Arc<AtomicBool>,
    overlay_hwnd: HWND,
    update_stream_text: &F,
) where
    F: Fn(&str),
{
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

    // Reconnection & CatchUp state
    let mut audio_mode = AudioMode::Normal;
    let mut mode_start = Instant::now();
    let mut silence_buffer: Vec<i16> = Vec::new();
    let mut last_transcription_time = Instant::now();
    let mut consecutive_empty_reads: u32 = 0;

    while !stop_signal.load(Ordering::SeqCst) && !abort_signal.load(Ordering::SeqCst) {
        if !preset.hide_recording_ui && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            break;
        }

        // State machine transitions
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
                        silence_buffer.drain(..).collect()
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

        // Read transcriptions
        loop {
            match socket.read() {
                Ok(tungstenite::Message::Text(msg)) => {
                    if let Some(t) = parse_input_transcription(msg.as_str()) {
                        if !t.is_empty() {
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
                }
                Ok(tungstenite::Message::Binary(data)) => {
                    if let Ok(s) = String::from_utf8(data.to_vec()) {
                        if let Some(t) = parse_input_transcription(&s) {
                            if !t.is_empty() {
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
                    }
                }
                Ok(tungstenite::Message::Close(_)) => {
                    if !try_reconnect(
                        socket,
                        api_key,
                        audio_buffer,
                        &mut silence_buffer,
                        &mut audio_mode,
                        &mut mode_start,
                        &mut last_transcription_time,
                        &mut consecutive_empty_reads,
                        stop_signal,
                    ) {
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
                    {
                        if !try_reconnect(
                            socket,
                            api_key,
                            audio_buffer,
                            &mut silence_buffer,
                            &mut audio_mode,
                            &mut mode_start,
                            &mut last_transcription_time,
                            &mut consecutive_empty_reads,
                            stop_signal,
                        ) {
                            return;
                        }
                    }
                    break;
                }
                Err(e) => {
                    let error_str = e.to_string();
                    if error_str.contains("reset")
                        || error_str.contains("closed")
                        || error_str.contains("broken")
                    {
                        if !try_reconnect(
                            socket,
                            api_key,
                            audio_buffer,
                            &mut silence_buffer,
                            &mut audio_mode,
                            &mut mode_start,
                            &mut last_transcription_time,
                            &mut consecutive_empty_reads,
                            stop_signal,
                        ) {
                            return;
                        }
                    } else {
                        return;
                    }
                }
            }
        }

        // Auto-stop: only active if not paused
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

/// Wait for final transcriptions after recording stops
fn wait_for_final_transcriptions(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    accumulated_text: &Arc<Mutex<String>>,
    preset: &Preset,
    streaming_hwnd: Option<HWND>,
) {
    // Adaptive wait: Start with 500ms
    // If we get data, extend by 600ms, up to max 4.0s
    let mut conclude_end = Instant::now() + Duration::from_millis(500);
    let max_stop_time = Instant::now() + Duration::from_millis(4000);
    let extension = Duration::from_millis(600);

    println!("[GeminiLiveStream] Waiting for tail...");

    while Instant::now() < conclude_end && Instant::now() < max_stop_time {
        match socket.read() {
            Ok(tungstenite::Message::Text(msg)) => {
                if let Some(t) = parse_input_transcription(msg.as_str()) {
                    if !t.is_empty() {
                        if let Ok(mut txt) = accumulated_text.lock() {
                            txt.push_str(&t);
                            if let Some(h) = streaming_hwnd {
                                update_window_text(h, &txt);
                            }
                        }
                        conclude_end = Instant::now() + extension;
                    }
                }
            }
            Ok(tungstenite::Message::Binary(data)) => {
                if let Ok(s) = String::from_utf8(data.to_vec()) {
                    if let Some(t) = parse_input_transcription(&s) {
                        if !t.is_empty() {
                            if let Ok(mut txt) = accumulated_text.lock() {
                                txt.push_str(&t);
                            }
                            if preset.auto_paste {
                                crate::overlay::utils::type_text_to_window(None, &t);
                            }
                            conclude_end = Instant::now() + extension;
                        }
                    }
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
