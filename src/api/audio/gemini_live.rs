//! Real-time Gemini Live WebSocket streaming for audio transcription.

#[cfg(test)]
mod live_tests;
mod pending_audio;
mod speech_boundaries;
mod stream_loop;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use windows::Win32::Foundation::*;

use super::failure_notice::report_audio_failure;
use super::utils::{WindowGuard, calculate_result_rects, create_streaming_overlay};
use crate::APP;
use crate::api::gemini_live::ready_session::{ConnectedLiveSocket, OpenOptions, ReadyLiveSession};
use crate::config::Preset;
use crate::model_config::{GEMINI_LIVE_API_MODEL_2_5, get_model_by_id};
use crate::overlay::result::update_window_text;

fn open_ready_session(
    api_key: &str,
    model: &str,
    vocabulary: &[String],
    resumption_handle: Option<&str>,
    cancelled: impl Fn() -> bool,
) -> anyhow::Result<ReadyLiveSession> {
    let setup =
        crate::api::gemini_transcribe::build_live_setup(model, vocabulary, resumption_handle);
    ConnectedLiveSocket::connect_cancellable(api_key, &cancelled)?.activate_with(
        setup,
        OpenOptions {
            setup_timeout: Duration::from_secs(30),
            ..OpenOptions::default()
        },
        cancelled,
    )
}

