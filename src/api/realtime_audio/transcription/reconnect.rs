use crate::api::audio::retention::append_pending;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::api::gemini_live::lifecycle::LiveBackoffPolicy;
use crate::api::gemini_live::ready_session::ReadyLiveSession;
use crate::api::gemini_live::setup_failure::{
    LIVE_TRANSCRIPTION_RECONNECT_ATTEMPTS, fatal_setup_error,
};
use crate::api::realtime_audio::DEVICE_RECONNECT_REQUESTED;

use super::main_loop::AudioMode;

pub(super) struct ReconnectContext<'a> {
    pub(super) session: &'a mut ReadyLiveSession,
    pub(super) api_key: &'a str,
    pub(super) model: &'a str,
    pub(super) audio_buffer: &'a Arc<Mutex<Vec<i16>>>,
    pub(super) silence_buffer: &'a mut Vec<i16>,
    pub(super) audio_mode: &'a mut AudioMode,
    pub(super) mode_start: &'a mut Instant,
    pub(super) last_transcription_time: &'a mut Instant,
    pub(super) consecutive_empty_reads: &'a mut u32,
    pub(super) stop_signal: &'a Arc<AtomicBool>,
    pub(super) resumption_handle: &'a mut Option<String>,
    pub(super) gap_notified: &'a mut bool,
}

const MAX_RECONNECT_ATTEMPTS: u32 = LIVE_TRANSCRIPTION_RECONNECT_ATTEMPTS;

pub(super) fn try_reconnect(context: ReconnectContext<'_>) -> anyhow::Result<bool> {
    let mut reconnect_buffer = std::mem::take(context.silence_buffer);
    let _ = context.session.close();

    for attempt in 0..MAX_RECONNECT_ATTEMPTS {
        if reconnect_cancelled(context.stop_signal) {
            return Ok(false);
        }
        let captured = std::mem::take(&mut *context.audio_buffer.lock().unwrap());
        if capture_may_have_dropped(&captured) {
            show_audio_gap(context.gap_notified);
        }
        retain_audio(&mut reconnect_buffer, &captured, context.gap_notified);
        let (_, vocabulary) = super::dedicated::vocabulary_snapshot();
        let handle = (attempt == 0)
            .then_some(context.resumption_handle.as_deref())
            .flatten();
        match super::open_ready_session(context.api_key, context.model, &vocabulary, handle, || {
            reconnect_cancelled(context.stop_signal)
        }) {
            Ok(new_session) => {
                let captured = std::mem::take(&mut *context.audio_buffer.lock().unwrap());
                if capture_may_have_dropped(&captured) {
                    show_audio_gap(context.gap_notified);
                }
                retain_audio(&mut reconnect_buffer, &captured, context.gap_notified);
                context.silence_buffer.clear();
                retain_audio(
                    context.silence_buffer,
                    &reconnect_buffer,
                    context.gap_notified,
                );
                *context.audio_mode = AudioMode::CatchUp;
                *context.mode_start = Instant::now();
                *context.session = new_session;
                *context.last_transcription_time = Instant::now();
                *context.consecutive_empty_reads = 0;
                return Ok(true);
            }
            Err(error) => {
                let had_resumption = attempt == 0 && context.resumption_handle.is_some();
                if attempt == 0 {
                    *context.resumption_handle = None;
                }
                if reconnect_cancelled(context.stop_signal) {
                    return Ok(false);
                }
                if !had_resumption && fatal_setup_error(&error) {
                    return Err(error);
                }
                if attempt + 1 == MAX_RECONNECT_ATTEMPTS {
                    break;
                }
                let wait = Duration::from_millis(LiveBackoffPolicy::default().delay_ms(attempt));
                let deadline = Instant::now() + wait;
                while Instant::now() < deadline {
                    if reconnect_cancelled(context.stop_signal) {
                        return Ok(false);
                    }
                    std::thread::sleep(
                        Duration::from_millis(20)
                            .min(deadline.saturating_duration_since(Instant::now())),
                    );
                }
            }
        }
    }
    Err(anyhow::anyhow!("Live transcription could not reconnect"))
}

pub(super) fn retain_audio(buffer: &mut Vec<i16>, audio: &[i16], gap_notified: &mut bool) {
    let dropped = merge_audio(buffer, audio);
    if dropped > 0 {
        crate::log_info!("[RealtimeGeminiLiveHealth] pending_audio_dropped_samples={dropped}");
        show_audio_gap(gap_notified);
    }
}

pub(super) fn capture_may_have_dropped(audio: &[i16]) -> bool {
    audio.len() >= crate::api::audio::retention::MAX_PENDING_SAMPLES
}

fn merge_audio(buffer: &mut Vec<i16>, audio: &[i16]) -> usize {
    append_pending(buffer, audio)
}

pub(super) fn show_audio_gap(gap_notified: &mut bool) {
    if *gap_notified {
        return;
    }
    *gap_notified = true;
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_default();
    let message = crate::gui::locale::LocaleText::get(&language)
        .realtime
        .realtime_audio_gap;
    crate::overlay::auto_copy_badge::show_error_notification(message);
}

fn reconnect_cancelled(stop_signal: &Arc<AtomicBool>) -> bool {
    super::setup_cancelled(stop_signal) || DEVICE_RECONNECT_REQUESTED.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::gemini_live::ready_session::LiveSetupServerError;
    #[test]
    fn shared_retry_schedule_and_fatal_classification() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/gemini-live-session/transcription-send-recovery.json"
        )))
        .unwrap();
        assert_eq!(fixture["maxReconnectAttempts"], MAX_RECONNECT_ATTEMPTS);
        for item in fixture["attempts"].as_array().unwrap() {
            assert_eq!(
                LiveBackoffPolicy::default().delay_ms(item["attempt"].as_u64().unwrap() as u32),
                item["delayMs"].as_u64().unwrap()
            );
        }
        let fatal = LiveSetupServerError {
            server: crate::api::gemini_live::ready_session::LiveServerError {
                message: "invalid setup".into(),
                retryable: false,
            },
        };
        let temporary = LiveSetupServerError {
            server: crate::api::gemini_live::ready_session::LiveServerError {
                message: "unavailable".into(),
                retryable: true,
            },
        };
        assert!(fatal_setup_error(&fatal.into()));
        assert!(!fatal_setup_error(&temporary.into()));
    }

    #[test]
    fn capture_and_replay_overflow_are_detected_and_keep_newest_audio() {
        let limit = crate::api::audio::retention::MAX_PENDING_SAMPLES;
        assert!(!capture_may_have_dropped(&vec![0; limit - 1]));
        assert!(capture_may_have_dropped(&vec![0; limit]));
        let mut pending = vec![1; limit - 2];
        assert_eq!(merge_audio(&mut pending, &[2, 3, 4]), 1);
        assert_eq!(pending.len(), limit);
        assert_eq!(&pending[pending.len() - 3..], &[2, 3, 4]);
    }
}
