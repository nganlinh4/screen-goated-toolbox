// --- RECORDING STATE ---
// Global signals, atomics, and shared state for recording overlay.

use std::sync::{
    Arc, LazyLock, Mutex, Once, Weak,
    atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicU64, Ordering},
};
use windows::Win32::UI::WindowsAndMessaging::WM_USER;

// --- GLOBAL SIGNALS ---
pub static AUDIO_STOP_SIGNAL: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
pub static AUDIO_PAUSE_SIGNAL: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
pub static AUDIO_ABORT_SIGNAL: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
#[derive(Default)]
struct RecordingCancellation {
    state: Mutex<RecordingOwnerState>,
}

#[derive(Default)]
struct RecordingOwnerState {
    next_id: usize,
    active: Option<(usize, Weak<crate::overlay::result::ChainCancelToken>)>,
    stop: Weak<AtomicBool>,
}

impl RecordingCancellation {
    #[cfg(test)]
    fn register(
        &self,
        token: &Arc<crate::overlay::result::ChainCancelToken>,
        aborted: &AtomicBool,
        stop: &Arc<AtomicBool>,
    ) -> usize {
        self.register_with_reset(token, aborted, stop, || {})
    }

    fn register_with_reset(
        &self,
        token: &Arc<crate::overlay::result::ChainCancelToken>,
        aborted: &AtomicBool,
        stop: &Arc<AtomicBool>,
        reset: impl FnOnce(),
    ) -> usize {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // A new capture replaces an active capture. A stopped capture is already
        // processing its result and must be allowed to finish independently.
        if state
            .stop
            .upgrade()
            .is_some_and(|previous| !previous.load(Ordering::SeqCst))
            && let Some(previous) = state.active.as_ref().and_then(|(_, token)| token.upgrade())
        {
            previous.cancel();
        }
        reset();
        if aborted.load(Ordering::SeqCst) {
            token.cancel();
        }
        state.next_id = state
            .next_id
            .checked_add(1)
            .expect("recording session id exhausted");
        state.active = Some((state.next_id, Arc::downgrade(token)));
        state.stop = Arc::downgrade(stop);
        state.next_id
    }

    fn cancel(&self, aborted: &AtomicBool) -> Option<usize> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        aborted.store(true, Ordering::SeqCst);
        if let Some(token) = state.active.as_ref().and_then(|(_, token)| token.upgrade()) {
            token.cancel();
        }
        state.active.as_ref().map(|(id, _)| *id)
    }

    fn cancel_id(&self, id: usize, aborted: &AtomicBool, stop: &AtomicBool) -> bool {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state
            .active
            .as_ref()
            .is_some_and(|(current, _)| *current == id)
        {
            return false;
        }
        aborted.store(true, Ordering::SeqCst);
        stop.store(true, Ordering::SeqCst);
        if let Some(session_stop) = state.stop.upgrade() {
            session_stop.store(true, Ordering::SeqCst);
        }
        if let Some(token) = state.active.as_ref().and_then(|(_, token)| token.upgrade()) {
            token.cancel();
        }
        true
    }

    fn toggle_pause_id(&self, id: usize, stop: &AtomicBool, pause: &AtomicBool) -> bool {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state
            .active
            .as_ref()
            .is_some_and(|(current, _)| *current == id)
            || stop.load(Ordering::SeqCst)
            || state
                .stop
                .upgrade()
                .is_some_and(|signal| signal.load(Ordering::SeqCst))
        {
            return false;
        }
        pause.fetch_xor(true, Ordering::SeqCst);
        true
    }

    fn stop_if_current(
        &self,
        token: &Arc<crate::overlay::result::ChainCancelToken>,
        stop: &AtomicBool,
    ) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state
            .active
            .as_ref()
            .and_then(|(_, token)| token.upgrade())
            .is_some_and(|current| Arc::ptr_eq(&current, token) && !token.is_cancelled())
        {
            stop.store(true, Ordering::SeqCst);
            if let Some(session_stop) = state.stop.upgrade() {
                session_stop.store(true, Ordering::SeqCst);
            }
        }
    }

    fn stop_current(&self, stop: &AtomicBool) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        stop.store(true, Ordering::SeqCst);
        if let Some(session_stop) = state.stop.upgrade() {
            session_stop.store(true, Ordering::SeqCst);
        }
    }

    fn is_stopped(&self, stop: &AtomicBool) -> bool {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        stop.load(Ordering::SeqCst)
            || state
                .stop
                .upgrade()
                .is_some_and(|signal| signal.load(Ordering::SeqCst))
    }

    fn id_for(&self, token: &Arc<crate::overlay::result::ChainCancelToken>) -> Option<usize> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active.as_ref().and_then(|(id, current)| {
            current
                .upgrade()
                .filter(|current| Arc::ptr_eq(current, token))
                .map(|_| *id)
        })
    }

    fn is_current_id(&self, id: usize) -> bool {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state
            .active
            .as_ref()
            .is_some_and(|(current, _)| *current == id)
    }

    fn with_current(
        &self,
        token: &Arc<crate::overlay::result::ChainCancelToken>,
        action: impl FnOnce(),
    ) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state
            .active
            .as_ref()
            .and_then(|(_, current)| current.upgrade())
            .is_some_and(|current| Arc::ptr_eq(&current, token) && !token.is_cancelled())
        {
            action();
        }
    }
}

