use super::super::*;
use super::drain::LiveTranslateOutputState;
use super::lifecycle_adapter::{AdapterPoll, LiveTranslateLifecycleAdapter};

pub(super) fn run_live_translate_continuous(
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    event_tx: mpsc::Sender<S2sEvent>,
    overlay_hwnd: HWND,
    session_id: u64,
    settings: S2sSettings,
) -> Result<()> {
    const MAX_PENDING_SAMPLES: usize = FRAME_SAMPLES * 10;
    let log_tag = settings.mode.log_tag();
    let mut pending = Vec::<i16>::new();
    let mut sent_chunks = 0usize;
    let mut dropped_samples = 0usize;
    let mut last_health_log = Instant::now();
    let mut output = LiveTranslateOutputState::default();
    let playback = crate::api::tts::player::audio_player::AudioPlayer::new(
        crate::api::tts::types::PLAYBACK_SAMPLE_RATE,
        crate::api::tts::TTS_MANAGER.clone(),
    );
    let mut lifecycle = LiveTranslateLifecycleAdapter::new(settings);
    let mut cancelled = || live_translate_should_stop(&stop_signal, session_id);
    lifecycle.start(&mut cancelled)?;

    while !cancelled() {
        append_captured_audio(&audio_buffer, &mut pending);
        dropped_samples =
            dropped_samples.saturating_add(cap_pending_audio(&mut pending, MAX_PENDING_SAMPLES));

        let input_active = pending
            .get(..FRAME_SAMPLES)
            .is_some_and(live_translate_frame_is_active);
        if !lifecycle.is_active() {
            lifecycle.tick(
                pending_work_count(&playback),
                sample_count(&pending),
                input_active,
                &mut cancelled,
            )?;
        }

        if lifecycle.is_active() && pending.len() >= FRAME_SAMPLES {
            let frame = pending[..FRAME_SAMPLES].to_vec();
            update_live_translate_volume(&frame, overlay_hwnd);
            if lifecycle.send_audio(&frame, input_active, &mut cancelled)? {
                pending.drain(..FRAME_SAMPLES);
                sent_chunks = sent_chunks.saturating_add(1);
            }
        }

        drain_ready_frames(
            &mut lifecycle,
            &mut output,
            &event_tx,
            &playback,
            &mut cancelled,
        )?;
        lifecycle.tick(
            pending_work_count(&playback),
            sample_count(&pending),
            input_active,
            &mut cancelled,
        )?;

        if last_health_log.elapsed() >= Duration::from_secs(5) {
            crate::log_info!(
                "[{}] continuous health generation={} sent_chunks={} received_audio_chunks={} pending_ms={} dropped_ms={} socket_age_ms={} since_server_ms={} since_input_ms={} reconnect_attempts={} generation_boundaries={} turn_boundaries={} interrupted_generations={}",
                log_tag,
                lifecycle.generation(),
                sent_chunks,
                output.received_audio_chunks(),
                samples_to_ms(pending.len()),
                samples_to_ms(dropped_samples),
                lifecycle.socket_age_ms(),
                lifecycle.since_server_activity_ms(),
                lifecycle.since_input_activity_ms(),
                lifecycle.reconnect_attempt(),
                output.generation_boundaries(),
                output.turn_boundaries(),
                output.interrupted_generations()
            );
            dropped_samples = 0;
            last_health_log = Instant::now();
        }

        if pending.len() < FRAME_SAMPLES || !lifecycle.is_active() {
            std::thread::sleep(Duration::from_millis(8));
        }
    }

    lifecycle.cancel()?;
    Ok(())
}

fn drain_ready_frames(
    lifecycle: &mut LiveTranslateLifecycleAdapter,
    output: &mut LiveTranslateOutputState,
    event_tx: &mpsc::Sender<S2sEvent>,
    playback: &crate::api::tts::player::audio_player::AudioPlayer,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<()> {
    loop {
        match lifecycle.poll(cancelled)? {
            AdapterPoll::Frame { frame, effects } => {
                output.apply_frame_effects(&frame, effects, event_tx, playback)?;
            }
            AdapterPoll::Idle | AdapterPoll::StateChanged => return Ok(()),
        }
    }
}

fn append_captured_audio(audio_buffer: &Arc<Mutex<Vec<i16>>>, pending: &mut Vec<i16>) {
    if let Ok(mut guard) = audio_buffer.lock()
        && !guard.is_empty()
    {
        pending.extend(guard.drain(..));
    }
}

fn cap_pending_audio(pending: &mut Vec<i16>, max_samples: usize) -> usize {
    let drop_count = pending.len().saturating_sub(max_samples);
    if drop_count > 0 {
        pending.drain(..drop_count);
    }
    drop_count
}

fn pending_work_count(playback: &crate::api::tts::player::audio_player::AudioPlayer) -> u64 {
    u64::from(playback.has_pending_playback())
}

fn sample_count(samples: &[i16]) -> u64 {
    u64::try_from(samples.len()).unwrap_or(u64::MAX)
}

fn live_translate_frame_is_active(frame: &[i16]) -> bool {
    calculate_rms(frame) >= MIN_SPEECH_THRESHOLD * SPEECH_THRESHOLD_MULTIPLIER
}

fn live_translate_should_stop(stop_signal: &Arc<AtomicBool>, session_id: u64) -> bool {
    stop_signal.load(Ordering::SeqCst)
        || is_stale_session(session_id)
        || AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
        || TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
        || LANGUAGE_CHANGE.load(Ordering::SeqCst)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_audio_cap_keeps_the_newest_samples() {
        let mut pending = (0_i16..20).collect::<Vec<_>>();
        assert_eq!(cap_pending_audio(&mut pending, 8), 12);
        assert_eq!(pending, (12_i16..20).collect::<Vec<_>>());
    }
}
