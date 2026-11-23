use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::api::audio::retention::retain_pending;
use crate::api::gemini_live::ready_session::LivePoll;
use crate::api::gemini_live::transcription_recovery::TranscriptionRecovery;
use crate::api::gemini_live::transport::is_recoverable_anyhow_socket_error;
use crate::config::Preset;

mod reconnect;
mod seam;
mod tail;
use reconnect::{ReconnectContext, ReconnectOutcome, requeue_unsent, try_reconnect};
pub(super) use seam::{Reopen, StreamClock, StreamSession, SystemClock};
pub(super) use tail::{FinalTranscriptionsContext, wait_for_final_transcriptions};

#[derive(Clone, Copy, PartialEq)]
enum AudioMode {
    Normal,
    Silence,
    CatchUp,
}

/// Main streaming loop - sends audio and receives transcriptions.
pub(super) struct StreamingLoopContext<'a, S, F> {
    pub(super) preset: &'a Preset,
    pub(super) session: &'a mut S,
    /// Opens the session that replaces a failed or rotated one.
    pub(super) reopen: &'a mut Reopen<'a, S>,
    pub(super) model: &'a str,
    pub(super) audio_buffer: &'a Arc<Mutex<Vec<i16>>>,
    pub(super) accumulated_text: &'a Arc<Mutex<String>>,
    pub(super) transcribe_text: &'a mut crate::api::gemini_transcribe::TranscriptState,
    pub(super) auto_paste: &'a crate::overlay::utils::StreamingAutoPaste,
    pub(super) stop_signal: &'a Arc<AtomicBool>,
    pub(super) pause_signal: &'a Arc<AtomicBool>,
    pub(super) abort_signal: &'a Arc<AtomicBool>,
    pub(super) overlay_hwnd: HWND,
    pub(super) update_stream_text: &'a F,
    pub(super) clock: &'a dyn StreamClock,
}

