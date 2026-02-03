//! Main audio recording functions and Parakeet streaming.

use std::io::Cursor;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::transcription::execute_audio_processing_logic;
use super::utils::{calculate_result_rects, create_streaming_overlay, encode_wav};
use crate::config::Preset;
use crate::overlay::result::update_window_text;

/// Record and stream audio using Parakeet (local speech recognition)
pub fn record_and_stream_parakeet(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    pause_signal: Arc<AtomicBool>,
    abort_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    _target_window: Option<HWND>,
) {
    let accumulated_text: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let full_audio_buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let acc_clone = accumulated_text.clone();
    let preset_clone = preset.clone();

    // Create streaming overlay if enabled
    let streaming_hwnd = create_streaming_overlay(&preset);
    let streaming_hwnd_clone = streaming_hwnd;

    let callback = move |text: String| {
        if !text.is_empty() {
            if let Ok(mut txt) = acc_clone.lock() {
                txt.push_str(&text);

                // Update streaming window if active
                if let Some(h) = streaming_hwnd_clone {
                    update_window_text(h, &txt);
                }
            }
            // Real-time typing
            if preset_clone.auto_paste {
                crate::overlay::utils::type_text_to_window(None, &text);
            }
        }
    };

    println!("[ParakeetStream] Starting Parakeet session...");

    // Run Parakeet session (blocks until stopped)
    let res = crate::api::realtime_audio::parakeet::run_parakeet_session(
        stop_signal.clone(),
        pause_signal.clone(),
        Some(full_audio_buffer.clone()),
        Some(overlay_hwnd),
        preset.hide_recording_ui,
        true, // Enable download badge
        Some(preset.audio_source.clone()),
        preset.auto_stop_recording,
        callback,
    );

    // Close streaming window immediately after recording stops
    if let Some(h) = streaming_hwnd {
        unsafe {
            let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    if let Err(e) = res {
        eprintln!("[ParakeetStream] Error: {:?}", e);
    }

    // Check for abort
    if abort_signal.load(Ordering::SeqCst) {
        unsafe {
            if IsWindow(Some(overlay_hwnd)).as_bool() {
                let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }

    let final_text = accumulated_text.lock().unwrap().clone();
    println!("[ParakeetStream] Final Result: '{}'", final_text);

    if final_text.is_empty() {
        unsafe {
            if IsWindow(Some(overlay_hwnd)).as_bool() {
                let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }

    let final_wav = {
        let samples = full_audio_buffer.lock().unwrap();
        encode_wav(&samples, 16000, 1)
    };

    // Save history
    {
        let app = crate::APP.lock().unwrap();
        app.history
            .save_audio(final_wav.clone(), final_text.clone());
    }

    let (rect, retrans) = calculate_result_rects(&preset);

    crate::overlay::process::show_audio_result(
        preset,
        final_text,
        final_wav,
        rect,
        retrans,
        overlay_hwnd,
        true, // is_streaming_result: disable auto-paste
    );
}

/// Record audio and transcribe using configured API provider
pub fn record_audio_and_transcribe(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    pause_signal: Arc<AtomicBool>,
    abort_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
) {
    let pause_signal_audio = pause_signal.clone();

    #[cfg(target_os = "windows")]
    let host = if preset.audio_source == "device" {
        cpal::host_from_id(cpal::HostId::Wasapi).unwrap_or(cpal::default_host())
    } else {
        cpal::default_host()
    };
    #[cfg(not(target_os = "windows"))]
    let host = cpal::default_host();

    let device = if preset.audio_source == "device" {
        #[cfg(target_os = "windows")]
        {
            match host.default_output_device() {
                Some(d) => d,
                None => {
                    eprintln!("Error: No default output device found for loopback.");
                    unsafe {
                        let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                    return;
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            eprintln!("Error: Device capture not supported on this OS.");
            unsafe {
                let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
    } else {
        match host.default_input_device() {
            Some(d) => d,
            None => {
                eprintln!("Error: No input device available.");
                unsafe {
                    let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                return;
            }
        }
    };

    let config = if preset.audio_source == "device" {
        match device.default_output_config() {
            Ok(c) => c,
            Err(_) => match device.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to get audio config: {}", e);
                    unsafe {
                        let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                    return;
                }
            },
        }
    } else {
        match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to get audio config: {}", e);
                unsafe {
                    let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                return;
            }
        }
    };

    let sample_rate = config.sample_rate();
    let channels = config.channels();

    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let err_fn = |err| eprintln!("Audio stream error: {}", err);

    // Threshold for "meaningful audio"
    const WARMUP_RMS_THRESHOLD: f32 = 0.001;

    let pause_signal_builder = pause_signal_audio.clone();
    let stream_res = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &_| {
                if !pause_signal_builder.load(Ordering::Relaxed) {
                    let _ = tx.send(data.to_vec());
                    let mut rms = 0.0;
                    for &x in data {
                        rms += x * x;
                    }
                    rms = (rms / data.len() as f32).sqrt();
                    crate::overlay::recording::update_audio_viz(rms);

                    if rms > WARMUP_RMS_THRESHOLD {
                        crate::overlay::recording::AUDIO_WARMUP_COMPLETE
                            .store(true, Ordering::SeqCst);
                    }
                }
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &_| {
                if !pause_signal_builder.load(Ordering::Relaxed) {
                    let f32_data: Vec<f32> =
                        data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    let _ = tx.send(f32_data);
                    let mut rms = 0.0;
                    for &x in data {
                        let f = x as f32 / i16::MAX as f32;
                        rms += f * f;
                    }
                    rms = (rms / data.len() as f32).sqrt();
                    crate::overlay::recording::update_audio_viz(rms);

                    if rms > WARMUP_RMS_THRESHOLD {
                        crate::overlay::recording::AUDIO_WARMUP_COMPLETE
                            .store(true, Ordering::SeqCst);
                    }
                }
            },
            err_fn,
            None,
        ),
        _ => {
            eprintln!(
                "Unsupported audio sample format: {:?}",
                config.sample_format()
            );
            Err(cpal::BuildStreamError::StreamConfigNotSupported)
        }
    };

    if let Err(e) = stream_res {
        eprintln!("Failed to build stream: {}", e);
        unsafe {
            let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        return;
    }
    let stream = stream_res.unwrap();

    if let Err(e) = stream.play() {
        eprintln!("Failed to play stream: {}", e);
        unsafe {
            let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        return;
    }

    let mut collected_samples: Vec<f32> = Vec::new();

    // Auto-stop state
    let auto_stop_enabled = preset.auto_stop_recording;
    let mut has_spoken = false;
    let mut first_speech_time: Option<std::time::Instant> = None;
    let mut last_active_time = std::time::Instant::now();

    const NOISE_THRESHOLD: f32 = 0.015;
    const SILENCE_LIMIT_MS: u128 = 800;
    const MIN_RECORDING_MS: u128 = 2000;

    while !stop_signal.load(Ordering::SeqCst) {
        while let Ok(chunk) = rx.try_recv() {
            collected_samples.extend(chunk);
        }

        // Auto-stop logic
        if auto_stop_enabled
            && !stop_signal.load(Ordering::Relaxed)
            && !pause_signal_audio.load(Ordering::Relaxed)
        {
            let rms_bits = crate::overlay::recording::CURRENT_RMS.load(Ordering::Relaxed);
            let current_rms = f32::from_bits(rms_bits);

            if current_rms > NOISE_THRESHOLD {
                if !has_spoken {
                    first_speech_time = Some(std::time::Instant::now());
                }
                has_spoken = true;
                last_active_time = std::time::Instant::now();
            } else if has_spoken {
                let recording_duration = first_speech_time
                    .map(|t| t.elapsed().as_millis())
                    .unwrap_or(0);
                if recording_duration >= MIN_RECORDING_MS {
                    let silence_duration = last_active_time.elapsed().as_millis();
                    if silence_duration > SILENCE_LIMIT_MS {
                        stop_signal.store(true, Ordering::SeqCst);
                    }
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
        if !preset.hide_recording_ui && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            return;
        }
    }

    drop(stream);

    if abort_signal.load(Ordering::SeqCst) {
        unsafe {
            if IsWindow(Some(overlay_hwnd)).as_bool() {
                let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }

    while let Ok(chunk) = rx.try_recv() {
        collected_samples.extend(chunk);
    }

    let samples: Vec<i16> = collected_samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect();

    if samples.is_empty() {
        println!("Warning: Recorded audio buffer is empty.");
        unsafe {
            let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        return;
    }

    let mut wav_cursor = Cursor::new(Vec::new());
    {
        let mut writer =
            hound::WavWriter::new(&mut wav_cursor, spec).expect("Failed to create memory writer");
        for sample in &samples {
            writer
                .write_sample(*sample)
                .expect("Failed to write sample");
        }
        writer.finalize().expect("Failed to finalize WAV");
    }
    let wav_data = wav_cursor.into_inner();

    // For MASTER presets, show the wheel BEFORE transcription
    let working_preset = if preset.is_master {
        let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let cursor_pos = POINT {
            x: screen_w / 2,
            y: screen_h / 2,
        };

        let audio_mode = Some(preset.audio_source.as_str());
        let selected =
            crate::overlay::preset_wheel::show_preset_wheel("audio", audio_mode, cursor_pos);

        if let Some(idx) = selected {
            let mut app = crate::APP.lock().unwrap();
            app.config.active_preset_idx = idx;
            app.config.presets[idx].clone()
        } else {
            unsafe {
                if IsWindow(Some(overlay_hwnd)).as_bool() {
                    let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            return;
        }
    } else {
        preset.clone()
    };

    let wav_data_for_history = wav_data.clone();
    let transcription_result = execute_audio_processing_logic(&working_preset, wav_data);

    if abort_signal.load(Ordering::SeqCst) {
        unsafe {
            if IsWindow(Some(overlay_hwnd)).as_bool() {
                let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }

    match transcription_result {
        Ok(transcription_text) => {
            let wav_data_for_overlay = wav_data_for_history.clone();

            // Save history
            {
                let app = crate::APP.lock().unwrap();
                app.history
                    .save_audio(wav_data_for_history, transcription_text.clone());
            }

            let (rect, retranslate_rect) = calculate_result_rects(&working_preset);

            crate::overlay::process::show_audio_result(
                working_preset,
                transcription_text,
                wav_data_for_overlay,
                rect,
                retranslate_rect,
                overlay_hwnd,
                false, // is_streaming_result: standard transcription (allow paste)
            );
        }
        Err(e) => {
            eprintln!("Transcription error: {}", e);
            unsafe {
                if IsWindow(Some(overlay_hwnd)).as_bool() {
                    let _ = PostMessageW(Some(overlay_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }
    }
}

/// Process an existing audio file (WAV data) using a specific preset.
/// Used for drag-and-drop audio file processing without recording.
pub fn process_audio_file_request(preset: Preset, wav_data: Vec<u8>) {
    let processing_result = execute_audio_processing_logic(&preset, wav_data.clone());

    match processing_result {
        Ok(result_text) => {
            // Save history
            {
                let app = crate::APP.lock().unwrap();
                app.history
                    .save_audio(wav_data.clone(), result_text.clone());
            }

            let (rect, retranslate_rect) = calculate_result_rects(&preset);

            crate::overlay::process::show_audio_result(
                preset,
                result_text,
                wav_data,
                rect,
                retranslate_rect,
                HWND(std::ptr::null_mut()),
                false, // is_streaming_result: file processing (allow paste)
            );
        }
        Err(e) => {
            eprintln!("Audio file processing error: {}", e);
        }
    }
}
