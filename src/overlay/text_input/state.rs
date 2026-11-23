// --- TEXT INPUT STATE ---
// Shared state, atomics, and constants for text input overlay.

use crate::win_types::SendHhook;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64};
use std::sync::{Arc, LazyLock, Mutex, Once};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::WM_USER;

// --- REGISTRATION ---
pub static REGISTER_INPUT_CLASS: Once = Once::new();

// --- ATOMIC STATE ---
pub static INPUT_HWND: AtomicIsize = AtomicIsize::new(0);
pub static IS_WARMING_UP: AtomicBool = AtomicBool::new(false);
pub static IS_WARMED_UP: AtomicBool = AtomicBool::new(false);
pub static IS_SHOWING: AtomicBool = AtomicBool::new(false);
pub static PASSIVE_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);

// --- WINDOW MESSAGES ---
pub const WM_APP_SHOW: u32 = WM_USER + 99;
pub const WM_APP_SET_TEXT: u32 = WM_USER + 100;
pub const WM_APP_HIDE: u32 = WM_USER + 101;
pub const WM_APP_SYNC_PASSIVE_EDITOR: u32 = WM_USER + 102;
pub const WM_APP_UPDATE_THEME: u32 = WM_USER + 103;
pub const WM_APP_CLOSE_SESSION: u32 = WM_USER + 104;
pub const WM_APP_REVOKE_TEXT: u32 = WM_USER + 106;
pub const WM_APP_ACTIVATION_ACK: u32 = WM_USER + 107;
pub const WM_APP_PRIOR_QUIET: u32 = WM_USER + 108;
pub const WM_APP_INPUT_LIMIT: u32 = WM_USER + 109;
pub const WM_APP_PASSIVE_KEY: u32 = WM_USER + 110;

pub static DOCUMENT_NONCE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

type TextSubmitCallback = Arc<dyn Fn(String, HWND, u64) + Send + Sync>;

pub struct PendingShow {
    pub title: String,
    pub language: String,
    pub cancel_hotkey: String,
    pub continuous: bool,
    pub passive: bool,
    pub callback: TextSubmitCallback,
}

pub static PENDING_SHOW: LazyLock<Mutex<Option<PendingShow>>> = LazyLock::new(|| Mutex::new(None));
pub static CALLBACK_OWNER: AtomicU64 = AtomicU64::new(0);
pub static EDITOR_READY: AtomicU64 = AtomicU64::new(0);

pub fn dispatch_submit(
    callbacks: &Mutex<Option<TextSubmitCallback>>,
    text: String,
    hwnd: HWND,
    session: u64,
) {
    let callback = callbacks.lock().unwrap().clone();
    if let Some(callback) = callback {
        struct SubmissionGuard;
        impl Drop for SubmissionGuard {
            fn drop(&mut self) {
                SUBMISSION_IN_PROGRESS.with(|active| active.set(false));
            }
        }
        SUBMISSION_IN_PROGRESS.with(|active| active.set(true));
        let _guard = SubmissionGuard;
        callback(text, hwnd, session);
    }
}

// --- LAZY STATICS ---
pub struct SubmittedText {
    pub text: String,
    pub revision: String,
    pub passive: bool,
    pub snapshot: String,
}

pub static SUBMITTED_TEXT: LazyLock<Mutex<Option<SubmittedText>>> =
    LazyLock::new(|| Mutex::new(None));
pub static SHOULD_CLOSE: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));
pub static SHOULD_CLEAR_ONLY: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

// Config Storage (Thread-safe for persistent window)
pub static CFG_TITLE: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
pub static CFG_LANG: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
pub static CFG_CANCEL: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
pub static CFG_CALLBACK: LazyLock<Mutex<Option<TextSubmitCallback>>> =
    LazyLock::new(|| Mutex::new(None));
pub static CFG_CONTINUOUS: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));
pub static INPUT_HOOK: LazyLock<Mutex<SendHhook>> =
    LazyLock::new(|| Mutex::new(SendHhook::default()));

// Cross-thread text injection (for auto-paste from transcription)
const MAX_PENDING_TEXT_ITEMS: usize = 64;
pub(super) const MAX_PENDING_TEXT_BYTES: usize = 1024 * 1024;
static NEXT_EDITOR_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
struct EditorSessionState {
    generation: u64,
    pending: VecDeque<PendingText>,
    pending_bytes: usize,
    active_operation: Option<u64>,
    next_sequence: u64,
}

#[derive(Default)]
pub struct EditorSessions {
    state: Mutex<EditorSessionState>,
    cancel_global: bool,
}