static ACTIVE_RECORDING_CHAIN: LazyLock<RecordingCancellation> =
    LazyLock::new(RecordingCancellation::default);

pub(super) fn register_recording_chain(
    token: &Arc<crate::overlay::result::ChainCancelToken>,
    stop: &Arc<AtomicBool>,
) -> usize {
    ACTIVE_RECORDING_CHAIN.register_with_reset(token, &AUDIO_ABORT_SIGNAL, stop, || {
        AUDIO_STOP_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_PAUSE_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_ABORT_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_WARMUP_COMPLETE.store(false, Ordering::SeqCst);
        AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
        CURRENT_RMS.store(0, Ordering::Relaxed);
    })
}
pub(super) fn cancel_recording_chain() -> Option<usize> {
    ACTIVE_RECORDING_CHAIN.cancel(&AUDIO_ABORT_SIGNAL)
}
pub(super) fn cancel_recording_id(id: usize) -> bool {
    ACTIVE_RECORDING_CHAIN.cancel_id(id, &AUDIO_ABORT_SIGNAL, &AUDIO_STOP_SIGNAL)
}
pub(super) fn toggle_recording_pause_id(id: usize) -> bool {
    ACTIVE_RECORDING_CHAIN.toggle_pause_id(id, &AUDIO_STOP_SIGNAL, &AUDIO_PAUSE_SIGNAL)
}
pub(crate) fn stop_if_current_recording(token: &Arc<crate::overlay::result::ChainCancelToken>) {
    ACTIVE_RECORDING_CHAIN.stop_if_current(token, &AUDIO_STOP_SIGNAL);
}
pub(super) fn stop_current_recording() {
    ACTIVE_RECORDING_CHAIN.stop_current(&AUDIO_STOP_SIGNAL);
}
pub(super) fn is_current_recording_stopped() -> bool {
    ACTIVE_RECORDING_CHAIN.is_stopped(&AUDIO_STOP_SIGNAL)
}
pub(crate) fn close_if_current_recording(
    hwnd: windows::Win32::Foundation::HWND,
    token: &Arc<crate::overlay::result::ChainCancelToken>,
) {
    if let Some(id) = ACTIVE_RECORDING_CHAIN.id_for(token) {
        close_recording_session(hwnd, id);
    }
}
pub(crate) fn recording_session_id(
    token: &Arc<crate::overlay::result::ChainCancelToken>,
) -> Option<usize> {
    ACTIVE_RECORDING_CHAIN.id_for(token)
}
pub(crate) fn close_recording_session(hwnd: windows::Win32::Foundation::HWND, id: usize) {
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(hwnd),
            WM_APP_SESSION_CLOSE,
            windows::Win32::Foundation::WPARAM(id),
            windows::Win32::Foundation::LPARAM(0),
        );
    }
}
pub(super) fn is_current_recording_id(id: usize) -> bool {
    ACTIVE_RECORDING_CHAIN.is_current_id(id)
}
pub(crate) fn update_viz_if_current_recording(
    token: &Arc<crate::overlay::result::ChainCancelToken>,
    rms: f32,
) {
    ACTIVE_RECORDING_CHAIN.with_current(token, || {
        update_audio_viz(rms);
        if rms > 0.001 {
            AUDIO_WARMUP_COMPLETE.store(true, Ordering::SeqCst);
        }
    });
}
pub(crate) fn notify_if_current_recording(
    token: &Arc<crate::overlay::result::ChainCancelToken>,
    message: &str,
) {
    ACTIVE_RECORDING_CHAIN.with_current(token, || {
        crate::overlay::auto_copy_badge::show_error_notification(message);
    });
}
pub(crate) fn set_initializing_if_current_recording(
    token: &Arc<crate::overlay::result::ChainCancelToken>,
    initializing: bool,
) {
    ACTIVE_RECORDING_CHAIN.with_current(token, || {
        AUDIO_INITIALIZING.store(initializing, Ordering::SeqCst);
    });
}
pub(crate) fn mark_ready_if_current_recording(
    token: &Arc<crate::overlay::result::ChainCancelToken>,
) {
    ACTIVE_RECORDING_CHAIN.with_current(token, || {
        AUDIO_INITIALIZING.store(false, Ordering::SeqCst);
        AUDIO_WARMUP_COMPLETE.store(true, Ordering::SeqCst);
    });
}
pub static AUDIO_WARMUP_COMPLETE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// Signal for Gemini Live initialization phase (WebSocket setup)
pub static AUDIO_INITIALIZING: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));

