//! Main audio recording functions.

use std::io::Cursor;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::utils::calculate_result_rects;
use crate::config::Preset;

struct AudioComApartment;

impl AudioComApartment {
    fn initialize_mta() -> anyhow::Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|error| {
                    anyhow::anyhow!("Failed to initialize audio COM apartment: {error}")
                })?;
        }
        Ok(Self)
    }
}

impl Drop for AudioComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

/// Record audio and transcribe using configured API provider
pub fn record_audio_and_transcribe(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    pause_signal: Arc<AtomicBool>,
    abort_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    destination: crate::overlay::process::chain::AutoPasteDestination,
    recording_token: Arc<crate::overlay::result::ChainCancelToken>,
) {
    if recording_token.is_cancelled() {
        return;
    }
    let diagnostics = super::capture_diagnostics::CaptureDiagnostics::new(
        "record-then-process",
        &preset.audio_source,
    );
    diagnostics.controls(stop_signal.clone(), pause_signal.clone());
    let _com_apartment = match AudioComApartment::initialize_mta() {
        Ok(apartment) => apartment,
        Err(error) => {
            diagnostics.event("open_error", &format!("initialize COM: {error}"));
            crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
            return;
        }
    };

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
                    diagnostics.event("open_error", "No default output device found for loopback");
                    crate::overlay::recording::close_if_current_recording(
                        overlay_hwnd,
                        &recording_token,
                    );
                    return;
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            eprintln!("Error: Device capture not supported on this OS.");
            crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
            return;
        }
    } else {
        match crate::audio_input::microphone_device() {
            Some(d) => d,
            None => {
                diagnostics.event("open_error", "No default input device available");
                crate::overlay::recording::close_if_current_recording(
                    overlay_hwnd,
                    &recording_token,
                );
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
                    diagnostics.event("open_error", &format!("query config: {e}"));
                    crate::overlay::recording::close_if_current_recording(
                        overlay_hwnd,
                        &recording_token,
                    );
                    return;
                }
            },
        }
    } else {
        match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                diagnostics.event("open_error", &format!("query config: {e}"));
                crate::overlay::recording::close_if_current_recording(
                    overlay_hwnd,
                    &recording_token,
                );
                return;
            }
        }
    };

    let spec = transcription_wav_spec();

    let (tx, rx) = mpsc::channel::<Vec<i16>>();
    let budget = super::retention::CaptureBudget::new(spec.sample_rate, spec.channels);
    let limit_exceeded = Arc::new(AtomicBool::new(false));
    let callback_limit_exceeded = limit_exceeded.clone();
    let callback_stop = stop_signal.clone();
    let callback_diagnostics = diagnostics.clone();
    let callback_token = recording_token.clone();
    let stream_res = super::pcm_capture::build(
        &device,
        &config,
        super::pcm_capture::CaptureOptions {
            // Downmix and resample in the callback: speech providers consume
            // 16 kHz mono, and native-rate stereo would be ~6x larger inline.
            interleaved: false,
            stop: stop_signal.clone(),
            pause: pause_signal_audio.clone(),
            diagnostics: callback_diagnostics,
        },
        move |pcm| {
            if !should_capture(&callback_stop, &callback_token) {
                return;
            }
            if !budget.claim(pcm.len()) {
                callback_limit_exceeded.store(true, Ordering::SeqCst);
                crate::overlay::recording::stop_if_current_recording(&callback_token);
                return;
            }
            let rms = crate::api::gemini_transcribe::compute_i16_rms(&pcm);
            crate::overlay::recording::update_viz_if_current_recording(&callback_token, rms);
            let _ = tx.send(pcm);
        },
        |_| {},
    );

    if let Err(e) = stream_res {
        diagnostics.event("open_error", &format!("build stream: {e}"));
        eprintln!("Failed to build stream: {}", e);
        crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
        return;
    }
    let stream = stream_res.unwrap();

    if let Err(e) = stream.play() {
        diagnostics.event("open_error", &format!("play stream: {e}"));
        eprintln!("Failed to play stream: {}", e);
        crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
        return;
    }

    let mut collected_samples: Vec<i16> = Vec::new();
    diagnostics.event("playing", "16 kHz mono recording");

    // Auto-stop state
    let auto_stop_enabled = preset.auto_stop_recording;
    let mut has_spoken = false;
    let mut first_speech_time: Option<std::time::Instant> = None;
    let mut last_active_time = std::time::Instant::now();

    const SILENCE_LIMIT_MS: u128 = 800;
    const MIN_RECORDING_MS: u128 = 2000;

    while should_capture(&stop_signal, &recording_token) {
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

            if super::activity::auto_stop_activity(current_rms) {
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
                        crate::overlay::recording::stop_if_current_recording(&recording_token);
                    }
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
        if !preset.hide_recording_ui && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            return;
        }
    }

    if let Err(err) = stream.pause() {
        diagnostics.event("pause_error", &err.to_string());
        eprintln!("[Recording] Failed to pause audio stream before drop: {err}");
    }
    std::thread::sleep(std::time::Duration::from_millis(80));
    drop(stream);
    if recording_token.is_cancelled() {
        crate::overlay::screen_record::notify_external_audio_capture_released(
            "record-then-process",
        );
        return;
    }
    diagnostics.event(
        "stopped",
        &format!(
            "abort={} paused={} collected_samples={}",
            abort_signal.load(Ordering::Relaxed),
            pause_signal_audio.load(Ordering::Relaxed),
            collected_samples.len()
        ),
    );
    crate::overlay::screen_record::notify_external_audio_capture_released("record-then-process");
    std::thread::sleep(std::time::Duration::from_millis(40));

    if abort_signal.load(Ordering::SeqCst) || recording_token.is_cancelled() {
        crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
        return;
    }

    if limit_exceeded.load(Ordering::SeqCst) {
        diagnostics.event("recording_limit", "maximum recording duration exceeded");
        crate::overlay::recording::notify_if_current_recording(
            &recording_token,
            "Recording stopped at the 10-minute limit.",
        );
    }

    while let Ok(chunk) = rx.try_recv() {
        collected_samples.extend(chunk);
    }

    let samples = collected_samples;

    if samples.is_empty() {
        println!("Warning: Recorded audio buffer is empty.");
        crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
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

    if recording_token.is_cancelled() {
        return;
    }

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
            crate::overlay::preset_wheel::show_preset_wheel_id("audio", audio_mode, cursor_pos);

        if let Some(Some(selected_preset)) = recording_token.run_if_active(|| {
            let selected = selected.as_deref()?;
            let mut app = crate::APP.lock().ok()?;
            let idx = app
                .config
                .presets
                .iter()
                .position(|preset| preset.id == selected)?;
            let selected_preset = app.config.presets[idx].clone();
            app.config.active_preset_idx = idx;
            Some(selected_preset)
        }) {
            selected_preset
        } else {
            crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
            return;
        }
    } else {
        preset.clone()
    };

    if abort_signal.load(Ordering::SeqCst) || recording_token.is_cancelled() {
        crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
        return;
    }

    process_recorded_audio(
        working_preset,
        wav_data,
        overlay_hwnd,
        destination,
        Some(recording_token),
    );
}

