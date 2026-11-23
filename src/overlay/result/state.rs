use std::collections::HashMap;
use std::sync::{
    Arc, LazyLock, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};
use windows::Win32::Foundation::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultPresentation {
    #[default]
    Standard,
    TextOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultControlOptions {
    pub anchor_rect: Option<[i32; 4]>,
    pub control_color: Option<String>,
    pub scale_percent: u16,
    pub group_actions: bool,
    pub edit_enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SourceReplacementRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub vertical: bool,
    #[serde(default)]
    pub wrap: bool,
    /// Allowed source rectangles, relative to this region; empty means rectangular.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footprint: Vec<[u32; 4]>,
}

impl Default for ResultControlOptions {
    fn default() -> Self {
        Self {
            anchor_rect: None,
            control_color: None,
            scale_percent: 100,
            group_actions: false,
            edit_enabled: true,
        }
    }
}

impl ResultControlOptions {
    pub(crate) fn shift_anchor(&mut self, dx: i32, dy: i32) -> bool {
        let Some(anchor) = self.anchor_rect.as_mut() else {
            return false;
        };
        anchor[0] = anchor[0].saturating_add(dx);
        anchor[1] = anchor[1].saturating_add(dy);
        true
    }
}

// --- HIERARCHICAL CANCEL TOKEN ---

/// Tree-structured cancellation token for chain processing.
/// Each node links to its parent. `is_cancelled()` walks up the tree:
/// if ANY ancestor is cancelled, the check returns true.
///
/// Close window B → signals B's token → B's downstream sees parent cancelled → stops.
/// Sibling branch C has a DIFFERENT token → unaffected.
pub struct ChainCancelToken {
    cancelled: AtomicBool,
    parent: Option<Arc<ChainCancelToken>>,
    listeners: Mutex<Vec<Weak<AtomicBool>>>,
    callbacks: Mutex<Vec<Weak<dyn Fn() + Send + Sync>>>,
    admission: Mutex<()>,
}

impl ChainCancelToken {
    /// Create a root token (no parent).
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            parent: None,
            listeners: Mutex::new(Vec::new()),
            callbacks: Mutex::new(Vec::new()),
            admission: Mutex::new(()),
        })
    }

    /// Create a child token linked to a parent.
    pub fn child(parent: &Arc<Self>) -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            parent: Some(parent.clone()),
            listeners: Mutex::new(Vec::new()),
            callbacks: Mutex::new(Vec::new()),
            admission: Mutex::new(()),
        })
    }

    /// Signal cancellation for this node.
    pub fn cancel(&self) {
        let admission = self
            .admission
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.cancelled.store(true, Ordering::SeqCst);
        let mut listeners = self
            .listeners
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for listener in listeners.drain(..) {
            if let Some(flag) = listener.upgrade() {
                flag.store(true, Ordering::SeqCst);
            }
        }
        drop(listeners);
        let callbacks = self
            .callbacks
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .drain(..)
            .filter_map(|callback| callback.upgrade())
            .collect::<Vec<_>>();
        drop(admission);
        for callback in callbacks {
            callback();
        }
    }

    /// Admit an effect only while this token and its ancestors are active.
    pub(crate) fn run_if_active<T>(&self, effect: impl FnOnce() -> T) -> Option<T> {
        let mut ancestors = Vec::new();
        let mut current = Some(self);
        while let Some(token) = current {
            ancestors.push(token);
            current = token.parent.as_deref();
        }
        let mut guards = Vec::with_capacity(ancestors.len());
        for token in ancestors.into_iter().rev() {
            let guard = token
                .admission
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if token.cancelled.load(Ordering::SeqCst) {
                return None;
            }
            guards.push(guard);
        }
        Some(effect())
    }

    /// Propagate this token and every ancestor to an existing API flag.
    pub(crate) fn register_cancel_flag(&self, flag: &Arc<AtomicBool>) {
        {
            let mut listeners = self
                .listeners
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if self.cancelled.load(Ordering::SeqCst) {
                flag.store(true, Ordering::SeqCst);
            } else {
                listeners.retain(|listener| listener.strong_count() != 0);
                listeners.push(Arc::downgrade(flag));
            }
        }
        if let Some(parent) = &self.parent {
            parent.register_cancel_flag(flag);
        }
    }

    /// Register a nonblocking notification for this token and its ancestors.
    /// Notifications are invoked only after the token's locks are released.
    pub(crate) fn register_cancel_callback(&self, callback: &Arc<dyn Fn() + Send + Sync>) {
        let already_cancelled = {
            let mut callbacks = self
                .callbacks
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if self.cancelled.load(Ordering::SeqCst) {
                true
            } else {
                callbacks.retain(|callback| callback.strong_count() != 0);
                callbacks.push(Arc::downgrade(callback));
                false
            }
        };
        if already_cancelled {
            callback();
        }
        if let Some(parent) = &self.parent {
            parent.register_cancel_callback(callback);
        }
    }

    /// Check if this node or any ancestor is cancelled.
    pub fn is_cancelled(&self) -> bool {
        if self.cancelled.load(Ordering::Relaxed) {
            return true;
        }
        if let Some(ref parent) = self.parent {
            return parent.is_cancelled();
        }
        false
    }
}

