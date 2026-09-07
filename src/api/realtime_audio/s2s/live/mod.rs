use super::*;

mod continuous;
mod drain;
mod lifecycle_adapter;
mod text_state;

use continuous::run_live_translate_continuous;

pub fn run_gemini_live_s2s(
    audio_source: String,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    translation_hwnd: Option<HWND>,
    state: SharedRealtimeState,
    session_id: u64,
) -> Result<()> {
    let settings = load_settings()?;
    crate::log_info!(
        "[{}] start model={} target_language={} audio_source={}",
        settings.mode.log_tag(),
        settings.model,
        settings.target_language,
        audio_source
    );
    apply_tts_speed_for_s2s(&settings.speed);
    let audio_buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
    let pause = Arc::new(AtomicBool::new(false));
    let selected_pid = SELECTED_APP_PID.load(Ordering::SeqCst);
    let _stream = if audio_source == "device" {
        let selected_pid = if selected_pid == 0 {
            crate::overlay::realtime_webview::app_selection::show_audio_app_selector_overlay();
            wait_for_selected_app(stop_signal.clone(), session_id)
        } else {
            Some(selected_pid)
        };
        if let Some(selected_pid) = selected_pid {
            let capture = start_per_app_capture(
                selected_pid,
                audio_buffer.clone(),
                stop_signal.clone(),
                pause.clone(),
            )?;
            Some(
                spawn_s2s_per_app_audio_pid_refresh(
                    selected_pid,
                    capture,
                    audio_buffer.clone(),
                    stop_signal.clone(),
                    pause.clone(),
                    session_id,
                    settings.mode,
                )?
                .into(),
            )
        } else {
            return Err(anyhow::anyhow!(
                "S2S device mode needs a selected app to avoid capturing its own translated audio"
            ));
        }
    } else {
        Some(start_mic_capture_resilient(
            audio_buffer.clone(),
            stop_signal.clone(),
            pause.clone(),
        )?)
    };
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(
            crate::api::realtime_audio::state::TranscriptionMethod::GeminiLiveS2s,
        );
    }

    let (event_tx, event_rx) = mpsc::channel::<S2sEvent>();
    let context_memory = Arc::new(Mutex::new(S2sContextMemory::default()));
    let coordinator_stop = stop_signal.clone();
    let coordinator_state = state.clone();
    let coordinator_overlay = overlay_hwnd.0 as isize;
    let coordinator_translation = translation_hwnd.map(|hwnd| hwnd.0 as isize);
    let coordinator_context = context_memory.clone();
    let mode = settings.mode;
    std::thread::spawn(move || {
        coordinate_output(
            event_rx,
            coordinator_stop,
            HWND(coordinator_overlay as *mut std::ffi::c_void),
            coordinator_translation.map(|hwnd| HWND(hwnd as *mut std::ffi::c_void)),
            coordinator_state,
            coordinator_context,
            mode,
        );
    });

    if settings.mode == S2sMode::LiveTranslate {
        return run_live_translate_continuous(
            audio_buffer,
            stop_signal,
            event_tx,
            overlay_hwnd,
            session_id,
            settings,
        );
    }

    let adaptive_vad = Arc::new(Mutex::new(AdaptiveS2sVadState::default()));
    let mut segment_senders = Vec::with_capacity(SESSION_COUNT);
    for session_index in 0..SESSION_COUNT {
        let (segment_tx, segment_rx) = mpsc::channel::<Segment>();
        segment_senders.push(segment_tx);
        let worker_stop = stop_signal.clone();
        let worker_events = event_tx.clone();
        let worker_settings = settings.clone();
        let worker_context = context_memory.clone();
        let worker_adaptive_vad = adaptive_vad.clone();
        std::thread::spawn(move || {
            session_worker(
                session_index,
                segment_rx,
                worker_events,
                worker_stop,
                worker_settings,
                worker_context,
                worker_adaptive_vad,
            );
        });
    }

    run_vad_loop(
        audio_buffer,
        stop_signal,
        segment_senders,
        event_tx,
        overlay_hwnd,
        session_id,
        adaptive_vad,
        settings.mode,
    );

    Ok(())
}

