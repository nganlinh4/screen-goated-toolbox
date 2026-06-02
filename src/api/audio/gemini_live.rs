//! Real-time Gemini Live WebSocket streaming for audio transcription.

mod stream_loop;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::utils::{
    WindowGuard, calculate_result_rects, create_streaming_overlay, encode_wav, resample_to_16khz,
};
use crate::APP;
use crate::api::realtime_audio::websocket::{
    connect_websocket, send_audio_chunk, send_audio_stream_end, send_setup_message,
    set_socket_nonblocking, set_socket_short_timeout,
};
use crate::config::Preset;
use crate::model_config::{GEMINI_LIVE_API_MODEL_2_5, get_model_by_id};
use crate::overlay::recording::AUDIO_INITIALIZING;
use crate::overlay::result::update_window_text;

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
    let gemini_live_model = preset
        .blocks
        .iter()
        .find(|block| block.block_type == "audio")
        .and_then(|block| get_model_by_id(&block.model))
        .map(|config| config.full_name.clone())
        .unwrap_or_else(|| GEMINI_LIVE_API_MODEL_2_5.to_string());

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

    if let Err(e) = send_setup_message(&mut socket, &gemini_live_model) {
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
    stream_loop::run_streaming_loop(stream_loop::StreamingLoopContext {
        preset: &preset,
        socket: &mut socket,
        api_key: &gemini_api_key,
        model: &gemini_live_model,
        audio_buffer: &audio_buffer,
        accumulated_text: &accumulated_text,
        stop_signal: &stop_signal,
        pause_signal: &pause_signal,
        abort_signal: &abort_signal,
        overlay_hwnd,
        update_stream_text: &update_stream_text,
    });

    drop(stream);
    crate::overlay::screen_record::notify_external_audio_capture_released("gemini-live-stream");
    println!("[GeminiLiveStream] Stopped, waiting for tail...");

    if !abort_signal.load(Ordering::SeqCst) {
        // Send remaining audio and wait for final transcriptions
        let remaining: Vec<i16> = std::mem::take(&mut *audio_buffer.lock().unwrap());
        if !remaining.is_empty() {
            let _ = send_audio_chunk(&mut socket, &remaining);
        }
        let _ = send_audio_stream_end(&mut socket);

        stream_loop::wait_for_final_transcriptions(
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