// Context for Refinement
#[derive(Clone)]
pub enum RefineContext {
    None,
    Image(Vec<u8>),   // PNG Bytes
    Audio(Arc<[u8]>), // WAV bytes
}

pub struct WindowState {
    pub presentation: ResultPresentation,
    pub control_options: Option<ResultControlOptions>,
    pub backdrop_data_url: Option<String>,
    pub foreground_color: Option<String>,
    pub preferred_font_size: Option<f32>,
    pub source_vertical: bool,
    pub source_regions: Vec<SourceReplacementRegion>,
    pub source_segments: Vec<String>,
    pub copy_success: bool,

    // Refinement editor state. The draft and lifecycle are canonical in Rust.
    pub(crate) refine_session: super::refine_session::RefineSession,
    pub context_data: RefineContext, // Data needed for API call
    pub full_text: String,           // Current full text content

    // Text History for Undo/Redo
    pub text_history: Vec<String>, // Stack of previous text states (for Undo)
    pub redo_history: Vec<String>, // Stack of undone text states (for Redo)

    // Refinement State
    pub is_refining: bool,

    // Streaming state - true when actively receiving chunks (buttons hidden during streaming)
    pub is_streaming_active: bool,

    // Metadata for Refinement/Processing
    pub model_id: String,
    pub model_name: String,
    pub provider: String,
    pub streaming_enabled: bool,

    // NEW: Preset Prompt for "Type" mode logic
    pub preset_prompt: String,
    // NEW: Input text currently being refined/processed
    pub input_text: String,

    pub bg_color: u32,
    pub linked_windows: Vec<HWND>,

    // Cancellation token — hierarchical; cancel propagates to descendants
    pub cancellation_token: Option<Arc<ChainCancelToken>>,
    // Active refinement is distinct from the chain and invalidated on replacement.
    pub refine_cancel_token: Option<Arc<AtomicBool>>,
    // Chain ID — shared by all windows in the same chain execution
    pub chain_id: Option<String>,
    // Correlates provider and renderer milestones for this chain block.
    pub latency_trace_id: Option<String>,

    // Web Browsing State
    pub is_browsing: bool, // True when user has navigated away from initial content
    pub is_navigation_loading: bool, // Native page stays hidden until its top-level load finishes
    pub navigation_depth: usize, // How many pages deep from initial content (0 = at result)
    pub max_navigation_depth: usize, // Max depth reached (to know if forward is possible)
    // Speaker/TTS state
    pub tts_request_id: u64,       // Active TTS request ID (0 = not speaking)
    pub tts_loading: bool,         // True when TTS is loading/connecting (shows spinner)
    pub opacity_percent: u8,       // Transparency level (0-100)
    pub preset_id: Option<String>, // ID of the preset that spawned this window
    pub is_chain_root: bool,       // True if this is the first window in a chain
    pub onboarding_pulse_claimed: bool,
    pub onboarding_pulse_token: u8,
}