// --- ATOMIC STATE ---
pub static LAST_SHOW_TIME: AtomicU64 = AtomicU64::new(0);
pub static CURRENT_RMS: AtomicU32 = AtomicU32::new(0);

// 0=Not Created, 1=Hidden/Warmup, 2=Visible/Recording
pub static RECORDING_STATE: AtomicI32 = AtomicI32::new(0);
pub static RECORDING_HWND_VAL: AtomicIsize = AtomicIsize::new(0);
pub static REGISTER_RECORDING_CLASS: Once = Once::new();
pub static LAST_THEME_IS_DARK: AtomicBool = AtomicBool::new(true);
pub static CURRENT_RECORDING_HIDDEN: AtomicBool = AtomicBool::new(false);

// --- WINDOW MESSAGES ---
pub const WM_APP_SHOW: u32 = WM_USER + 20;
pub const WM_APP_HIDE: u32 = WM_USER + 21;
pub const WM_APP_REAL_SHOW: u32 = WM_USER + 22;
pub const WM_APP_UPDATE_STATE: u32 = WM_USER + 23;
pub const WM_APP_SESSION_CLOSE: u32 = WM_USER + 24;
pub const WM_USER_FULL_CLOSE: u32 = WM_USER + 99;

// --- HELPER FUNCTIONS ---

pub fn update_audio_viz(rms: f32) {
    let bits = rms.to_bits();
    CURRENT_RMS.store(bits, Ordering::Relaxed);
}

