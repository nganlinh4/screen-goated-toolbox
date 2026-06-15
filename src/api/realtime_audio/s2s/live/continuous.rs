use super::super::*;
use super::drain::drain_live_translate_socket;
use super::text_state::LiveTranslateTextState;

pub(super) fn run_live_translate_continuous(
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    event_tx: mpsc::Sender<S2sEvent>,
    overlay_hwnd: HWND,
    session_id: u64,
    settings: S2sSettings,
) -> Result<()> {
    const MAX_PENDING_SAMPLES: usize = FRAME_SAMPLES * 10;
    const PROACTIVE_ROTATE_AFTER: Duration = Duration::from_secs(12 * 60);
    const ROTATE_QUIET_FOR: Duration = Duration::from_secs(3);
    const SERVER_SILENT_RECONNECT_AFTER: Duration = Duration::from_secs(15);
    const SERVER_SILENT_SENT_CHUNKS: usize = 100;
    let mut pending = Vec::<i16>::new();
    let mut socket: Option<tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>> =
        None;
    let mut socket_connected_at: Option<Instant> = None;
    let mut stream_id = 0u64;
    let mut sent_chunks = 0usize;
    let mut received_audio_chunks = 0usize;
    let mut dropped_samples = 0usize;
    let mut last_health_log = Instant::now();
    let mut last_server_activity = Instant::now();
    let mut last_active_input = Instant::now();
    let mut sent_chunks_at_last_activity = 0usize;
    let mut reconnect_attempts = 0u32;
    let mut next_connect_after = Instant::now();
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
            if next_connect_after > Instant::now() {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
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
                        "[{}] continuous socket connected stream={} reconnect_attempts={}",
                        settings.mode.log_tag(),
                        stream_id,
                        reconnect_attempts
                    );
                    socket_connected_at = Some(Instant::now());
                    last_server_activity = Instant::now();
                    sent_chunks_at_last_activity = sent_chunks;
                    reconnect_attempts = 0;
                    socket = Some(opened);
                }
                Err(error) => {
                    let delay = live_translate_reconnect_delay(reconnect_attempts, stream_id);
                    crate::log_info!(
                        "[{}] continuous socket connect failed stream={} attempt={} retry_ms={} error={}",
                        settings.mode.log_tag(),
                        stream_id,
                        reconnect_attempts + 1,
                        delay.as_millis(),
                        error
                    );
                    reconnect_attempts = reconnect_attempts.saturating_add(1);
                    next_connect_after = Instant::now() + delay;
                    continue;
                }
            }
        }

        if let Ok(mut guard) = audio_buffer.lock()
            && !guard.is_empty() {
                pending.extend(guard.drain(..));
            }
        if pending.len() > MAX_PENDING_SAMPLES {
            let drop_count = pending.len() - MAX_PENDING_SAMPLES;
            pending.drain(..drop_count);
            dropped_samples += drop_count;
        }

        if pending.len() < FRAME_SAMPLES {
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
                )? {
                    schedule_live_translate_reconnect(
                        &mut socket,
                        &mut socket_connected_at,
                        &mut stream_id,
                        &mut reconnect_attempts,
                        &mut next_connect_after,
                        settings.mode.log_tag(),
                        "socket-drain",
                    );
                }
            maybe_rotate_live_translate_socket(
                &mut socket,
                &mut socket_connected_at,
                &mut stream_id,
                &mut reconnect_attempts,
                &mut next_connect_after,
                settings.mode.log_tag(),
                pending.len(),
                last_active_input,
                last_server_activity,
                PROACTIVE_ROTATE_AFTER,
                ROTATE_QUIET_FOR,
            );
            std::thread::sleep(Duration::from_millis(8));
            continue;
        }

        let frame: Vec<i16> = pending.drain(..FRAME_SAMPLES).collect();
        if calculate_rms(&frame) >= MIN_SPEECH_THRESHOLD * SPEECH_THRESHOLD_MULTIPLIER {
            last_active_input = Instant::now();
        }
        update_live_translate_volume(&frame, overlay_hwnd);
        let send_result = socket
            .as_mut()
            .map(|open_socket| send_audio_chunk(open_socket, &frame))
            .unwrap_or_else(|| Err(anyhow::anyhow!("socket unavailable")));
        if let Err(error) = send_result {
            let delay = live_translate_reconnect_delay(reconnect_attempts, stream_id);
            crate::log_info!(
                "[{}] continuous send failed stream={} attempt={} retry_ms={} socket_age_ms={} since_server_ms={} error={}",
                settings.mode.log_tag(),
                stream_id,
                reconnect_attempts + 1,
                delay.as_millis(),
                socket_age_ms(socket_connected_at),
                last_server_activity.elapsed().as_millis(),
                error
            );
            socket = None;
            socket_connected_at = None;
            stream_id += 1;
            reconnect_attempts = reconnect_attempts.saturating_add(1);
            next_connect_after = Instant::now() + delay;
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
            schedule_live_translate_reconnect(
                &mut socket,
                &mut socket_connected_at,
                &mut stream_id,
                &mut reconnect_attempts,
                &mut next_connect_after,
                settings.mode.log_tag(),
                "socket-drain",
            );
        }

        if last_health_log.elapsed() >= Duration::from_secs(5) {
            crate::log_info!(
                "[{}] continuous health stream={} sent_chunks={} received_audio_chunks={} pending_ms={} dropped_ms={} socket_age_ms={} since_server_ms={} since_input_ms={} reconnect_attempts={}",
                settings.mode.log_tag(),
                stream_id,
                sent_chunks,
                received_audio_chunks,
                samples_to_ms(pending.len()),
                samples_to_ms(dropped_samples),
                socket_age_ms(socket_connected_at),
                last_server_activity.elapsed().as_millis(),
                last_active_input.elapsed().as_millis(),
                reconnect_attempts
            );
            dropped_samples = 0;
            last_health_log = Instant::now();
        }

        let silent_sent_chunks = sent_chunks.saturating_sub(sent_chunks_at_last_activity);
        if socket.is_some()
            && silent_sent_chunks >= SERVER_SILENT_SENT_CHUNKS
            && last_server_activity.elapsed() >= SERVER_SILENT_RECONNECT_AFTER
        {
            crate::log_info!(
                "[{}] continuous reconnect reason=server-silent stream={} silent_ms={} silent_sent_chunks={} received_audio_chunks={} socket_age_ms={}",
                settings.mode.log_tag(),
                stream_id,
                last_server_activity.elapsed().as_millis(),
                silent_sent_chunks,
                received_audio_chunks,
                socket_age_ms(socket_connected_at)
            );
            schedule_live_translate_reconnect(
                &mut socket,
                &mut socket_connected_at,
                &mut stream_id,
                &mut reconnect_attempts,
                &mut next_connect_after,
                settings.mode.log_tag(),
                "server-silent",
            );
            last_server_activity = Instant::now();
            sent_chunks_at_last_activity = sent_chunks;
        }
        maybe_rotate_live_translate_socket(
            &mut socket,
            &mut socket_connected_at,
            &mut stream_id,
            &mut reconnect_attempts,
            &mut next_connect_after,
            settings.mode.log_tag(),
            pending.len(),
            last_active_input,
            last_server_activity,
            PROACTIVE_ROTATE_AFTER,
            ROTATE_QUIET_FOR,
        );
    }
    Ok(())
}