// SAFETY: the only non-Send fields are the HWND values in `linked_windows`.
// An HWND is a process-wide handle value, not thread-owned memory, and the
// state is reached only through the WINDOW_STATES mutex. Sync is not needed:
// Mutex<WindowState> is Sync once WindowState is Send.
unsafe impl Send for WindowState {}

pub static WINDOW_STATES: LazyLock<Mutex<HashMap<isize, WindowState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub enum WindowType {
    Primary,
    // Note: Secondary and SecondaryExplicit were removed as dead code
}

pub fn link_windows(hwnd1: HWND, hwnd2: HWND) {
    link_window_states(hwnd1, hwnd2);
    super::scene_compositor::sync_controls(hwnd1);
    super::scene_compositor::sync_controls(hwnd2);
}

fn link_window_states(hwnd1: HWND, hwnd2: HWND) {
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(s1) = states.get_mut(&(hwnd1.0 as isize))
            && !s1.linked_windows.contains(&hwnd2)
        {
            s1.linked_windows.push(hwnd2);
        }
        if let Some(s2) = states.get_mut(&(hwnd2.0 as isize))
            && !s2.linked_windows.contains(&hwnd1)
        {
            s2.linked_windows.push(hwnd1);
        }
    }
}

use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW, WM_CLOSE};

/// Close all windows belonging to a chain (by chain_id).
/// Signals each window's cancellation token and posts WM_CLOSE.
/// Used in continuous input mode to destroy previous result overlays before spawning new ones.
pub fn close_chain_windows(chain_id: &str) {
    let mut to_close = Vec::new();
    let mut tokens = Vec::new();
    {
        let states = WINDOW_STATES.lock().unwrap();
        for (&h_val, state) in states.iter() {
            if state.chain_id.as_deref() == Some(chain_id) {
                if let Some(ref token) = state.cancellation_token {
                    tokens.push(token.clone());
                }
                to_close.push(HWND(h_val as *mut std::ffi::c_void));
            }
        }
    }

    for token in tokens {
        token.cancel();
    }

    for hwnd in to_close {
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() {
                // HIDE IMMEDIATELY so collision detection (which uses IsWindowVisible)
                // will ignore these windows even if they take a moment to be destroyed.
                let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
                );
                super::scene_compositor::remove_window(hwnd);

                let _ = PostMessageW(
                    Some(hwnd),
                    WM_CLOSE,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }
    }
}

/// Get a group of windows linked via the stored window adjacency graph.
/// Used for right-click group close/drag — follows the linked chain.
pub fn get_window_group(hwnd: HWND) -> Vec<(HWND, RECT)> {
    let mut group = Vec::new();
    let states = WINDOW_STATES.lock().unwrap();

    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    queue.push_back(hwnd);
    visited.insert(hwnd.0);

    while let Some(current) = queue.pop_front() {
        let mut r = RECT::default();
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(current, &mut r);
        }
        group.push((current, r));

        if let Some(s) = states.get(&(current.0 as isize)) {
            for linked in &s.linked_windows {
                if states.contains_key(&(linked.0 as isize)) && !visited.contains(&linked.0) {
                    visited.insert(linked.0);
                    queue.push_back(*linked);
                }
            }
        }
    }

    group
}

#[cfg(test)]
mod tests {
    use super::{ChainCancelToken, ResultControlOptions};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn anchored_controls_follow_committed_result_movement() {
        let mut options = ResultControlOptions {
            anchor_rect: Some([10, 20, 300, 180]),
            ..Default::default()
        };

        assert!(options.shift_anchor(7, -4));
        assert_eq!(options.anchor_rect, Some([17, 16, 300, 180]));
    }