impl EditorSessions {
    pub fn begin(&self) {
        let mut state = self.state.lock().unwrap();
        let previous = (state.generation & 1 != 0).then_some(state.generation);
        state.generation = NEXT_EDITOR_SESSION
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |value| value.checked_add(2),
            )
            .expect("editor session identifiers exhausted");
        let abandoned = std::mem::take(&mut state.pending);
        state.pending_bytes = 0;
        state.active_operation = None;
        state.next_sequence = 0;
        drop(state);
        for pending in abandoned {
            if let Some(operation) = pending.operation {
                operation.cancel();
            }
        }
        if self.cancel_global
            && let Some(previous) = previous
        {
            super::delivery::cancel_session(previous);
        }
    }

    pub fn end(&self) {
        let mut state = self.state.lock().unwrap();
        let previous = (state.generation & 1 != 0).then_some(state.generation);
        if state.generation & 1 != 0 {
            state.generation = state.generation.wrapping_add(1);
        }
        let abandoned = std::mem::take(&mut state.pending);
        state.pending_bytes = 0;
        state.active_operation = None;
        drop(state);
        for pending in abandoned {
            if let Some(operation) = pending.operation {
                operation.cancel();
            }
        }
        if self.cancel_global
            && let Some(previous) = previous
        {
            super::delivery::cancel_session(previous);
        }
    }

    pub fn current(&self) -> Option<u64> {
        let value = self.state.lock().unwrap().generation;
        (value & 1 == 1).then_some(value)
    }

    pub fn try_current(&self) -> anyhow::Result<Option<u64>> {
        let value = self
            .state
            .try_lock()
            .map_err(|_| anyhow::anyhow!("The editor destination is unavailable"))?
            .generation;
        Ok((value & 1 == 1).then_some(value))
    }

    pub fn is_current(&self, id: u64) -> bool {
        self.current() == Some(id)
    }

    pub fn enqueue(&self, pending: PendingText) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.generation != pending.session
            || pending.session & 1 == 0
            || pending
                .owner
                .as_ref()
                .is_some_and(|owner| owner.session != pending.session)
            || state.pending.len() >= MAX_PENDING_TEXT_ITEMS
            || pending.payload_bytes() > MAX_PENDING_TEXT_BYTES.saturating_sub(state.pending_bytes)
        {
            return false;
        }
        let Some(sequence) = state.next_sequence.checked_add(1) else {
            return false;
        };
        if let Some(operation) = &pending.operation
            && (operation.session() != pending.session || !operation.set_sequence(sequence))
        {
            return false;
        }
        state.next_sequence = sequence;
        state.pending_bytes += pending.payload_bytes();
        state.pending.push_back(pending);
        true
    }

    pub fn take_pending(&self) -> Option<PendingText> {
        let mut state = self.state.lock().unwrap();
        if state.active_operation.is_some() {
            return None;
        }
        let pending = state.pending.pop_front()?;
        state.pending_bytes -= pending.payload_bytes();
        state.active_operation = pending.operation.as_ref().map(|operation| operation.id());
        Some(pending)
    }

    pub fn finish_operation(&self, id: u64) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.active_operation == Some(id) {
            state.active_operation = None;
            !state.pending.is_empty()
        } else {
            false
        }
    }

    pub fn remove_pending(&self, operation_id: u64) {
        let mut state = self.state.lock().unwrap();
        if let Some(index) = state.pending.iter().position(|pending| {
            pending
                .operation
                .as_ref()
                .is_some_and(|operation| operation.id() == operation_id)
        }) && let Some(pending) = state.pending.remove(index)
        {
            state.pending_bytes -= pending.payload_bytes();
        }
    }
}

pub static EDITOR_SESSIONS: LazyLock<EditorSessions> = LazyLock::new(|| EditorSessions {
    state: Mutex::new(EditorSessionState::default()),
    cancel_global: true,
});

#[derive(Clone)]
pub struct EditorOwner {
    pub token: Arc<crate::overlay::result::ChainCancelToken>,
    pub session: u64,
}

pub struct PendingText {
    pub text: String,
    pub action: EditorAction,
    pub session: u64,
    pub owner: Option<EditorOwner>,
    pub operation: Option<Arc<super::delivery::Operation>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorAction {
    Insert,
    Replace,
    Focus {
        retry: bool,
    },
    Clear {
        revision: String,
        passive: bool,
        snapshot: String,
    },
}

impl PendingText {
    fn payload_bytes(&self) -> usize {
        match &self.action {
            EditorAction::Clear {
                revision, snapshot, ..
            } => self
                .text
                .len()
                .saturating_add(revision.len())
                .saturating_add(snapshot.len()),
            _ => self.text.len(),
        }
    }

    pub fn dispatch(
        self,
        sessions: &EditorSessions,
        effect: impl FnOnce(
            String,
            EditorAction,
            Option<EditorOwner>,
            Option<Arc<super::delivery::Operation>>,
        ),
    ) {
        if !sessions.is_current(self.session) {
            if let Some(operation) = self.operation {
                operation.cancel();
                if sessions.finish_operation(operation.id()) {
                    super::wake_editor_queue();
                }
            }
            return;
        }
        if self
            .owner
            .as_ref()
            .is_some_and(|owner| owner.token.is_cancelled())
        {
            if let Some(operation) = self.operation {
                operation.cancel();
                if sessions.finish_operation(operation.id()) {
                    super::wake_editor_queue();
                }
            }
            return;
        }
        effect(self.text, self.action, self.owner, self.operation);
    }
}

// --- THREAD LOCAL ---
thread_local! {
    pub static HIDE_PENDING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub static SUBMISSION_IN_PROGRESS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub static DELAYED_FOCUS: RefCell<Option<super::focus::DelayedFocus>> = const { RefCell::new(None) };
    pub static TEXT_INPUT_WEBVIEW: RefCell<Option<wry::WebView>> = const { RefCell::new(None) };
    pub static TEXT_INPUT_WEB_CONTEXT: RefCell<Option<crate::overlay::webview_runtime::ManagedContext>> = const { RefCell::new(None) };
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