fn spawn_s2s_per_app_audio_pid_refresh(
    initial_pid: u32,
    capture: super::super::capture::CaptureWorker,
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    session_id: u64,
    mode: S2sMode,
) -> Result<super::super::capture::CaptureWorker> {
    super::super::capture::CaptureWorker::spawn("sgt-per-app-pid-monitor", move |monitor_stop| {
        let mut capture = Some(capture);
        let started = Instant::now();
        let mut last_observed_samples = 0usize;
        let mut refresh_pending = true;
        while !stop_signal.load(Ordering::Relaxed)
            && !monitor_stop.load(Ordering::Acquire)
            && !is_stale_session(session_id)
            && !AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
            && !TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
        {
            std::thread::sleep(Duration::from_millis(100));
            if monitor_stop.load(Ordering::Acquire)
                || stop_signal.load(Ordering::Relaxed)
                || is_stale_session(session_id)
                || AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
                || TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
            {
                break;
            }
            if !refresh_pending {
                continue;
            }
            let observed_samples = audio_buffer.lock().map(|buffer| buffer.len()).unwrap_or(0);
            if observed_samples > last_observed_samples + FRAME_SAMPLES
                || started.elapsed() >= Duration::from_secs(10)
            {
                refresh_pending = false;
                continue;
            }
            last_observed_samples = observed_samples;
            let Some(refreshed_pid) =
                crate::overlay::realtime_webview::app_selection::refresh_selected_audio_capture_pid(
                )
            else {
                continue;
            };
            if refreshed_pid == 0 || refreshed_pid == initial_pid {
                continue;
            }
            crate::log_info!(
                "[{}] restart per-app capture initial_pid={} refreshed_pid={} elapsed_ms={}",
                mode.log_tag(),
                initial_pid,
                refreshed_pid,
                started.elapsed().as_millis()
            );
            // The previous producer must finish before a replacement can publish samples.
            drop(capture.take());
            if monitor_stop.load(Ordering::Acquire)
                || stop_signal.load(Ordering::Relaxed)
                || is_stale_session(session_id)
                || AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
                || TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
            {
                break;
            }
            SELECTED_APP_PID.store(refreshed_pid, Ordering::SeqCst);
            match start_per_app_capture(
                refreshed_pid,
                audio_buffer.clone(),
                stop_signal.clone(),
                pause.clone(),
            ) {
                Ok(replacement) => capture = Some(replacement),
                Err(error) => {
                    crate::log_info!(
                        "[{}] restart per-app capture failed refreshed_pid={} error={}",
                        mode.log_tag(),
                        refreshed_pid,
                        error
                    );
                    break;
                }
            }
            refresh_pending = false;
        }
        drop(capture);
    })
}

fn apply_tts_speed_for_s2s(speed: &str) {
    let mapped_speed = match speed {
        "Slow" => 85,
        "Fast" => 125,
        _ => 100,
    };
    crate::overlay::realtime_webview::state::REALTIME_TTS_SPEED
        .store(mapped_speed, Ordering::SeqCst);
    crate::overlay::realtime_webview::state::CURRENT_TTS_SPEED
        .store(mapped_speed, Ordering::SeqCst);
}

fn wait_for_selected_app(stop_signal: Arc<AtomicBool>, session_id: u64) -> Option<u32> {
    let started = Instant::now();
    while !stop_signal.load(Ordering::SeqCst)
        && !is_stale_session(session_id)
        && !AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
        && !TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
    {
        let pid = SELECTED_APP_PID.load(Ordering::SeqCst);
        if pid > 0 {
            return Some(pid);
        }
        if started.elapsed() > Duration::from_secs(30) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}
