//! Bounded reconnect for the preset Gemini Live dictation stream.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use super::AudioMode;
use super::seam::{Reopen, StreamClock, StreamSession};
use crate::api::audio::retention::retain_pending;
use crate::api::gemini_live::lifecycle::LiveBackoffPolicy;
use crate::api::gemini_live::setup_failure::{
    LIVE_TRANSCRIPTION_RECONNECT_ATTEMPTS, fatal_setup_error,
};
use crate::api::gemini_live::transcription_recovery::TranscriptionRecovery;

pub(super) struct ReconnectContext<'a, 'r, S> {
    pub session: &'a mut S,
    pub reopen: &'a mut Reopen<'r, S>,
    pub audio_buffer: &'a Arc<Mutex<Vec<i16>>>,
    pub silence_buffer: &'a mut Vec<i16>,
    pub audio_mode: &'a mut AudioMode,
    pub mode_start: &'a mut Instant,
    pub consecutive_empty_reads: &'a mut u32,
    pub recovery: &'a mut TranscriptionRecovery,
    pub dedicated_vad: &'a mut crate::api::gemini_transcribe::HybridVad,
    /// Consumed by the first attempt; a failed resume falls back to a fresh session.
    pub resumption_handle: &'a mut Option<String>,
    pub stop_signal: &'a Arc<AtomicBool>,
    pub abort_signal: &'a Arc<AtomicBool>,
    pub clock: &'a dyn StreamClock,
}

pub(super) enum ReconnectOutcome {
    Reconnected,
    Cancelled,
    /// The stream cannot continue; the error is reported to the user.
    Failed(anyhow::Error),
}

/// Puts samples that failed to send back ahead of everything still pending,
/// so a reconnect replays them first and in capture order.
pub(super) fn requeue_unsent(pending: &mut Vec<i16>, unsent: &[i16]) {
    if unsent.is_empty() {
        return;
    }
    let newer = std::mem::take(pending);
    retain_pending(pending, unsent);
    retain_pending(pending, &newer);
}

/// What to do after a failed reconnect attempt (zero-based).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum RetryVerdict {
    RetryAfter(Duration),
    GiveUp,
}

/// A rejected resumption handle is not fatal: the next attempt opens a fresh
/// session without it.
pub(super) fn retry_verdict(attempt: u32, error: &anyhow::Error, resumed: bool) -> RetryVerdict {
    if (!resumed && fatal_setup_error(error))
        || attempt + 1 >= LIVE_TRANSCRIPTION_RECONNECT_ATTEMPTS
    {
        return RetryVerdict::GiveUp;
    }
    RetryVerdict::RetryAfter(Duration::from_millis(
        LiveBackoffPolicy::default().delay_ms(attempt),
    ))
}

pub(super) fn try_reconnect<S: StreamSession>(
    context: ReconnectContext<'_, '_, S>,
) -> ReconnectOutcome {
    let ReconnectContext {
        session,
        reopen,
        audio_buffer,
        silence_buffer,
        audio_mode,
        mode_start,
        consecutive_empty_reads,
        recovery,
        dedicated_vad,
        resumption_handle,
        stop_signal,
        abort_signal,
        clock,
    } = context;
    let cancelled = || stop_signal.load(Ordering::Relaxed) || abort_signal.load(Ordering::Relaxed);
    // Older unsent audio must remain ahead of samples captured during reconnect.
    let mut reconnect_buffer = std::mem::take(silence_buffer);
    session.close();

    let mut attempt = 0;
    loop {
        if cancelled() {
            retain_pending(silence_buffer, &reconnect_buffer);
            return ReconnectOutcome::Cancelled;
        }
        {
            let mut buf = audio_buffer.lock().unwrap();
            retain_pending(&mut reconnect_buffer, &std::mem::take(&mut *buf));
        }
        let handle = resumption_handle.take();
        let resumed = handle.is_some();
        let opened = reopen(handle.as_deref(), &cancelled);
        let error = match opened {
            Ok(new_session) => {
                {
                    let mut buf = audio_buffer.lock().unwrap();
                    retain_pending(&mut reconnect_buffer, &std::mem::take(&mut *buf));
                }
                silence_buffer.clear();
                retain_pending(silence_buffer, &reconnect_buffer);
                *audio_mode = AudioMode::CatchUp;
                *mode_start = clock.now();
                *session = new_session;
                *consecutive_empty_reads = 0;
                recovery.reset();
                dedicated_vad.reset_connection();
                return ReconnectOutcome::Reconnected;
            }
            Err(error) => error,
        };
        if cancelled() {
            retain_pending(silence_buffer, &reconnect_buffer);
            return ReconnectOutcome::Cancelled;
        }
        let RetryVerdict::RetryAfter(wait) = retry_verdict(attempt, &error, resumed) else {
            retain_pending(silence_buffer, &reconnect_buffer);
            return ReconnectOutcome::Failed(error);
        };
        println!("[GeminiLiveStream] Reconnection failed: {error}. Retrying in {wait:?}...");
        let deadline = clock.now() + wait;
        while clock.now() < deadline && !cancelled() {
            clock.sleep(
                Duration::from_millis(20).min(deadline.saturating_duration_since(clock.now())),
            );
        }
        attempt += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsent_samples_replay_before_newer_pending_audio() {
        let mut pending = vec![3, 4];
        requeue_unsent(&mut pending, &[1, 2]);
        assert_eq!(pending, [1, 2, 3, 4]);
        requeue_unsent(&mut pending, &[]);
        assert_eq!(pending, [1, 2, 3, 4]);
    }

    #[test]
    fn reconnect_is_bounded_and_stops_on_fatal_setup_errors() {
        let transient = anyhow::anyhow!("read Gemini Live setup response: connection reset");
        assert_eq!(
            retry_verdict(0, &transient, false),
            RetryVerdict::RetryAfter(Duration::from_millis(
                LiveBackoffPolicy::default().delay_ms(0)
            ))
        );
        assert_eq!(
            retry_verdict(LIVE_TRANSCRIPTION_RECONNECT_ATTEMPTS - 1, &transient, false),
            RetryVerdict::GiveUp
        );
        let revoked =
            anyhow::anyhow!("Gemini Live peer closed during setup (code 1007: API key not valid)");
        assert_eq!(retry_verdict(0, &revoked, false), RetryVerdict::GiveUp);
    }

    #[test]
    fn rejected_resumption_falls_back_to_a_fresh_session() {
        let rejected = anyhow::Error::new(
            crate::api::gemini_live::ready_session::LiveSetupServerError {
                server: crate::api::gemini_live::ready_session::LiveServerError {
                    message: "invalid session handle".into(),
                    retryable: false,
                },
            },
        );
        assert_eq!(retry_verdict(0, &rejected, false), RetryVerdict::GiveUp);
        assert!(matches!(
            retry_verdict(0, &rejected, true),
            RetryVerdict::RetryAfter(_)
        ));
    }
}
