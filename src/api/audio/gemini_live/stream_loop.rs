use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::api::audio::retention::retain_pending;
use crate::api::gemini_live::ready_session::{LivePoll, ReadyLiveSession};
use crate::api::gemini_live::transcription_recovery::TranscriptionRecovery;
use crate::api::gemini_live::transport::is_recoverable_anyhow_socket_error;
use crate::config::Preset;
use crate::overlay::result::update_window_text;

#[derive(Clone, Copy, PartialEq)]
enum AudioMode {
    Normal,
    Silence,
    CatchUp,
}

struct ReconnectContext<'a> {
    session: &'a mut ReadyLiveSession,
    api_key: &'a str,
    model: &'a str,
    vocabulary: &'a [String],
    audio_buffer: &'a Arc<Mutex<Vec<i16>>>,
    silence_buffer: &'a mut Vec<i16>,
    audio_mode: &'a mut AudioMode,
    mode_start: &'a mut Instant,
    consecutive_empty_reads: &'a mut u32,
    recovery: &'a mut TranscriptionRecovery,
    dedicated_vad: &'a mut crate::api::gemini_transcribe::HybridVad,
    stop_signal: &'a Arc<AtomicBool>,
    abort_signal: &'a Arc<AtomicBool>,
}

fn try_reconnect(context: ReconnectContext<'_>) -> bool {
    let ReconnectContext {
        session,
        api_key,
        model,
        vocabulary,
        audio_buffer,
        silence_buffer,
        audio_mode,
        mode_start,
        consecutive_empty_reads,
        recovery,
        dedicated_vad,
        stop_signal,
        abort_signal,
    } = context;
    // Older unsent audio must remain ahead of samples captured during reconnect.
    let mut reconnect_buffer = std::mem::take(silence_buffer);
    let _ = session.close();

    loop {
        if stop_signal.load(Ordering::Relaxed) || abort_signal.load(Ordering::Relaxed) {
            println!("[GeminiLiveStream] Cancellation received during reconnection.");
            retain_pending(silence_buffer, &reconnect_buffer);
            return false;
        }

        {
            let mut buf = audio_buffer.lock().unwrap();
            retain_pending(&mut reconnect_buffer, &std::mem::take(&mut *buf));
        }

        match super::open_ready_session(api_key, model, vocabulary, || {
            stop_signal.load(Ordering::Relaxed) || abort_signal.load(Ordering::Relaxed)
        }) {
            Ok(new_session) => {
                {
                    let mut buf = audio_buffer.lock().unwrap();
                    retain_pending(&mut reconnect_buffer, &std::mem::take(&mut *buf));
                }

                silence_buffer.clear();
                retain_pending(silence_buffer, &reconnect_buffer);
                *audio_mode = AudioMode::CatchUp;
                *mode_start = Instant::now();
                *session = new_session;
                *consecutive_empty_reads = 0;
                recovery.reset();
                dedicated_vad.reset_connection();

                return true;
            }
            Err(e) => {
                if stop_signal.load(Ordering::Relaxed) || abort_signal.load(Ordering::Relaxed) {
                    retain_pending(silence_buffer, &reconnect_buffer);
                    return false;
                }
                println!(
                    "[GeminiLiveStream] Reconnection failed: {}. Retrying in 1s...",
                    e
                );
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

/// Main streaming loop - sends audio and receives transcriptions.
pub(super) struct StreamingLoopContext<'a, F> {
    pub(super) preset: &'a Preset,
    pub(super) session: &'a mut ReadyLiveSession,
    pub(super) api_key: &'a str,
    pub(super) model: &'a str,
    pub(super) vocabulary: &'a [String],
    pub(super) audio_buffer: &'a Arc<Mutex<Vec<i16>>>,
    pub(super) accumulated_text: &'a Arc<Mutex<String>>,
    pub(super) transcribe_text: &'a mut crate::api::gemini_transcribe::TranscriptState,
    pub(super) auto_paste: &'a crate::overlay::utils::StreamingAutoPaste,
    pub(super) stop_signal: &'a Arc<AtomicBool>,
    pub(super) pause_signal: &'a Arc<AtomicBool>,
    pub(super) abort_signal: &'a Arc<AtomicBool>,
    pub(super) overlay_hwnd: HWND,
    pub(super) update_stream_text: &'a F,
}

pub(super) fn run_streaming_loop<F>(context: StreamingLoopContext<'_, F>)
where
    F: Fn(&str),
{
    let StreamingLoopContext {
        preset,
        session,
        api_key,
        model,
        vocabulary,
        audio_buffer,
        accumulated_text,
        transcribe_text,
        auto_paste,
        stop_signal,
        pause_signal,
        abort_signal,
        overlay_hwnd,
        update_stream_text,
    } = context;
    const CHUNK_SIZE: usize = 1600;
    const NORMAL_DURATION: Duration = Duration::from_secs(20);
    const SILENCE_DURATION: Duration = Duration::from_secs(2);
    const SAMPLES_PER_100MS: usize = 1600;
    const EMPTY_READ_CHECK_COUNT: u32 = 50;

    let mut last_send = Instant::now();
    let send_interval = Duration::from_millis(100);
    let auto_stop = preset.auto_stop_recording;
    let mut has_spoken = false;
    let mut first_speech: Option<Instant> = None;
    let mut last_active = Instant::now();

    let mut audio_mode = AudioMode::Normal;
    let mut mode_start = Instant::now();
    let mut silence_buffer =
        super::pending_audio::PendingAudio::new(audio_buffer.clone(), abort_signal.clone());
    let mut consecutive_empty_reads: u32 = 0;
    let uses_interim_transcripts = crate::api::gemini_transcribe::is_live_transcribe(model);
    let started = Instant::now();
    let mut recovery = TranscriptionRecovery::default();
    let mut dedicated_vad = crate::api::gemini_transcribe::HybridVad::default();

    while !stop_signal.load(Ordering::SeqCst) && !abort_signal.load(Ordering::SeqCst) {
        if !preset.hide_recording_ui && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            break;
        }

        match audio_mode {
            AudioMode::Normal => {
                if !uses_interim_transcripts && mode_start.elapsed() >= NORMAL_DURATION {
                    audio_mode = AudioMode::Silence;
                    mode_start = Instant::now();
                    silence_buffer.clear();
                }
            }
            AudioMode::Silence => {
                if mode_start.elapsed() >= SILENCE_DURATION {
                    recovery.input_flushed(started.elapsed().as_millis() as u64);
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

        if last_send.elapsed() >= send_interval {
            let real_audio: Vec<i16> = {
                let mut buf = audio_buffer.lock().unwrap();
                std::mem::take(&mut *buf)
            };

            match audio_mode {
                AudioMode::Normal => {
                    if !real_audio.is_empty() && !pause_signal.load(Ordering::Relaxed) {
                        for chunk in real_audio.chunks(CHUNK_SIZE) {
                            if send_audio(session, chunk).is_err() {
                                break;
                            }
                            observe_sent_audio(&mut recovery, chunk);
                            if uses_interim_transcripts
                                && super::speech_boundaries::flush_completed_speech(
                                    &mut dedicated_vad,
                                    Some(chunk),
                                    Instant::now(),
                                    || session.end_audio_stream(),
                                )
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                }
                AudioMode::Silence => {
                    retain_pending(&mut silence_buffer, &real_audio);
                    let silence: Vec<i16> = vec![0i16; SAMPLES_PER_100MS];
                    if send_audio(session, &silence).is_err() {
                        break;
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
                    if !to_send.is_empty() && send_audio(session, &to_send).is_err() {
                        break;
                    }
                    observe_sent_audio(&mut recovery, &to_send);
                    if uses_interim_transcripts
                        && !to_send.is_empty()
                        && super::speech_boundaries::flush_completed_speech(
                            &mut dedicated_vad,
                            Some(&to_send),
                            Instant::now(),
                            || session.end_audio_stream(),
                        )
                        .is_err()
                    {
                        return;
                    }
                }
            }
            last_send = Instant::now();
        }

        if uses_interim_transcripts
            && super::speech_boundaries::flush_completed_speech(
                &mut dedicated_vad,
                None,
                Instant::now(),
                || session.end_audio_stream(),
            )
            .is_err()
        {
            return;
        }

        loop {
            match session.poll() {
                Ok(LivePoll::Frame(frame)) => {
                    if frame.content_count() > 0 || frame.response_complete() || frame.interrupted {
                        recovery.reset();
                    }
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
                }
                Ok(LivePoll::PeerClosed(_)) => {
                    if !try_reconnect(ReconnectContext {
                        session,
                        api_key,
                        model,
                        vocabulary,
                        audio_buffer,
                        silence_buffer: &mut silence_buffer,
                        audio_mode: &mut audio_mode,
                        mode_start: &mut mode_start,
                        consecutive_empty_reads: &mut consecutive_empty_reads,
                        recovery: &mut recovery,
                        dedicated_vad: &mut dedicated_vad,
                        stop_signal,
                        abort_signal,
                    }) {
                        return;
                    }
                }
                Ok(LivePoll::Idle) => {
                    consecutive_empty_reads += 1;
                    if !uses_interim_transcripts
                        && consecutive_empty_reads >= EMPTY_READ_CHECK_COUNT
                        && recovery.should_reconnect(started.elapsed().as_millis() as u64)
                        && !try_reconnect(ReconnectContext {
                            session,
                            api_key,
                            model,
                            vocabulary,
                            audio_buffer,
                            silence_buffer: &mut silence_buffer,
                            audio_mode: &mut audio_mode,
                            mode_start: &mut mode_start,
                            consecutive_empty_reads: &mut consecutive_empty_reads,
                            recovery: &mut recovery,
                            dedicated_vad: &mut dedicated_vad,
                            stop_signal,
                            abort_signal,
                        })
                    {
                        return;
                    }
                    break;
                }
                Ok(LivePoll::ServerError(error)) => {
                    eprintln!("[GeminiLiveStream] Server error: {error}");
                    return;
                }
                Ok(LivePoll::Unparsed { .. }) => {}
                Err(e) => {
                    if is_recoverable_anyhow_socket_error(&e) {
                        if !try_reconnect(ReconnectContext {
                            session,
                            api_key,
                            model,
                            vocabulary,
                            audio_buffer,
                            silence_buffer: &mut silence_buffer,
                            audio_mode: &mut audio_mode,
                            mode_start: &mut mode_start,
                            consecutive_empty_reads: &mut consecutive_empty_reads,
                            recovery: &mut recovery,
                            dedicated_vad: &mut dedicated_vad,
                            stop_signal,
                            abort_signal,
                        }) {
                            return;
                        }
                    } else {
                        return;
                    }
                }
            }
        }

        if auto_stop && !pause_signal.load(Ordering::Relaxed) {
            let rms =
                f32::from_bits(crate::overlay::recording::CURRENT_RMS.load(Ordering::Relaxed));
            if crate::api::audio::activity::auto_stop_activity(rms) {
                if !has_spoken {
                    first_speech = Some(Instant::now());
                }
                has_spoken = true;
                last_active = Instant::now();
            } else if has_spoken
                && first_speech.map(|t| t.elapsed().as_millis()).unwrap_or(0) >= 2000
                && last_active.elapsed().as_millis() > 800
            {
                stop_signal.store(true, Ordering::SeqCst);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Wait for final transcriptions after recording stops.
pub(super) struct FinalTranscriptionsContext<'a> {
    pub(super) session: &'a mut ReadyLiveSession,
    pub(super) accumulated_text: &'a Arc<Mutex<String>>,
    pub(super) transcribe_text: &'a mut crate::api::gemini_transcribe::TranscriptState,
    pub(super) auto_paste: &'a crate::overlay::utils::StreamingAutoPaste,
    pub(super) uses_interim_transcripts: bool,
    pub(super) streaming_hwnd: Option<HWND>,
    pub(super) abort_signal: &'a AtomicBool,
}

pub(super) fn wait_for_final_transcriptions(context: FinalTranscriptionsContext<'_>) {
    let FinalTranscriptionsContext {
        session,
        accumulated_text,
        transcribe_text,
        auto_paste,
        uses_interim_transcripts,
        streaming_hwnd,
        abort_signal,
    } = context;
    let mut conclude_end = Instant::now() + Duration::from_millis(1200);
    let max_stop_time = Instant::now() + Duration::from_millis(5000);
    let extension = Duration::from_millis(700);

    println!("[GeminiLiveStream] Waiting for tail...");

    while Instant::now() < conclude_end && Instant::now() < max_stop_time {
        if abort_signal.load(Ordering::Relaxed) {
            break;
        }
        match session.poll() {
            Ok(LivePoll::Frame(frame)) => {
                if uses_interim_transcripts
                    && frame.input_transcript.is_none()
                    && let Some(interim) = frame.interim_input_transcript
                    && !interim.is_empty()
                {
                    transcribe_text.replace_interim(&interim);
                    if !abort_signal.load(Ordering::Relaxed) {
                        auto_paste.interim(
                            &transcribe_text.display()[transcribe_text.committed().len()..],
                        );
                    }
                    if let Some(h) = streaming_hwnd {
                        update_window_text(h, &transcribe_text.display());
                    }
                    conclude_end = extend_tail_deadline(
                        conclude_end,
                        Instant::now(),
                        extension,
                        max_stop_time,
                    );
                }
                if let Some(t) = frame.input_transcript
                    && !t.is_empty()
                {
                    let delta = if uses_interim_transcripts {
                        transcribe_text
                            .apply_update(None, Some(&t))
                            .unwrap_or_default()
                    } else {
                        t.clone()
                    };
                    if let Ok(mut txt) = accumulated_text.lock() {
                        if uses_interim_transcripts {
                            txt.clear();
                            txt.push_str(transcribe_text.committed());
                        } else {
                            txt.push_str(&t);
                        }
                        if let Some(h) = streaming_hwnd {
                            update_window_text(h, &txt);
                        }
                    }
                    if !abort_signal.load(Ordering::Relaxed) {
                        auto_paste.final_text(&delta);
                    }
                    conclude_end = extend_tail_deadline(
                        conclude_end,
                        Instant::now(),
                        extension,
                        max_stop_time,
                    );
                }
            }
            Ok(LivePoll::Idle) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(LivePoll::Unparsed { .. }) => {}
            Ok(LivePoll::PeerClosed(_) | LivePoll::ServerError(_)) | Err(_) => break,
        }
    }
}

fn send_audio(session: &mut ReadyLiveSession, samples: &[i16]) -> anyhow::Result<()> {
    session.send_audio_pcm(samples, 16_000)
}

fn observe_sent_audio(recovery: &mut TranscriptionRecovery, samples: &[i16]) {
    if crate::api::gemini_transcribe::compute_i16_rms(samples) >= 0.004 {
        recovery.audio_sent(crate::api::gemini_transcribe::samples_to_ms(samples.len()));
    }
}

fn extend_tail_deadline(
    current: Instant,
    now: Instant,
    extension: Duration,
    limit: Instant,
) -> Instant {
    current.max(now + extension).min(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_tail_response_extends_without_shortening_initial_grace() {
        let start = Instant::now();
        let initial = start + Duration::from_millis(1_200);
        let extension = Duration::from_millis(700);
        let limit = start + Duration::from_secs(5);
        assert_eq!(
            extend_tail_deadline(
                initial,
                start + Duration::from_millis(100),
                extension,
                limit
            ),
            initial
        );
        assert_eq!(
            extend_tail_deadline(initial, start + Duration::from_secs(1), extension, limit),
            start + Duration::from_millis(1_700)
        );
        assert_eq!(
            extend_tail_deadline(
                initial,
                start + Duration::from_millis(4_900),
                extension,
                limit
            ),
            limit
        );
    }
}