/// Get adaptive UI dimensions based on screen aspect ratio
pub fn get_ui_dimensions() -> (i32, i32) {
    let area = crate::overlay::display_placement::primary_work_area();
    let screen_w = area.width;
    let screen_h = area.height;

    // Width scales inversely with aspect ratio for consistent UI appearance
    // At 16:9 (1.78:1): 450px width
    // At 21:9 (2.37:1): 375px width (narrower on ultrawide)
    let aspect_ratio = screen_w as f64 / screen_h as f64;
    let base_aspect = 16.0 / 9.0; // 1.778
    let scale = crate::overlay::display_placement::primary_scale();
    let width =
        ((450.0 - (aspect_ratio - base_aspect) * 127.0).clamp(350.0, 500.0) * scale).round() as i32;
    let width = width.min(screen_w);

    // Height stays constant at 70px
    let height = ((70.0 * scale).round() as i32).min(screen_h);

    (width, height)
}

#[cfg(test)]
mod recording_cancellation_tests {
    use super::*;
    use crate::overlay::result::ChainCancelToken;

    #[test]
    fn stale_controls_cannot_cancel_or_pause_a_replacement() {
        let owner = RecordingCancellation::default();
        let aborted = AtomicBool::new(false);
        let stopped = AtomicBool::new(false);
        let paused = AtomicBool::new(false);
        let old = ChainCancelToken::new();
        let old_stop = Arc::new(AtomicBool::new(false));
        let old_id = owner.register(&old, &aborted, &old_stop);
        let next = ChainCancelToken::new();
        let next_stop = Arc::new(AtomicBool::new(false));
        let next_id = owner.register(&next, &aborted, &next_stop);

        assert!(!owner.toggle_pause_id(old_id, &stopped, &paused));
        assert!(!owner.cancel_id(old_id, &aborted, &stopped));
        assert!(!paused.load(Ordering::SeqCst));
        assert!(!aborted.load(Ordering::SeqCst));
        assert!(!next.is_cancelled());
        assert!(owner.toggle_pause_id(next_id, &stopped, &paused));
        assert!(paused.load(Ordering::SeqCst));
        assert!(owner.cancel_id(next_id, &aborted, &stopped));
        assert!(next.is_cancelled());
        assert!(next_stop.load(Ordering::SeqCst));
    }