    #[test]
    fn cancellation_flags_follow_ancestors_but_not_siblings() {
        let root = ChainCancelToken::new();
        let left = ChainCancelToken::child(&root);
        let right = ChainCancelToken::child(&root);
        let child = ChainCancelToken::child(&left);
        let left_flag = Arc::new(AtomicBool::new(false));
        let right_flag = Arc::new(AtomicBool::new(false));
        child.register_cancel_flag(&left_flag);
        right.register_cancel_flag(&right_flag);
        left.cancel();
        assert!(left_flag.load(Ordering::SeqCst));
        assert!(!right_flag.load(Ordering::SeqCst));
        let late_flag = Arc::new(AtomicBool::new(false));
        child.register_cancel_flag(&late_flag);
        assert!(late_flag.load(Ordering::SeqCst));
        root.cancel();
        assert!(right_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn side_effect_admission_tracks_branch_and_parent_cancellation() {
        let root = ChainCancelToken::new();
        let left = ChainCancelToken::child(&root);
        let right = ChainCancelToken::child(&root);
        left.cancel();
        assert_eq!(left.run_if_active(|| 1), None);
        assert_eq!(right.run_if_active(|| 2), Some(2));
        root.cancel();
        assert_eq!(right.run_if_active(|| 3), None);
    }

    #[test]
    fn cancellation_waits_for_admitted_effect_and_rejects_later_effects() {
        let token = ChainCancelToken::new();
        let worker_token = token.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            worker_token.run_if_active(|| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
        });
        started_rx.recv().unwrap();
        let cancel_token = token.clone();
        let (cancelled_tx, cancelled_rx) = std::sync::mpsc::channel();
        let cancel = std::thread::spawn(move || {
            cancel_token.cancel();
            cancelled_tx.send(()).unwrap();
        });
        assert!(
            cancelled_rx
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err()
        );
        release_tx.send(()).unwrap();
        assert_eq!(worker.join().unwrap(), Some(()));
        cancel.join().unwrap();
        assert_eq!(token.run_if_active(|| 1), None);
    }

    #[test]
    fn registration_racing_cancellation_never_loses_the_signal() {
        for _ in 0..256 {
            let token = ChainCancelToken::new();
            let child = ChainCancelToken::child(&token);
            let flag = Arc::new(AtomicBool::new(false));
            std::thread::scope(|scope| {
                scope.spawn(|| child.register_cancel_flag(&flag));
                scope.spawn(|| token.cancel());
            });
            assert!(flag.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn cancellation_callbacks_observe_ancestors_and_can_reenter() {
        let root = ChainCancelToken::new();
        let child = ChainCancelToken::child(&root);
        let notified = Arc::new(AtomicBool::new(false));
        let callback_root = root.clone();
        let callback_notified = notified.clone();
        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            callback_notified.store(true, Ordering::SeqCst);
            callback_root.cancel();
        });
        child.register_cancel_callback(&callback);
        root.cancel();
        assert!(notified.load(Ordering::SeqCst));
        let late = Arc::new(AtomicBool::new(false));
        let late_flag = late.clone();
        let late_callback: Arc<dyn Fn() + Send + Sync> =
            Arc::new(move || late_flag.store(true, Ordering::SeqCst));
        child.register_cancel_callback(&late_callback);
        assert!(late.load(Ordering::SeqCst));
    }

    #[test]
    fn dead_cancellation_subscriptions_are_pruned() {
        let token = ChainCancelToken::new();
        for _ in 0..100 {
            token.register_cancel_flag(&Arc::new(AtomicBool::new(false)));
        }
        let live = Arc::new(AtomicBool::new(false));
        token.register_cancel_flag(&live);
        assert_eq!(token.listeners.lock().unwrap().len(), 1);
        token.cancel();
        assert!(live.load(Ordering::SeqCst));
    }
}