fn live_translate_reconnect_delay(attempt: u32, stream_id: u64) -> Duration {
    let capped_attempt = attempt.min(5);
    let base_ms = 250u64.saturating_mul(1u64 << capped_attempt);
    let jitter_ms = ((stream_id.wrapping_mul(97) + attempt as u64 * 53) % 180) + 20;
    Duration::from_millis((base_ms + jitter_ms).min(6_000))
}

fn socket_age_ms(socket_connected_at: Option<Instant>) -> u128 {
    socket_connected_at
        .map(|connected| connected.elapsed().as_millis())
        .unwrap_or(0)
}

fn schedule_live_translate_reconnect(
    socket: &mut Option<tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>>,
    socket_connected_at: &mut Option<Instant>,
    stream_id: &mut u64,
    reconnect_attempts: &mut u32,
    next_connect_after: &mut Instant,
    log_tag: &str,
    reason: &str,
) {
    let delay = live_translate_reconnect_delay(*reconnect_attempts, *stream_id);
    crate::log_info!(
        "[{}] continuous reconnect scheduled reason={} stream={} attempt={} retry_ms={} socket_age_ms={}",
        log_tag,
        reason,
        *stream_id,
        (*reconnect_attempts).saturating_add(1),
        delay.as_millis(),
        socket_age_ms(*socket_connected_at)
    );
    *socket = None;
    *socket_connected_at = None;
    *stream_id = (*stream_id).saturating_add(1);
    *reconnect_attempts = (*reconnect_attempts).saturating_add(1);
    *next_connect_after = Instant::now() + delay;
}

#[expect(
    clippy::too_many_arguments,
    reason = "keeps live translate socket state explicit at the call site"
)]
fn maybe_rotate_live_translate_socket(
    socket: &mut Option<tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>>,
    socket_connected_at: &mut Option<Instant>,
    stream_id: &mut u64,
    reconnect_attempts: &mut u32,
    next_connect_after: &mut Instant,
    log_tag: &str,
    pending_samples: usize,
    last_active_input: Instant,
    last_server_activity: Instant,
    rotate_after: Duration,
    quiet_for: Duration,
) {
    let Some(connected_at) = *socket_connected_at else {
        return;
    };
    if socket.is_none()
        || connected_at.elapsed() < rotate_after
        || pending_samples >= FRAME_SAMPLES
        || last_active_input.elapsed() < quiet_for
        || last_server_activity.elapsed() < quiet_for
    {
        return;
    }
    crate::log_info!(
        "[{}] continuous reconnect reason=proactive-rotation stream={} socket_age_ms={} quiet_input_ms={} quiet_server_ms={}",
        log_tag,
        *stream_id,
        connected_at.elapsed().as_millis(),
        last_active_input.elapsed().as_millis(),
        last_server_activity.elapsed().as_millis()
    );
    schedule_live_translate_reconnect(
        socket,
        socket_connected_at,
        stream_id,
        reconnect_attempts,
        next_connect_after,
        log_tag,
        "proactive-rotation",
    );
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
