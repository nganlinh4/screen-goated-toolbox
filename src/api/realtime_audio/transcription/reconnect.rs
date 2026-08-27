use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::api::gemini_live::ready_session::ReadyLiveSession;
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
}

pub(super) fn try_reconnect(context: ReconnectContext<'_>) -> bool {
    let mut reconnect_buffer = Vec::new();
    let _ = context.session.close();

    for attempt in 1..=3 {
        if reconnect_cancelled(context.stop_signal) {
            return false;
        }
        reconnect_buffer.extend(std::mem::take(&mut *context.audio_buffer.lock().unwrap()));
        let (_, vocabulary) = super::dedicated::vocabulary_snapshot();
        let handle = (attempt == 1)
            .then_some(context.resumption_handle.as_deref())
            .flatten();
        match super::open_ready_session(context.api_key, context.model, &vocabulary, handle, || {
            reconnect_cancelled(context.stop_signal)
        }) {
            Ok(new_session) => {
                reconnect_buffer.extend(std::mem::take(&mut *context.audio_buffer.lock().unwrap()));
                context.silence_buffer.clear();
                context.silence_buffer.extend(reconnect_buffer);
                *context.audio_mode = AudioMode::CatchUp;
                *context.mode_start = Instant::now();
                *context.session = new_session;
                *context.last_transcription_time = Instant::now();
                *context.consecutive_empty_reads = 0;
                return true;
            }
            Err(_) => {
                if attempt == 1 {
                    *context.resumption_handle = None;
                }
                if reconnect_cancelled(context.stop_signal) {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    false
}

fn reconnect_cancelled(stop_signal: &Arc<AtomicBool>) -> bool {
    super::setup_cancelled(stop_signal) || DEVICE_RECONNECT_REQUESTED.load(Ordering::SeqCst)
}
