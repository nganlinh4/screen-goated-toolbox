use super::*;
use super::transport::parse_s2s_update;

pub fn run_gemini_live_s2s(
    preset: Preset,
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
        preset.audio_source
    );
    apply_tts_speed_for_s2s(&settings.speed);
    let audio_buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
    let pause = Arc::new(AtomicBool::new(false));
    let selected_pid = SELECTED_APP_PID.load(Ordering::SeqCst);
    let mut per_app_capture_stop: Option<Arc<AtomicBool>> = None;
    let mut per_app_initial_pid: Option<u32> = None;
    let _stream = if preset.audio_source == "device" {
        let selected_pid = if selected_pid == 0 {
            crate::overlay::realtime_webview::app_selection::show_audio_app_selector_overlay();
            wait_for_selected_app(stop_signal.clone(), session_id)
        } else {
            Some(selected_pid)
        };
        if let Some(selected_pid) = selected_pid {
            per_app_initial_pid = Some(selected_pid);
            #[cfg(target_os = "windows")]
            {
                let capture_stop = Arc::new(AtomicBool::new(false));
                per_app_capture_stop = Some(capture_stop.clone());
                start_per_app_capture(
                    selected_pid,
                    audio_buffer.clone(),
                    capture_stop,
                    pause.clone(),
                )?;
            }
            None
        } else {
            return Err(anyhow::anyhow!(
                "S2S device mode needs a selected app to avoid capturing its own translated audio"
            ));
        }
    } else {
        Some(start_mic_capture(
            audio_buffer.clone(),
            stop_signal.clone(),
            pause.clone(),
        )?)
    };
    if let (Some(capture_stop), Some(initial_pid)) =
        (per_app_capture_stop.clone(), per_app_initial_pid)
    {
        spawn_s2s_per_app_audio_pid_refresh(
            initial_pid,
            capture_stop,
            audio_buffer.clone(),
            stop_signal.clone(),
            pause.clone(),
            session_id,
            settings.mode,
        );
    }

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
    capture_stop: Arc<AtomicBool>,
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    session_id: u64,
    mode: S2sMode,
) {
    std::thread::spawn(move || {
        let started = Instant::now();
        let mut last_observed_samples = 0usize;
        while !stop_signal.load(Ordering::Relaxed)
            && !is_stale_session(session_id)
            && started.elapsed() < Duration::from_secs(10)
        {
            std::thread::sleep(Duration::from_millis(500));
            let observed_samples = audio_buffer.lock().map(|buffer| buffer.len()).unwrap_or(0);
            if observed_samples > last_observed_samples + FRAME_SAMPLES {
                return;
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
            capture_stop.store(true, Ordering::SeqCst);
            SELECTED_APP_PID.store(refreshed_pid, Ordering::SeqCst);
            if let Err(error) = start_per_app_capture(
                refreshed_pid,
                audio_buffer.clone(),
                stop_signal.clone(),
                pause.clone(),
            ) {
                crate::log_info!(
                    "[{}] restart per-app capture failed refreshed_pid={} error={}",
                    mode.log_tag(),
                    refreshed_pid,
                    error
                );
            }
            return;
        }
    });
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
    while !stop_signal.load(Ordering::SeqCst) && !is_stale_session(session_id) {
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

fn run_live_translate_continuous(
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    event_tx: mpsc::Sender<S2sEvent>,
    overlay_hwnd: HWND,
    session_id: u64,
    settings: S2sSettings,
) -> Result<()> {
    const MAX_PENDING_SAMPLES: usize = FRAME_SAMPLES * 10;
    let mut pending = Vec::<i16>::new();
    let mut socket: Option<tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>> =
        None;
    let mut stream_id = 0u64;
    let mut sent_chunks = 0usize;
    let mut received_audio_chunks = 0usize;
    let mut dropped_samples = 0usize;
    let mut last_health_log = Instant::now();
    let mut last_server_activity = Instant::now();
    let mut sent_chunks_at_last_activity = 0usize;
    let mut text_state = LiveTranslateTextState::default();
    let playback = crate::api::tts::player::audio_player::AudioPlayer::new(
        crate::api::tts::types::PLAYBACK_SAMPLE_RATE,
        crate::api::tts::TTS_MANAGER.clone(),
    );

    while !stop_signal.load(Ordering::SeqCst)
        && !is_stale_session(session_id)
        && !AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
        && !TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
        && !LANGUAGE_CHANGE.load(Ordering::SeqCst)
    {
        if socket.is_none() {
            match open_fresh_socket_session(
                0,
                stream_id,
                &settings,
                &S2sContextSnapshot {
                    text: String::new(),
                },
                &stop_signal,
            ) {
                Ok(opened) => {
                    let mut opened = opened;
                    set_live_translate_socket_poll_timeout(&mut opened)?;
                    crate::log_info!(
                        "[{}] continuous socket connected stream={}",
                        settings.mode.log_tag(),
                        stream_id
                    );
                    last_server_activity = Instant::now();
                    sent_chunks_at_last_activity = sent_chunks;
                    socket = Some(opened);
                }
                Err(error) => {
                    crate::log_info!(
                        "[{}] continuous socket connect failed stream={} error={}",
                        settings.mode.log_tag(),
                        stream_id,
                        error
                    );
                    std::thread::sleep(Duration::from_millis(700));
                    continue;
                }
            }
        }

        if let Ok(mut guard) = audio_buffer.lock() {
            if !guard.is_empty() {
                pending.extend(guard.drain(..));
            }
        }
        if pending.len() > MAX_PENDING_SAMPLES {
            let drop_count = pending.len() - MAX_PENDING_SAMPLES;
            pending.drain(..drop_count);
            dropped_samples += drop_count;
        }

        if pending.len() < FRAME_SAMPLES {
            if let Some(open_socket) = socket.as_mut() {
                if !drain_live_translate_socket(
                    open_socket,
                    stream_id,
                    &event_tx,
                    &mut received_audio_chunks,
                    &mut text_state,
                    &mut last_server_activity,
                    &mut sent_chunks_at_last_activity,
                    sent_chunks,
                    &playback,
                )? {
                    socket = None;
                    stream_id += 1;
                }
            }
            std::thread::sleep(Duration::from_millis(8));
            continue;
        }

        let frame: Vec<i16> = pending.drain(..FRAME_SAMPLES).collect();
        update_live_translate_volume(&frame, overlay_hwnd);
        let send_result = socket
            .as_mut()
            .map(|open_socket| send_audio_chunk(open_socket, &frame))
            .unwrap_or_else(|| Err(anyhow::anyhow!("socket unavailable")));
        if let Err(error) = send_result {
            crate::log_info!(
                "[{}] continuous send failed stream={} error={}",
                settings.mode.log_tag(),
                stream_id,
                error
            );
            socket = None;
            stream_id += 1;
            continue;
        }
        sent_chunks += 1;

        if let Some(open_socket) = socket.as_mut()
            && !drain_live_translate_socket(
                open_socket,
                stream_id,
                &event_tx,
                &mut received_audio_chunks,
                &mut text_state,
                &mut last_server_activity,
                &mut sent_chunks_at_last_activity,
                sent_chunks,
                &playback,
            )?
        {
            socket = None;
            stream_id += 1;
        }

        if last_health_log.elapsed() >= Duration::from_secs(5) {
            crate::log_info!(
                "[{}] continuous health stream={} sent_chunks={} received_audio_chunks={} pending_ms={} dropped_ms={}",
                settings.mode.log_tag(),
                stream_id,
                sent_chunks,
                received_audio_chunks,
                samples_to_ms(pending.len()),
                samples_to_ms(dropped_samples)
            );
            dropped_samples = 0;
            last_health_log = Instant::now();
        }

        let silent_sent_chunks = sent_chunks.saturating_sub(sent_chunks_at_last_activity);
        if socket.is_some()
            && silent_sent_chunks >= 100
            && last_server_activity.elapsed() >= Duration::from_secs(15)
        {
            crate::log_info!(
                "[{}] continuous reconnect reason=server-silent stream={} silent_ms={} silent_sent_chunks={} received_audio_chunks={}",
                settings.mode.log_tag(),
                stream_id,
                last_server_activity.elapsed().as_millis(),
                silent_sent_chunks,
                received_audio_chunks
            );
            socket = None;
            stream_id += 1;
            last_server_activity = Instant::now();
            sent_chunks_at_last_activity = sent_chunks;
        }
    }
    Ok(())
}

fn set_live_translate_socket_poll_timeout(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
) -> Result<()> {
    socket
        .get_mut()
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(2)))?;
    Ok(())
}

fn drain_live_translate_socket(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    stream_id: u64,
    event_tx: &mpsc::Sender<S2sEvent>,
    received_audio_chunks: &mut usize,
    text_state: &mut LiveTranslateTextState,
    last_server_activity: &mut Instant,
    sent_chunks_at_last_activity: &mut usize,
    sent_chunks: usize,
    playback: &crate::api::tts::player::audio_player::AudioPlayer,
) -> Result<bool> {
    loop {
        match socket.read() {
            Ok(Message::Text(msg)) => {
                handle_live_translate_message(
                    stream_id,
                    msg.as_str(),
                    event_tx,
                    received_audio_chunks,
                    text_state,
                    last_server_activity,
                    sent_chunks_at_last_activity,
                    sent_chunks,
                    playback,
                );
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    handle_live_translate_message(
                        stream_id,
                        &text,
                        event_tx,
                        received_audio_chunks,
                        text_state,
                        last_server_activity,
                        sent_chunks_at_last_activity,
                        sent_chunks,
                        playback,
                    );
                }
            }
            Ok(Message::Close(frame)) => {
                crate::log_info!(
                    "[RealtimeLiveTranslate] continuous socket closed stream={} frame={:?}",
                    stream_id,
                    frame
                );
                return Ok(false);
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Ok(true);
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn handle_live_translate_message(
    stream_id: u64,
    message: &str,
    event_tx: &mpsc::Sender<S2sEvent>,
    received_audio_chunks: &mut usize,
    text_state: &mut LiveTranslateTextState,
    last_server_activity: &mut Instant,
    sent_chunks_at_last_activity: &mut usize,
    sent_chunks: usize,
    playback: &crate::api::tts::player::audio_player::AudioPlayer,
) {
    let update = parse_s2s_update(message);
    if let Some(error) = update.error {
        let _ = event_tx.send(S2sEvent::Error {
            id: stream_id,
            message: error,
        });
        return;
    }
    if update.interrupted {
        let _ = event_tx.send(S2sEvent::Interrupt);
    }
    let mut text_changed = false;
    if let Some(text) = update.input_transcript {
        text_changed |= text_state.update_source(&text);
    }
    if let Some(text) = update.output_transcript {
        text_changed |= text_state.update_target(&text);
    }
    let audio_chunk_count = update.audio_chunks.len();
    let has_activity =
        text_changed || audio_chunk_count > 0 || update.interrupted || update.turn_complete;
    if has_activity {
        *last_server_activity = Instant::now();
        *sent_chunks_at_last_activity = sent_chunks;
    }
    if text_changed {
        let _ = event_tx.send(text_state.snapshot_event());
    }
    *received_audio_chunks += audio_chunk_count;
    for bytes in update.audio_chunks {
        playback.play_native_stream(&bytes);
    }
}

#[derive(Default)]
struct LiveTranslateTextState {
    source_committed: String,
    source_draft: String,
    target_committed: String,
    target_draft: String,
}

impl LiveTranslateTextState {
    fn update_source(&mut self, incoming: &str) -> bool {
        let before = (self.source_committed.clone(), self.source_draft.clone());
        update_live_text_pair(&mut self.source_committed, &mut self.source_draft, incoming);
        before.0 != self.source_committed || before.1 != self.source_draft
    }

    fn update_target(&mut self, incoming: &str) -> bool {
        let before = (self.target_committed.clone(), self.target_draft.clone());
        update_live_text_pair(&mut self.target_committed, &mut self.target_draft, incoming);
        before.0 != self.target_committed || before.1 != self.target_draft
    }

    fn snapshot_event(&self) -> S2sEvent {
        let source_full = join_live_text(&self.source_committed, &self.source_draft);
        S2sEvent::LiveText {
            source_committed_len: self.source_committed.len(),
            source_full,
            target_committed: self.target_committed.clone(),
            target_draft: self.target_draft.clone(),
        }
    }
}

fn update_live_text_pair(committed: &mut String, draft: &mut String, incoming: &str) {
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return;
    }
    if draft.is_empty() {
        draft.push_str(incoming);
        maybe_commit_live_draft(committed, draft);
        return;
    }
    if incoming == draft.trim() || draft.trim_start().starts_with(incoming) {
        return;
    }
    if incoming.starts_with(draft.trim()) {
        draft.clear();
        draft.push_str(incoming);
        maybe_commit_live_draft(committed, draft);
        return;
    }

    let overlap = largest_suffix_prefix_overlap(draft.trim_end(), incoming);
    if overlap > 0 {
        merge_segment_text(draft, incoming);
        maybe_commit_live_draft(committed, draft);
        return;
    }

    commit_live_draft(committed, draft);
    draft.push_str(incoming);
    maybe_commit_live_draft(committed, draft);
}

fn maybe_commit_live_draft(committed: &mut String, draft: &mut String) {
    let trimmed = draft.trim();
    let word_count = trimmed.split_whitespace().count();
    let ends_sentence = trimmed
        .chars()
        .last()
        .is_some_and(|ch| matches!(ch, '.' | '?' | '!' | '。' | '？' | '！'));
    if ends_sentence || word_count >= 18 {
        commit_live_draft(committed, draft);
    }
}

fn commit_live_draft(committed: &mut String, draft: &mut String) {
    let trimmed = draft.trim();
    if trimmed.is_empty() {
        draft.clear();
        return;
    }
    if !committed.is_empty() {
        committed.push(' ');
    }
    committed.push_str(trimmed);
    draft.clear();
}

fn join_live_text(committed: &str, draft: &str) -> String {
    if committed.is_empty() {
        draft.trim().to_string()
    } else if draft.trim().is_empty() {
        committed.to_string()
    } else {
        format!("{} {}", committed, draft.trim())
    }
}

fn update_live_translate_volume(frame: &[i16], overlay_hwnd: HWND) {
    let rms = calculate_rms(frame);
    REALTIME_RMS.store(rms.to_bits(), Ordering::Relaxed);
    if overlay_hwnd.is_invalid() {
        return;
    }
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(overlay_hwnd),
            WM_VOLUME_UPDATE,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
        );
    }
}
