//! Shared state for realtime transcription overlay

use crate::api::realtime_audio::{RealtimeState, SharedRealtimeState};
#[cfg(not(feature = "recorder-worker"))]
pub use crate::win_types::HwndWrapper;
use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
};
use windows::Win32::Foundation::*;
pub const WM_APP_REALTIME_START: u32 = 0x0400 + 500; // WM_USER + 500
pub const WM_APP_REALTIME_HIDE: u32 = 0x0400 + 501; // WM_USER + 501

// Gap between realtime and translation overlays
pub const GAP: i32 = 20;

struct SessionCancellation {
    current: Mutex<Arc<AtomicBool>>,
}

impl SessionCancellation {
    fn new() -> Self {
        Self {
            current: Mutex::new(Arc::new(AtomicBool::new(false))),
        }
    }

    fn current(&self) -> Arc<AtomicBool> {
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn begin_session(&self) -> Arc<AtomicBool> {
        let signal = Arc::new(AtomicBool::new(false));
        let mut current = self
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        current.store(true, Ordering::SeqCst);
        *current = signal.clone();
        signal
    }

    fn set_current(&self, stopped: bool) {
        self.current().store(stopped, Ordering::SeqCst);
    }
}

static REALTIME_CANCELLATION: LazyLock<SessionCancellation> =
    LazyLock::new(SessionCancellation::new);
pub(crate) static REALTIME_SESSION_PUBLICATION: Mutex<()> = Mutex::new(());
pub(crate) static TRANSCRIPTION_TRANSITION: Mutex<()> = Mutex::new(());
pub(crate) static TRANSLATION_OPTIONS_REVISION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static TRANSCRIPTION_ENGINE_REVISION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

pub(crate) fn engine_is_current(engine_revision: u64) -> bool {
    TRANSCRIPTION_ENGINE_REVISION.load(Ordering::SeqCst) == engine_revision
}

pub fn current_stop_signal() -> Arc<AtomicBool> {
    REALTIME_CANCELLATION.current()
}

pub fn begin_stop_signal_session() -> Arc<AtomicBool> {
    REALTIME_CANCELLATION.begin_session()
}

pub(crate) fn set_current_stop_signal_locked(stopped: bool) {
    REALTIME_CANCELLATION.set_current(stopped);
}

pub(crate) fn publish_if_current(
    session_id: u64,
    session_stopped: &AtomicBool,
    effect: impl FnOnce(),
) -> bool {
    publish_if_session_with(
        &REALTIME_SESSION_PUBLICATION,
        &REALTIME_SESSION_ID,
        session_id,
        Some(session_stopped),
        effect,
    )
}

pub(crate) fn publish_if_engine_current(
    session_id: u64,
    session_stopped: &AtomicBool,
    engine_revision: u64,
    effect: impl FnOnce(),
) -> bool {
    publish_if_engine_current_with(
        &REALTIME_SESSION_PUBLICATION,
        &REALTIME_SESSION_ID,
        &TRANSCRIPTION_ENGINE_REVISION,
        session_id,
        session_stopped,
        engine_revision,
        effect,
    )
}

pub(crate) fn try_publish_if_engine_current(
    session_id: u64,
    session_stopped: &AtomicBool,
    engine_revision: u64,
    effect: impl FnOnce(),
) -> bool {
    try_publish_if_engine_current_with(
        &REALTIME_SESSION_PUBLICATION,
        &REALTIME_SESSION_ID,
        &TRANSCRIPTION_ENGINE_REVISION,
        session_id,
        session_stopped,
        engine_revision,
        effect,
    )
}

fn try_publish_if_engine_current_with(
    publication: &Mutex<()>,
    current_id: &std::sync::atomic::AtomicU64,
    current_revision: &std::sync::atomic::AtomicU64,
    session_id: u64,
    session_stopped: &AtomicBool,
    engine_revision: u64,
    effect: impl FnOnce(),
) -> bool {
    let _publication = match publication.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return false,
    };
    if session_stopped.load(Ordering::SeqCst)
        || current_id.load(Ordering::SeqCst) != session_id
        || current_revision.load(Ordering::SeqCst) != engine_revision
    {
        return false;
    }
    effect();
    true
}

pub(crate) fn publish_if_engine_current_with(
    publication: &Mutex<()>,
    current_id: &std::sync::atomic::AtomicU64,
    current_revision: &std::sync::atomic::AtomicU64,
    session_id: u64,
    session_stopped: &AtomicBool,
    engine_revision: u64,
    effect: impl FnOnce(),
) -> bool {
    let mut published = false;
    publish_if_session_with(
        publication,
        current_id,
        session_id,
        Some(session_stopped),
        || {
            if current_revision.load(Ordering::SeqCst) == engine_revision {
                effect();
                published = true;
            }
        },
    );
    published
}

