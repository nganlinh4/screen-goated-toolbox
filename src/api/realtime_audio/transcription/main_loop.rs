use anyhow::Result;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::dedicated::{compute_i16_rms, samples_to_ms, uses_periodic_silence_cycle};
use super::reconnect::{
    ReconnectContext, capture_may_have_dropped, retain_audio, show_audio_gap, try_reconnect,
};
use crate::api::gemini_live::ready_session::{LivePoll, ReadyLiveSession};
use crate::api::gemini_live::transport::is_recoverable_anyhow_socket_error;
use crate::api::realtime_audio::state::SharedRealtimeState;
use crate::api::realtime_audio::utils::notify_transcript_changed;
use crate::api::realtime_audio::{DEVICE_RECONNECT_REQUESTED, WM_VOLUME_UPDATE};

/// Audio mode state machine for silence injection.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum AudioMode {
    Normal,
    Silence,
    CatchUp,
}

#[derive(Default)]
struct DedicatedTranscriptState {
    delivery: crate::api::gemini_transcribe::TranscriptState,
}

impl DedicatedTranscriptState {
    fn replace_interim(&mut self, text: &str) {
        self.delivery.replace_interim(text);
    }

    fn commit_final(&mut self, text: &str) {
        self.delivery.commit_final(text);
    }

    fn apply_to(&self, state: &mut crate::api::realtime_audio::state::RealtimeState) {
        state.set_transcript_segments(self.delivery.committed(), self.delivery.interim());
    }
}

impl AudioMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Silence => "silence",
            Self::CatchUp => "catchup",
        }
    }
}

pub(super) struct RealtimeMainLoop<'a> {
    pub(super) session: ReadyLiveSession,
    pub(super) audio_buffer: Arc<Mutex<Vec<i16>>>,
    pub(super) stop_signal: Arc<AtomicBool>,
    pub(super) overlay_hwnd: HWND,
    pub(super) state: SharedRealtimeState,
    pub(super) session_id: u64,
    pub(super) engine_revision: u64,
    pub(super) gemini_live_model: &'a str,
    pub(super) gemini_api_key: &'a str,
    pub(super) capture_label: &'static str,
    pub(super) reconnect_on_no_results: bool,
    pub(super) uses_interim_transcripts: bool,
    #[cfg(test)]
    pub(super) observations: Option<Arc<Mutex<SessionObservations>>>,
}

#[cfg(test)]
#[derive(Default, serde::Serialize)]
pub(super) struct SessionObservations {
    pub(super) first_no_result_reconnect_ms: Option<u128>,
    pub(super) transcript_updates: u64,
    pub(super) transcript_updates_after_first_flush: u64,
    pub(super) interim_updates: u64,
    pub(super) final_updates: u64,
    pub(super) input_events: Vec<(u128, bool, String)>,
    pub(super) generated_audio_bytes: usize,
}