/// Runs until the user stops; returns the error that ended the stream early
/// (a reconnect that could not recover) so the caller can report it.
pub(super) fn run_streaming_loop<S, F>(
    context: StreamingLoopContext<'_, S, F>,
) -> Option<anyhow::Error>
where
    S: StreamSession,
    F: Fn(&str),
{
    let StreamingLoopContext {
        preset,
        session,
        reopen,
        model,
        audio_buffer,
        accumulated_text,
        transcribe_text,
        auto_paste,
        stop_signal,
        pause_signal,
        abort_signal,
        overlay_hwnd,
        update_stream_text,
        clock,
    } = context;
    const CHUNK_SIZE: usize = 1600;
    const NORMAL_DURATION: Duration = Duration::from_secs(20);
    const SILENCE_DURATION: Duration = Duration::from_secs(2);
    const SAMPLES_PER_100MS: usize = 1600;
    const EMPTY_READ_CHECK_COUNT: u32 = 50;
    let since = |moment: Instant| clock.now().saturating_duration_since(moment);

    let mut last_send = clock.now();
    let send_interval = Duration::from_millis(100);
    let auto_stop = preset.auto_stop_recording;
    let mut has_spoken = false;
    let mut first_speech: Option<Instant> = None;
    let mut last_active = clock.now();

    let mut audio_mode = AudioMode::Normal;
    let mut mode_start = clock.now();
    let mut silence_buffer =
        super::pending_audio::PendingAudio::new(audio_buffer.clone(), abort_signal.clone());
    let mut consecutive_empty_reads: u32 = 0;
    let uses_interim_transcripts = crate::api::gemini_transcribe::is_live_transcribe(model);
    let started = clock.now();
    let mut recovery = TranscriptionRecovery::default();
    let mut dedicated_vad = crate::api::gemini_transcribe::HybridVad::default();
    let mut connection_started = clock.now();
    let mut resumption_handle: Option<String> = None;

    // Every transport failure (send, flush, read, retryable server error)
    // takes the same bounded reconnect path with unsent audio replayed first.
    macro_rules! reconnect_or_exit {
        () => {
            match try_reconnect(ReconnectContext {
                session: &mut *session,
                reopen: &mut *reopen,
                audio_buffer,
                silence_buffer: &mut silence_buffer,
                audio_mode: &mut audio_mode,
                mode_start: &mut mode_start,
                consecutive_empty_reads: &mut consecutive_empty_reads,
                recovery: &mut recovery,
                dedicated_vad: &mut dedicated_vad,
                resumption_handle: &mut resumption_handle,
                stop_signal,
                abort_signal,
                clock,
            }) {
                ReconnectOutcome::Reconnected => connection_started = clock.now(),
                ReconnectOutcome::Cancelled => return None,
                ReconnectOutcome::Failed(error) => return Some(error),
            }
        };
    }

    while !stop_signal.load(Ordering::SeqCst) && !abort_signal.load(Ordering::SeqCst) {
        if !preset.hide_recording_ui && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            break;
        }
        if crate::api::gemini_transcribe::proactive_rotation_due(
            uses_interim_transcripts,
            since(connection_started),
            false,
            dedicated_vad.is_safe_gap(),
        ) {
            reconnect_or_exit!();
        }

        match audio_mode {
            AudioMode::Normal => {
                if !uses_interim_transcripts && since(mode_start) >= NORMAL_DURATION {
                    audio_mode = AudioMode::Silence;
                    mode_start = clock.now();
                    silence_buffer.clear();
                }
            }
            AudioMode::Silence => {
                if since(mode_start) >= SILENCE_DURATION {
                    recovery.input_flushed(since(started).as_millis() as u64);
                    audio_mode = AudioMode::CatchUp;
                    mode_start = clock.now();
                }
            }
            AudioMode::CatchUp => {
                if silence_buffer.is_empty() {
                    audio_mode = AudioMode::Normal;
                    mode_start = clock.now();
                }
            }
        }

        if since(last_send) >= send_interval {
            let real_audio: Vec<i16> = {
                let mut buf = audio_buffer.lock().unwrap();
                std::mem::take(&mut *buf)
            };

            // Samples that did not reach the socket, or a failed flush.
            let mut unsent: Option<Vec<i16>> = None;
            match audio_mode {
                AudioMode::Normal => {
                    if !real_audio.is_empty() && !pause_signal.load(Ordering::Relaxed) {
                        for (index, chunk) in real_audio.chunks(CHUNK_SIZE).enumerate() {
                            if session.send_audio(chunk).is_err() {
                                unsent = Some(real_audio[index * CHUNK_SIZE..].to_vec());
                                break;
                            }
                            observe_sent_audio(&mut recovery, chunk);
                            if uses_interim_transcripts
                                && super::speech_boundaries::flush_completed_speech(
                                    &mut dedicated_vad,
                                    Some(chunk),
                                    clock.now(),
                                    || session.end_audio_stream(),
                                )
                                .is_err()
                            {
                                let rest = real_audio.get((index + 1) * CHUNK_SIZE..);
                                unsent = Some(rest.unwrap_or_default().to_vec());
                                break;
                            }
                        }
                    }
                }
                AudioMode::Silence => {
                    retain_pending(&mut silence_buffer, &real_audio);
                    let silence: Vec<i16> = vec![0i16; SAMPLES_PER_100MS];
                    if session.send_audio(&silence).is_err() {
                        unsent = Some(Vec::new());
                    }
                }
                AudioMode::CatchUp => {
                    retain_pending(&mut silence_buffer, &real_audio);
                    let double_chunk = SAMPLES_PER_100MS * 2;
                    let to_send: Vec<i16> = if silence_buffer.len() >= double_chunk {
                        silence_buffer.drain(..double_chunk).collect()
                    } else if !silence_buffer.is_empty() {
                        std::mem::take(&mut *silence_buffer)
                    } else {
                        Vec::new()
                    };
                    if !to_send.is_empty() && session.send_audio(&to_send).is_err() {
                        unsent = Some(to_send);
                    } else {
                        observe_sent_audio(&mut recovery, &to_send);
                        if uses_interim_transcripts
                            && !to_send.is_empty()
                            && super::speech_boundaries::flush_completed_speech(
                                &mut dedicated_vad,
                                Some(&to_send),
                                clock.now(),
                                || session.end_audio_stream(),
                            )
                            .is_err()
                        {
                            unsent = Some(Vec::new());
                        }
                    }
                }
            }
            last_send = clock.now();
            if let Some(unsent) = unsent {
                requeue_unsent(&mut silence_buffer, &unsent);
                reconnect_or_exit!();
                continue;
            }
        }

        if uses_interim_transcripts
            && super::speech_boundaries::flush_completed_speech(
                &mut dedicated_vad,
                None,
                clock.now(),
                || session.end_audio_stream(),
            )
            .is_err()
        {
            reconnect_or_exit!();
            continue;
        }

        loop {
            match session.poll() {
                Ok(LivePoll::Frame(frame)) => {
                    if frame.content_count() > 0 || frame.response_complete() || frame.interrupted {
                        recovery.reset();
                    }
                    crate::api::gemini_transcribe::observe_resumption(
                        &mut resumption_handle,
                        frame.session_resumption.as_ref(),
                    );
                    let go_away = crate::api::gemini_transcribe::rotates_on_go_away(
                        uses_interim_transcripts,
                        frame.go_away.is_some(),
                    );
                    if uses_interim_transcripts {
                        let has_update = frame.interim_input_transcript.is_some()
                            || frame.input_transcript.is_some();
                        let delta = transcribe_text.apply_update(
                            frame.interim_input_transcript.as_deref(),
                            frame.input_transcript.as_deref(),
                        );
                        if has_update {
                            consecutive_empty_reads = 0;
                            update_stream_text(&transcribe_text.display());
                        }
                        if delta.is_none()
                            && frame.interim_input_transcript.is_some()
                            && !abort_signal.load(Ordering::Relaxed)
                        {
                            auto_paste.interim(
                                &transcribe_text.display()[transcribe_text.committed().len()..],
                            );
                        }
                        if let Some(delta) = delta {
                            if let Ok(mut txt) = accumulated_text.lock() {
                                txt.clear();
                                txt.push_str(transcribe_text.committed());
                            }
                            crate::log_info!(
                                "[GeminiLiveStream] authoritative_final chars={} recording_active=true",
                                delta.chars().count()
                            );
                            if !abort_signal.load(Ordering::Relaxed) {
                                auto_paste.final_text(&delta);
                            }
                        }
                    } else if let Some(t) = frame.input_transcript
                        && !t.is_empty()
                    {
                        consecutive_empty_reads = 0;
                        if let Ok(mut txt) = accumulated_text.lock() {
                            txt.push_str(&t);
                            update_stream_text(&txt);
                        }
                        if !abort_signal.load(Ordering::Relaxed) {
                            auto_paste.final_text(&t);
                        }
                    }
                    if go_away {
                        reconnect_or_exit!();
                    }
                }
                Ok(LivePoll::PeerClosed(_)) => reconnect_or_exit!(),
                Ok(LivePoll::Idle) => {
                    consecutive_empty_reads += 1;
                    if !uses_interim_transcripts
                        && consecutive_empty_reads >= EMPTY_READ_CHECK_COUNT
                        && recovery.should_reconnect(since(started).as_millis() as u64)
                    {
                        reconnect_or_exit!();
                    }
                    break;
                }
                Ok(LivePoll::ServerError(error)) => {
                    eprintln!("[GeminiLiveStream] Server error: {error}");
                    if !error.retryable {
                        return Some(anyhow::anyhow!("Gemini Live error: {error}"));
                    }
                    reconnect_or_exit!();
                }
                Ok(LivePoll::Unparsed { .. }) => {}
                Err(e) => {
                    if !is_recoverable_anyhow_socket_error(&e) {
                        return Some(e);
                    }
                    reconnect_or_exit!();
                }
            }
        }

        if auto_stop && !pause_signal.load(Ordering::Relaxed) {
            let rms =
                f32::from_bits(crate::overlay::recording::CURRENT_RMS.load(Ordering::Relaxed));
            if crate::api::audio::activity::auto_stop_activity(rms) {
                if !has_spoken {
                    first_speech = Some(clock.now());
                }
                has_spoken = true;
                last_active = clock.now();
            } else if has_spoken
                && first_speech.map(|t| since(t).as_millis()).unwrap_or(0) >= 2000
                && since(last_active).as_millis() > 800
            {
                stop_signal.store(true, Ordering::SeqCst);
            }
        }
        clock.sleep(Duration::from_millis(10));
    }
    None
}

fn observe_sent_audio(recovery: &mut TranscriptionRecovery, samples: &[i16]) {
    if crate::api::gemini_transcribe::compute_i16_rms(samples) >= 0.004 {
        recovery.audio_sent(crate::api::gemini_transcribe::samples_to_ms(samples.len()));
    }
}

#[cfg(test)]
mod loop_tests;