pub(crate) fn publish_if_session(session_id: u64, effect: impl FnOnce()) -> bool {
    publish_if_session_with(
        &REALTIME_SESSION_PUBLICATION,
        &REALTIME_SESSION_ID,
        session_id,
        None,
        effect,
    )
}

fn publish_if_session_with(
    publication: &Mutex<()>,
    current_id: &std::sync::atomic::AtomicU64,
    session_id: u64,
    session_stopped: Option<&AtomicBool>,
    effect: impl FnOnce(),
) -> bool {
    let _publication = publication
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if session_stopped.is_some_and(|stop| stop.load(Ordering::SeqCst))
        || current_id.load(Ordering::SeqCst) != session_id
    {
        return false;
    }
    effect();
    true
}

#[cfg(test)]
#[path = "publication_tests.rs"]
mod publication_tests;
/// True while a realtime session is winding down and must not be restarted yet.
pub static REALTIME_SESSION_STOPPING: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// Monotonic realtime session generation. Incrementing invalidates stale backend loops.
pub static REALTIME_SESSION_ID: LazyLock<Arc<std::sync::atomic::AtomicU64>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU64::new(1)));
pub static REALTIME_STATE: LazyLock<SharedRealtimeState> =
    LazyLock::new(|| Arc::new(Mutex::new(RealtimeState::new())));
/// Signal to change audio source (true = restart with new source)
pub static AUDIO_SOURCE_CHANGE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// The new audio source to use ("mic" or "device")
pub static NEW_AUDIO_SOURCE: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
/// Signal to change target language
pub static LANGUAGE_CHANGE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// The new target language to use
pub static NEW_TARGET_LANGUAGE: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
/// Signal to change translation model
pub static TRANSLATION_MODEL_CHANGE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// The new translation model to use ("text-llm" or "google-gtx")
pub static NEW_TRANSLATION_MODEL: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
/// Signal to change transcription model
pub static TRANSCRIPTION_MODEL_CHANGE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// The new transcription model id requested by the overlay.
pub static NEW_TRANSCRIPTION_MODEL: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
/// Visibility state for the two cards in the shared compositor.
pub static MIC_VISIBLE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(true)));
pub static TRANS_VISIBLE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(true)));

// --- Per-App Audio Capture State ---
/// Selected app's Process ID for per-app audio capture (0 = not selected / use mic)
pub static SELECTED_APP_PID: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(0)));
/// Selected app's name for display in UI
pub static SELECTED_APP_NAME: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));

// --- Realtime TTS State ---
/// Enable/disable realtime TTS for committed translations
pub static REALTIME_TTS_ENABLED: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// TTS playback speed (100 = 1.0x, 50 = 0.5x, 150 = 1.5x, etc.)
pub static REALTIME_TTS_SPEED: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(100)));
/// Auto-speed mode: automatically adjust speed based on queue length
pub static REALTIME_TTS_AUTO_SPEED: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(true)));
/// Current effective TTS speed (including auto-speed boost) for UI display
pub static CURRENT_TTS_SPEED: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(100)));
/// Signal to close TTS modal (shared between app selection and main window)
pub static CLOSE_TTS_MODAL_REQUEST: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// TTS output volume (0–100, default 100)
pub static CURRENT_TTS_VOLUME: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(100)));

pub static mut REALTIME_HWND: HWND = HWND(std::ptr::null_mut());
/// Written by the realtime relay thread, read by hotkey and GUI threads.
pub static IS_ACTIVE: AtomicBool = AtomicBool::new(false);

// The renderer thread owns exactly one WebView and one WebContext.
thread_local! {
    pub static REALTIME_WEBVIEW: std::cell::RefCell<Option<wry::WebView>> = const { std::cell::RefCell::new(None) };
    // Shared WebContext for this thread using common data directory
    pub static REALTIME_WEB_CONTEXT: std::cell::RefCell<Option<crate::overlay::webview_runtime::ManagedContext>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
mod tests {
    use super::SessionCancellation;
    use std::sync::atomic::Ordering;

    #[test]
    fn a_new_session_cannot_revive_cancelled_workers() {
        let cancellation = SessionCancellation::new();
        let old_session = cancellation.current();
        cancellation.set_current(true);

        let new_session = cancellation.begin_session();

        assert!(old_session.load(Ordering::SeqCst));
        assert!(!new_session.load(Ordering::SeqCst));
    }
}