/// Real-time record and stream to Gemini Live WebSocket.
/// Connects WebSocket FIRST, then streams audio in real-time during recording.
pub(crate) fn record_and_stream_gemini_live(
    preset: Preset,
    stop_signal: Arc<AtomicBool>,
    pause_signal: Arc<AtomicBool>,
    abort_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    auto_paste: Arc<crate::overlay::utils::StreamingAutoPaste>,
    recording_token: Arc<crate::overlay::result::ChainCancelToken>,
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

    let (gemini_api_key, custom_vocabulary) = {
        let app = APP.lock().unwrap();
        (
            app.config.gemini_api_key.clone(),
            app.config.realtime_custom_vocabulary.clone(),
        )
    };
    let gemini_live_model = crate::overlay::process::chain::initial_audio_blocks(
        &preset.blocks,
        &preset.block_connections,
        preset.graph_explicit,
    )
    .first()
    .and_then(|index| preset.blocks.get(*index))
    .and_then(|block| get_model_by_id(&block.model))
    .map(|config| config.full_name.clone())
    .unwrap_or_else(|| GEMINI_LIVE_API_MODEL_2_5.to_string());

    if gemini_api_key.trim().is_empty() {
        report_audio_failure(&recording_token, &anyhow::anyhow!("NO_API_KEY:gemini"));
        crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
        return;
    }

    // Connect WebSocket (Initializing state)
    crate::overlay::recording::set_initializing_if_current_recording(&recording_token, true);
    println!("[GeminiLiveStream] Connecting WebSocket...");

    let mut session = match open_ready_session(
        &gemini_api_key,
        &gemini_live_model,
        &custom_vocabulary,
        None,
        || stop_signal.load(Ordering::SeqCst) || abort_signal.load(Ordering::SeqCst),
    ) {
        Ok(session) => session,
        Err(e) => {
            if !stop_signal.load(Ordering::SeqCst) && !abort_signal.load(Ordering::SeqCst) {
                report_audio_failure(&recording_token, &e);
            }
            crate::overlay::recording::set_initializing_if_current_recording(
                &recording_token,
                false,
            );
            crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
            return;
        }
    };

    if stop_signal.load(Ordering::SeqCst) || abort_signal.load(Ordering::SeqCst) {
        let _ = session.close();
        crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
        return;
    }

    crate::overlay::recording::mark_ready_if_current_recording(&recording_token);
    println!("[GeminiLiveStream] Setup complete, starting audio...");

    // Start audio capture
    let (device, config) = match setup_audio_device(&preset) {
        Some(d) => d,
        None => {
            let _ = session.close();
            crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
            return;
        }
    };

    let audio_buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let full_audio_buffer = Arc::new(Mutex::new(super::retention::AudioAttachment::default()));
    let accumulated_text: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let mut transcribe_text = crate::api::gemini_transcribe::TranscriptState::default();
    let audio_buffer_clone = audio_buffer.clone();
    let full_buffer_clone = full_audio_buffer.clone();
    let pause_clone = pause_signal.clone();

    let stream = build_audio_stream(
        &device,
        &config,
        audio_buffer_clone,
        full_buffer_clone,
        pause_clone,
        (stop_signal.clone(), recording_token.clone()),
        &preset.audio_source,
    );

    let stream = match stream {
        Some(s) => s,
        None => {
            let _ = session.close();
            crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
            return;
        }
    };

    if stop_signal.load(Ordering::SeqCst) || abort_signal.load(Ordering::SeqCst) {
        let _ = session.close();
        crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
        return;
    }

    if stream.play().is_err() {
        let _ = session.close();
        crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
        return;
    }

    println!("[GeminiLiveStream] Streaming audio...");

    // Run the main streaming loop
    let stream_failure = stream_loop::run_streaming_loop(stream_loop::StreamingLoopContext {
        preset: &preset,
        session: &mut session,
        reopen: &mut |handle, cancelled| {
            open_ready_session(
                &gemini_api_key,
                &gemini_live_model,
                &custom_vocabulary,
                handle,
                cancelled,
            )
        },
        model: &gemini_live_model,
        audio_buffer: &audio_buffer,
        accumulated_text: &accumulated_text,
        transcribe_text: &mut transcribe_text,
        auto_paste: &auto_paste,
        stop_signal: &stop_signal,
        pause_signal: &pause_signal,
        abort_signal: &abort_signal,
        overlay_hwnd,
        update_stream_text: &update_stream_text,
        clock: &stream_loop::SystemClock,
    });

    if let Some(error) = &stream_failure {
        report_audio_failure(&recording_token, error);
    }
    auto_paste.begin_drain();
    drop(stream);
    crate::overlay::screen_record::notify_external_audio_capture_released("gemini-live-stream");
    println!("[GeminiLiveStream] Stopped, waiting for tail...");

    if !abort_signal.load(Ordering::SeqCst) {
        // Send remaining audio and wait for final transcriptions
        let remaining: Vec<i16> = std::mem::take(&mut *audio_buffer.lock().unwrap());
        if !remaining.is_empty() {
            let _ = session.send_audio_pcm(&remaining, 16_000);
        }
        let _ = session.end_audio_stream();

        stream_loop::wait_for_final_transcriptions(stream_loop::FinalTranscriptionsContext {
            session: &mut session,
            accumulated_text: &accumulated_text,
            transcribe_text: &mut transcribe_text,
            auto_paste: &auto_paste,
            uses_interim_transcripts: crate::api::gemini_transcribe::is_live_transcribe(
                &gemini_live_model,
            ),
            streaming_hwnd,
            abort_signal: &abort_signal,
            clock: &stream_loop::SystemClock,
        });
    }

    if !abort_signal.load(Ordering::SeqCst)
        && crate::api::gemini_transcribe::is_live_transcribe(&gemini_live_model)
    {
        let delta = transcribe_text.finish_pending();
        if !delta.is_empty() {
            auto_paste.final_text(&delta);
        }
        *accumulated_text.lock().unwrap() = transcribe_text.committed().to_owned();
    }
    let _ = session.close();
    auto_paste.finish();
    let final_text = accumulated_text.lock().unwrap().clone();

    if abort_signal.load(Ordering::SeqCst)
        || recording_token.is_cancelled()
        || final_text.is_empty()
    {
        crate::overlay::recording::close_if_current_recording(overlay_hwnd, &recording_token);
        return;
    }

    // Save to history
    {
        let app = APP.lock().unwrap();
        app.history.save_audio_shared_cancellable(
            Arc::from([]),
            final_text.clone(),
            recording_token.clone(),
        );
    }

    let (rect, retrans) = calculate_result_rects(&preset);
    let final_wav = {
        let samples = full_audio_buffer.lock().unwrap();
        samples.wav()
    };

    crate::overlay::process::show_audio_result(
        preset,
        final_text,
        final_wav,
        rect,
        retrans,
        overlay_hwnd,
        crate::overlay::process::AudioResultMode::Streaming(recording_token),
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
        crate::audio_input::microphone_device()?
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

/// Build capture through the same format, diagnostic, and resampling adapter.
fn build_audio_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    full_buffer: Arc<Mutex<super::retention::AudioAttachment>>,
    pause_signal: Arc<AtomicBool>,
    session: (
        Arc<AtomicBool>,
        Arc<crate::overlay::result::ChainCancelToken>,
    ),
    source: &str,
) -> Option<cpal::Stream> {
    let (stop_signal, recording_token) = session;
    let diagnostics = super::capture_diagnostics::CaptureDiagnostics::new("preset-live", source);
    let callback_diagnostics = diagnostics.clone();
    let result = super::pcm_capture::build(
        device,
        config,
        super::pcm_capture::CaptureOptions {
            interleaved: false,
            stop: stop_signal,
            pause: pause_signal,
            diagnostics: diagnostics.clone(),
        },
        move |pcm| {
            crate::overlay::recording::update_viz_if_current_recording(
                &recording_token,
                crate::api::gemini_transcribe::compute_i16_rms(&pcm),
            );
            if let Ok(mut buffer) = audio_buffer.lock() {
                let dropped = super::retention::append_pending(&mut buffer, &pcm);
                if dropped > 0 {
                    callback_diagnostics
                        .event("pending_overflow", &format!("dropped_samples={dropped}"));
                }
            }
            if let Ok(mut full) = full_buffer.lock()
                && full.extend(&pcm)
            {
                callback_diagnostics.event(
                    "attachment_limit",
                    "audio attachment omitted after 600 seconds; transcription continues",
                );
            }
        },
        |_| {},
    );
    match result {
        Ok(stream) => {
            diagnostics.event("configured_stream", "target_rate=16000 mono");
            Some(stream)
        }
        Err(error) => {
            diagnostics.event("open_error", &format!("{error:#}"));
            None
        }
    }
}