/// Record-then-process audio is captured as 16 kHz mono PCM, the format the
/// shared capture converter produces and every speech provider accepts.
fn transcription_wav_spec() -> hound::WavSpec {
    hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    }
}

fn should_capture(
    stop_signal: &AtomicBool,
    recording_token: &crate::overlay::result::ChainCancelToken,
) -> bool {
    !stop_signal.load(Ordering::SeqCst) && !recording_token.is_cancelled()
}

/// Process an existing audio file (WAV data) using a specific preset.
/// Used for drag-and-drop audio file processing without recording.
pub fn process_audio_file_request(
    preset: Preset,
    wav_data: Vec<u8>,
    destination: crate::overlay::process::chain::AutoPasteDestination,
) {
    process_recorded_audio(
        preset,
        wav_data,
        HWND(std::ptr::null_mut()),
        destination,
        None,
    );
}

fn process_recorded_audio(
    preset: Preset,
    wav_data: Vec<u8>,
    overlay_hwnd: HWND,
    destination: crate::overlay::process::chain::AutoPasteDestination,
    recording_token: Option<Arc<crate::overlay::result::ChainCancelToken>>,
) {
    if recording_token
        .as_ref()
        .is_some_and(|token| token.is_cancelled())
    {
        return;
    }
    let initial_audio = crate::overlay::process::chain::initial_audio_blocks(
        &preset.blocks,
        &preset.block_connections,
        preset.graph_explicit,
    );
    if initial_audio.is_empty() {
        if preset
            .blocks
            .first()
            .is_some_and(|block| block.block_type == "input_adapter")
        {
            let app = crate::APP.lock().unwrap();
            if let Some(token) = &recording_token {
                app.history.save_audio_shared_cancellable(
                    wav_data.clone().into(),
                    String::new(),
                    token.clone(),
                );
            } else {
                app.history.save_audio(wav_data.clone(), String::new());
            }
        } else {
            if let Some(token) = recording_token.as_ref() {
                crate::overlay::recording::notify_if_current_recording(
                    token,
                    "Audio preset has no reachable audio processing block.",
                );
                crate::overlay::recording::close_if_current_recording(overlay_hwnd, token);
            } else {
                crate::overlay::auto_copy_badge::show_error_notification(
                    "Audio preset has no reachable audio processing block.",
                );
            }
            return;
        }
    }
    let (rect, retranslate_rect) = calculate_result_rects(&preset);
    crate::overlay::process::show_audio_result(
        preset,
        String::new(),
        wav_data,
        rect,
        retranslate_rect,
        overlay_hwnd,
        crate::overlay::process::AudioResultMode::Recorded(recording_token, destination),
    );
}

#[cfg(test)]
mod session_capture_tests;
