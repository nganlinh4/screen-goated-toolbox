//! The bounded wait for final transcriptions after recording stops.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

use super::seam::{StreamClock, StreamSession};
use crate::api::gemini_live::ready_session::LivePoll;
use crate::overlay::result::update_window_text;

/// Wait for final transcriptions after recording stops.
pub(in super::super) struct FinalTranscriptionsContext<'a> {
    pub(in super::super) session: &'a mut dyn StreamSession,
    pub(in super::super) accumulated_text: &'a Arc<Mutex<String>>,
    pub(in super::super) transcribe_text: &'a mut crate::api::gemini_transcribe::TranscriptState,
    pub(in super::super) auto_paste: &'a crate::overlay::utils::StreamingAutoPaste,
    pub(in super::super) uses_interim_transcripts: bool,
    pub(in super::super) streaming_hwnd: Option<HWND>,
    pub(in super::super) abort_signal: &'a AtomicBool,
    pub(in super::super) clock: &'a dyn StreamClock,
}

pub(in super::super) fn wait_for_final_transcriptions(context: FinalTranscriptionsContext<'_>) {
    let FinalTranscriptionsContext {
        session,
        accumulated_text,
        transcribe_text,
        auto_paste,
        uses_interim_transcripts,
        streaming_hwnd,
        abort_signal,
        clock,
    } = context;
    let mut conclude_end = clock.now() + Duration::from_millis(1200);
    let max_stop_time = clock.now() + Duration::from_millis(5000);
    let extension = Duration::from_millis(700);

    println!("[GeminiLiveStream] Waiting for tail...");

    while clock.now() < conclude_end && clock.now() < max_stop_time {
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
                    conclude_end =
                        extend_tail_deadline(conclude_end, clock.now(), extension, max_stop_time);
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
                    conclude_end =
                        extend_tail_deadline(conclude_end, clock.now(), extension, max_stop_time);
                }
            }
            Ok(LivePoll::Idle) => {
                clock.sleep(Duration::from_millis(20));
            }
            Ok(LivePoll::Unparsed { .. }) => {}
            Ok(LivePoll::PeerClosed(_) | LivePoll::ServerError(_)) | Err(_) => break,
        }
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
    use super::extend_tail_deadline;
    use std::time::{Duration, Instant};

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