pub(super) fn run_main_loop(params: RealtimeMainLoop<'_>) -> Result<()> {
    let RealtimeMainLoop {
        mut session,
        audio_buffer,
        stop_signal,
        overlay_hwnd,
        state,
        session_id,
        engine_revision,
        gemini_live_model,
        gemini_api_key,
        capture_label,
        reconnect_on_no_results,
        uses_interim_transcripts,
        #[cfg(test)]
        observations,
    } = params;
    let mut last_send = Instant::now();
    let send_interval = Duration::from_millis(100);

    let mut audio_mode = AudioMode::Normal;
    let mut mode_start = Instant::now();
    let mut silence_buffer: Vec<i16> = Vec::new();

    const NORMAL_DURATION: Duration = Duration::from_secs(20);
    const SILENCE_DURATION: Duration = Duration::from_secs(2);
    const SAMPLES_PER_100MS: usize = 1600;

    let mut last_transcription_time = Instant::now();
    let mut consecutive_empty_reads: u32 = 0;
    const EMPTY_READ_CHECK_COUNT: u32 = 50;

    let session_started = Instant::now();
    let mut connection_started = Instant::now();
    let (mut vocabulary_version, _) = super::dedicated::vocabulary_snapshot();
    let mut resumption_handle: Option<String> = None;
    let mut recovery =
        crate::api::gemini_live::transcription_recovery::TranscriptionRecovery::default();
    let mut dedicated_vad = super::dedicated::HybridVad::default();
    let mut last_health_log = Instant::now();
    let mut sent_chunks = 0u64;
    let mut sent_samples = 0usize;
    let mut active_sent_samples = 0usize;
    let mut active_samples_since_transcript = 0usize;
    let mut transcript_updates = 0u64;
    let mut transcript_chars = 0usize;
    let mut reconnect_count = 0u32;
    let mut dedicated_transcript = DedicatedTranscriptState::default();
    let mut gap_notified = false;

    macro_rules! reconnect {
        () => {{
            let connected = try_reconnect(ReconnectContext {
                session: &mut session,
                api_key: gemini_api_key,
                model: gemini_live_model,
                audio_buffer: &audio_buffer,
                silence_buffer: &mut silence_buffer,
                audio_mode: &mut audio_mode,
                mode_start: &mut mode_start,
                last_transcription_time: &mut last_transcription_time,
                consecutive_empty_reads: &mut consecutive_empty_reads,
                stop_signal: &stop_signal,
                resumption_handle: &mut resumption_handle,
                gap_notified: &mut gap_notified,
            })?;
            if connected {
                connection_started = Instant::now();
                dedicated_vad.reset_connection();
                recovery.reset();
                active_samples_since_transcript = 0;
                reconnect_count += 1;
                last_send = Instant::now();
            }
            connected
        }};
    }

    while !stop_signal.load(Ordering::Relaxed)
        && crate::overlay::realtime_webview::state::engine_is_current(engine_revision)
    {
        if overlay_hwnd.0 != 0 as _ && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            stop_signal.store(true, Ordering::SeqCst);
            break;
        }

        {
            use crate::overlay::realtime_webview::{
                AUDIO_SOURCE_CHANGE, TRANSCRIPTION_MODEL_CHANGE,
            };
            if AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
                || TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
                || DEVICE_RECONNECT_REQUESTED.load(Ordering::SeqCst)
            {
                break;
            }
        }

        let current_vocabulary_version = super::dedicated::vocabulary_snapshot().0;
        if crate::api::gemini_transcribe::proactive_rotation_due(
            uses_interim_transcripts,
            connection_started.elapsed(),
            current_vocabulary_version != vocabulary_version,
            dedicated_vad.is_safe_gap(),
        ) {
            let reason = if current_vocabulary_version != vocabulary_version {
                "vocabulary"
            } else {
                "rotation"
            };
            if !reconnect!() {
                break;
            }
            vocabulary_version = current_vocabulary_version;
            crate::log_info!("[RealtimeGeminiLiveHealth] reconnect-ok reason={reason}");
        }

        if audio_mode == AudioMode::CatchUp && silence_buffer.is_empty() {
            audio_mode = AudioMode::Normal;
            mode_start = Instant::now();
        }
        if uses_periodic_silence_cycle(uses_interim_transcripts) {
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
                        recovery.input_flushed(session_started.elapsed().as_millis() as u64);
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
        }

        if last_send.elapsed() >= send_interval {
            let mut send_failed = false;
            let real_audio: Vec<i16> = {
                let mut buf = audio_buffer.lock().unwrap();
                std::mem::take(&mut *buf)
            };
            if capture_may_have_dropped(&real_audio) {
                show_audio_gap(&mut gap_notified);
            }
            match audio_mode {
                AudioMode::Normal => {
                    if !real_audio.is_empty() {
                        let is_active = compute_i16_rms(&real_audio) >= ACTIVE_AUDIO_RMS_THRESHOLD;
                        send_failed = send_audio(&mut session, &real_audio).is_err()
                            || (uses_interim_transcripts
                                && dedicated_vad
                                    .observe(compute_i16_rms(&real_audio), Instant::now())
                                && session.end_audio_stream().is_err());
                        if !send_failed {
                            sent_chunks += 1;
                            sent_samples += real_audio.len();
                        }
                        if is_active && !send_failed {
                            active_sent_samples += real_audio.len();
                            active_samples_since_transcript += real_audio.len();
                            recovery.audio_sent(samples_to_ms(real_audio.len()));
                        }
                    }
                }
                AudioMode::Silence => {
                    retain_audio(&mut silence_buffer, &real_audio, &mut gap_notified);
                    let silence: Vec<i16> = vec![0i16; SAMPLES_PER_100MS];
                    if send_audio(&mut session, &silence).is_err() {
                        send_failed = true;
                    }
                    if !send_failed {
                        sent_chunks += 1;
                        sent_samples += silence.len();
                    }
                }
                AudioMode::CatchUp => {
                    retain_audio(&mut silence_buffer, &real_audio, &mut gap_notified);
                    let chunk_size = SAMPLES_PER_100MS * 2;
                    let to_send: Vec<i16> = if silence_buffer.len() >= chunk_size {
                        silence_buffer.drain(..chunk_size).collect()
                    } else if !silence_buffer.is_empty() {
                        std::mem::take(&mut silence_buffer)
                    } else {
                        Vec::new()
                    };
                    if !to_send.is_empty() && send_audio(&mut session, &to_send).is_err() {
                        send_failed = true;
                    }
                    if !to_send.is_empty() && !send_failed {
                        if uses_interim_transcripts
                            && dedicated_vad.observe(compute_i16_rms(&to_send), Instant::now())
                            && session.end_audio_stream().is_err()
                        {
                            send_failed = true;
                        }
                        let is_active = compute_i16_rms(&to_send) >= ACTIVE_AUDIO_RMS_THRESHOLD;
                        sent_chunks += 1;
                        sent_samples += to_send.len();
                        if is_active {
                            active_sent_samples += to_send.len();
                            active_samples_since_transcript += to_send.len();
                            recovery.audio_sent(samples_to_ms(to_send.len()));
                        }
                    }
                }
            }
            if send_failed {
                show_audio_gap(&mut gap_notified);
                if !reconnect!() {
                    break;
                }
                continue;
            }
            last_send = Instant::now();
            unsafe {
                let _ = PostMessageW(Some(overlay_hwnd), WM_VOLUME_UPDATE, WPARAM(0), LPARAM(0));
            }
        }
        if uses_interim_transcripts
            && dedicated_vad.poll_end(Instant::now())
            && session.end_audio_stream().is_err()
        {
            show_audio_gap(&mut gap_notified);
            if !reconnect!() {
                break;
            }
            continue;
        }

        match session.poll() {
            Ok(LivePoll::Frame(frame)) => {
                #[cfg(test)]
                if let Some(observations) = observations.as_ref() {
                    observations.lock().unwrap().generated_audio_bytes +=
                        frame.audio_chunks.iter().map(Vec::len).sum::<usize>();
                }
                if frame.content_count() > 0 || frame.response_complete() || frame.interrupted {
                    recovery.reset();
                }
                crate::api::gemini_transcribe::observe_resumption(
                    &mut resumption_handle,
                    frame.session_resumption.as_ref(),
                );
                let should_rotate_for_go_away = crate::api::gemini_transcribe::rotates_on_go_away(
                    uses_interim_transcripts,
                    frame.go_away.is_some(),
                );
                let transcript = if uses_interim_transcripts {
                    match (frame.input_transcript, frame.interim_input_transcript) {
                        (Some(text), _) => Some((text, true)),
                        (None, Some(text)) => Some((text, false)),
                        (None, None) => None,
                    }
                } else {
                    frame.input_transcript.map(|text| (text, true))
                };
                if let Some((transcript, is_final)) = transcript {
                    last_transcription_time = Instant::now();
                    if is_final {
                        active_samples_since_transcript = 0;
                    }
                    consecutive_empty_reads = 0;
                    transcript_updates += 1;
                    gap_notified = false;
                    #[cfg(test)]
                    if let Some(observations) = observations.as_ref() {
                        let mut observations = observations.lock().unwrap();
                        observations.transcript_updates += 1;
                        observations.input_events.push((
                            session_started.elapsed().as_millis(),
                            is_final,
                            transcript.clone(),
                        ));
                        if is_final {
                            observations.final_updates += 1;
                        } else {
                            observations.interim_updates += 1;
                        }
                        if session_started.elapsed() >= Duration::from_secs(22) {
                            observations.transcript_updates_after_first_flush += 1;
                        }
                    }
                    transcript_chars += transcript.chars().count();
                    crate::overlay::realtime_webview::state::publish_if_engine_current(
                        session_id,
                        &stop_signal,
                        engine_revision,
                        || {
                            let has_display_text = if let Ok(mut s) = state.lock() {
                                if uses_interim_transcripts {
                                    if is_final {
                                        dedicated_transcript.commit_final(&transcript);
                                    } else {
                                        dedicated_transcript.replace_interim(&transcript);
                                    }
                                    dedicated_transcript.apply_to(&mut s);
                                } else {
                                    s.append_transcript(&transcript);
                                }
                                !s.display_transcript.is_empty()
                            } else {
                                false
                            };
                            if has_display_text {
                                notify_transcript_changed(overlay_hwnd);
                            }
                        },
                    );
                }
                if should_rotate_for_go_away && !reconnect!() {
                    break;
                }
            }
            Ok(LivePoll::PeerClosed(_)) => {
                show_audio_gap(&mut gap_notified);
                crate::log_info!(
                    "[RealtimeGeminiLiveHealth] reconnect-start reason=close empty_reads={} since_transcript_ms={} mode={}",
                    consecutive_empty_reads,
                    last_transcription_time.elapsed().as_millis(),
                    audio_mode.as_str()
                );
                if !reconnect!() {
                    crate::log_info!("[RealtimeGeminiLiveHealth] reconnect-failed reason=close");
                    break;
                }
                crate::log_info!(
                    "[RealtimeGeminiLiveHealth] reconnect-ok reason=close count={} catchup_ms={}",
                    reconnect_count,
                    samples_to_ms(silence_buffer.len())
                );
            }
            Ok(LivePoll::Idle) => {
                consecutive_empty_reads += 1;
                if reconnect_on_no_results
                    && consecutive_empty_reads >= EMPTY_READ_CHECK_COUNT
                    && recovery.should_reconnect(session_started.elapsed().as_millis() as u64)
                {
                    #[cfg(test)]
                    if let Some(observations) = observations.as_ref() {
                        observations
                            .lock()
                            .unwrap()
                            .first_no_result_reconnect_ms
                            .get_or_insert(session_started.elapsed().as_millis());
                    }
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-start reason=no-results empty_reads={} since_transcript_ms={} active_since_transcript_ms={} mode={}",
                        consecutive_empty_reads,
                        last_transcription_time.elapsed().as_millis(),
                        samples_to_ms(active_samples_since_transcript),
                        audio_mode.as_str()
                    );
                    if !reconnect!() {
                        crate::log_info!(
                            "[RealtimeGeminiLiveHealth] reconnect-failed reason=no-results"
                        );
                        break;
                    }
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-ok reason=no-results count={} catchup_ms={}",
                        reconnect_count,
                        samples_to_ms(silence_buffer.len())
                    );
                }
            }
            Ok(LivePoll::ServerError(error)) => {
                if !error.retryable {
                    return Err(anyhow::anyhow!("Gemini Live server error: {error}"));
                }
                show_audio_gap(&mut gap_notified);
                if !reconnect!() {
                    break;
                }
            }
            Ok(LivePoll::Unparsed { .. }) => {}
            Err(e) => {
                let error_str = e.to_string();
                if is_recoverable_anyhow_socket_error(&e) {
                    show_audio_gap(&mut gap_notified);
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-start reason=socket-error error={} empty_reads={} since_transcript_ms={} mode={}",
                        error_str,
                        consecutive_empty_reads,
                        last_transcription_time.elapsed().as_millis(),
                        audio_mode.as_str()
                    );
                    if !reconnect!() {
                        crate::log_info!(
                            "[RealtimeGeminiLiveHealth] reconnect-failed reason=socket-error error={}",
                            error_str
                        );
                        break;
                    }
                    crate::log_info!(
                        "[RealtimeGeminiLiveHealth] reconnect-ok reason=socket-error count={} catchup_ms={}",
                        reconnect_count,
                        samples_to_ms(silence_buffer.len())
                    );
                } else {
                    return Err(e);
                }
            }
        }

        if last_health_log.elapsed() >= Duration::from_secs(5) {
            crate::log_info!(
                "[RealtimeGeminiLiveHealth] uptime_ms={} model={} capture={} mode={} mode_ms={} sent_chunks={} sent_ms={} active_ms={} active_since_transcript_ms={} catchup_ms={} transcript_updates={} transcript_chars={} empty_reads={} since_transcript_ms={} reconnects={}",
                session_started.elapsed().as_millis(),
                gemini_live_model,
                capture_label,
                audio_mode.as_str(),
                mode_start.elapsed().as_millis(),
                sent_chunks,
                samples_to_ms(sent_samples),
                samples_to_ms(active_sent_samples),
                samples_to_ms(active_samples_since_transcript),
                samples_to_ms(silence_buffer.len()),
                transcript_updates,
                transcript_chars,
                consecutive_empty_reads,
                last_transcription_time.elapsed().as_millis(),
                reconnect_count
            );
            last_health_log = Instant::now();
            sent_chunks = 0;
            sent_samples = 0;
            active_sent_samples = 0;
            transcript_updates = 0;
            transcript_chars = 0;
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    let _ = session.close();
    Ok(())
}

const ACTIVE_AUDIO_RMS_THRESHOLD: f32 = 0.004;

fn send_audio(session: &mut ReadyLiveSession, samples: &[i16]) -> Result<()> {
    session.send_audio_pcm(samples, 16_000)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