    #[test]
    fn old_cancel_cannot_interleave_with_new_owner_signal_reset() {
        use std::sync::mpsc;
        use std::time::Duration;

        let owner = Arc::new(RecordingCancellation::default());
        let aborted = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let old = ChainCancelToken::new();
        let old_stop = Arc::new(AtomicBool::new(false));
        let old_id = owner.register(&old, &aborted, &old_stop);
        let next = ChainCancelToken::new();
        let next_stop = Arc::new(AtomicBool::new(false));
        let (entered, entered_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let next_owner = owner.clone();
        let next_aborted = aborted.clone();
        let next_stopped = stopped.clone();
        let next_token = next.clone();
        let registration = std::thread::spawn(move || {
            next_owner.register_with_reset(&next_token, &next_aborted, &next_stop, || {
                next_aborted.store(false, Ordering::SeqCst);
                next_stopped.store(false, Ordering::SeqCst);
                entered.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let old_owner = owner.clone();
        let old_aborted = aborted.clone();
        let old_stopped = stopped.clone();
        let (attempt, attempt_rx) = mpsc::channel();
        let cancellation = std::thread::spawn(move || {
            attempt.send(()).unwrap();
            old_owner.cancel_id(old_id, &old_aborted, &old_stopped)
        });
        attempt_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        release.send(()).unwrap();
        let next_id = registration.join().unwrap();
        assert_ne!(old_id, next_id);
        assert!(!cancellation.join().unwrap());
        assert!(!aborted.load(Ordering::SeqCst));
        assert!(!stopped.load(Ordering::SeqCst));
        assert!(!next.is_cancelled());
    }

    #[test]
    fn replacing_an_active_capture_retires_only_the_old_session() {
        let owner = RecordingCancellation::default();
        let aborted = AtomicBool::new(false);
        let old = ChainCancelToken::new();
        let old_stop = Arc::new(AtomicBool::new(false));
        let old_abort = Arc::new(AtomicBool::new(false));
        old.register_cancel_flag(&old_stop);
        old.register_cancel_flag(&old_abort);
        owner.register(&old, &aborted, &old_stop);

        let next = ChainCancelToken::new();
        let next_stop = Arc::new(AtomicBool::new(false));
        let next_abort = Arc::new(AtomicBool::new(false));
        next.register_cancel_flag(&next_stop);
        next.register_cancel_flag(&next_abort);
        owner.register(&next, &aborted, &next_stop);

        assert!(old.is_cancelled());
        assert!(old_stop.load(Ordering::SeqCst));
        assert!(old_abort.load(Ordering::SeqCst));
        assert!(!next.is_cancelled());
        assert!(!next_stop.load(Ordering::SeqCst));
        assert!(!next_abort.load(Ordering::SeqCst));
    }

    #[test]
    fn new_capture_does_not_cancel_a_stopped_sessions_processing() {
        let owner = RecordingCancellation::default();
        let aborted = AtomicBool::new(false);
        let old = ChainCancelToken::new();
        let old_stop = Arc::new(AtomicBool::new(false));
        old.register_cancel_flag(&old_stop);
        owner.register(&old, &aborted, &old_stop);
        owner.stop_current(&AtomicBool::new(false));

        let next = ChainCancelToken::new();
        let next_stop = Arc::new(AtomicBool::new(false));
        next.register_cancel_flag(&next_stop);
        owner.register(&next, &aborted, &next_stop);

        assert!(old_stop.load(Ordering::SeqCst));
        assert!(!old.is_cancelled());
        assert!(!next.is_cancelled());
    }

    #[test]
    fn cancelled_recording_stays_cancelled_after_a_new_session_starts() {
        let owner = RecordingCancellation::default();
        let aborted = AtomicBool::new(false);
        let old = ChainCancelToken::new();
        let branch = ChainCancelToken::child(&old);
        let worker_cancelled = Arc::new(AtomicBool::new(false));
        branch.register_cancel_flag(&worker_cancelled);
        let old_stop = Arc::new(AtomicBool::new(false));
        old.register_cancel_flag(&old_stop);
        owner.register(&old, &aborted, &old_stop);
        let queued_old_close = owner.id_for(&old).unwrap();
        assert!(owner.is_current_id(queued_old_close));
        let _ = owner.cancel(&aborted);
        assert!(worker_cancelled.load(Ordering::SeqCst));
        assert!(old_stop.load(Ordering::SeqCst));
        assert!(branch.is_cancelled());

        aborted.store(false, Ordering::SeqCst);
        let next = ChainCancelToken::new();
        let next_stop = Arc::new(AtomicBool::new(false));
        next.register_cancel_flag(&next_stop);
        owner.register(&next, &aborted, &next_stop);
        let next_id = owner.id_for(&next).unwrap();
        assert_ne!(queued_old_close, next_id);
        assert!(!owner.is_current_id(queued_old_close));
        assert!(owner.is_current_id(next_id));
        let observed = AtomicBool::new(false);
        owner.with_current(&old, || observed.store(true, Ordering::SeqCst));
        assert!(!observed.load(Ordering::SeqCst));
        owner.with_current(&next, || observed.store(true, Ordering::SeqCst));
        assert!(observed.load(Ordering::SeqCst));
        let stop = AtomicBool::new(false);
        owner.stop_if_current(&old, &stop);
        assert!(!stop.load(Ordering::SeqCst));
        assert!(!next_stop.load(Ordering::SeqCst));
        owner.stop_if_current(&next, &stop);
        assert!(stop.load(Ordering::SeqCst));
        assert!(next_stop.load(Ordering::SeqCst));
        assert!(!next.is_cancelled());
        assert!(branch.is_cancelled());
        let _ = owner.cancel(&aborted);
        assert!(next.is_cancelled());
    }
}
