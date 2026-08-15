use std::collections::HashMap;
use std::sync::{
    Arc, LazyLock, Mutex,
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
}

// SAFETY: AtomicBool is Send+Sync, Arc is Send+Sync. The parent chain is immutable after creation.
unsafe impl Send for ChainCancelToken {}
unsafe impl Sync for ChainCancelToken {}

impl ChainCancelToken {
    /// Create a root token (no parent).
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            parent: None,
        })
    }

    /// Create a child token linked to a parent.
    pub fn child(parent: &Arc<Self>) -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            parent: Some(parent.clone()),
        })
    }

    /// Signal cancellation for this node.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
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
    Image(Vec<u8>), // PNG Bytes
    Audio(Vec<u8>), // WAV Bytes
}

pub struct WindowState {
    pub presentation: ResultPresentation,
    pub control_options: Option<ResultControlOptions>,
    pub backdrop_data_url: Option<String>,
    pub foreground_color: Option<String>,
    pub preferred_font_size: Option<f32>,
    pub copy_success: bool,

    // Edit Mode
    pub is_editing: bool,            // Is the edit box open?
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
    // Chain ID — shared by all windows in the same chain execution
    pub chain_id: Option<String>,
    // Correlates provider and renderer milestones for this chain block.
    pub latency_trace_id: Option<String>,

    // Web Browsing State
    pub is_browsing: bool, // True when user has navigated away from initial content
    pub navigation_depth: usize, // How many pages deep from initial content (0 = at result)
    pub max_navigation_depth: usize, // Max depth reached (to know if forward is possible)
    // Speaker/TTS state
    pub tts_request_id: u64,       // Active TTS request ID (0 = not speaking)
    pub tts_loading: bool,         // True when TTS is loading/connecting (shows spinner)
    pub opacity_percent: u8,       // Transparency level (0-100)
    pub preset_id: Option<String>, // ID of the preset that spawned this window
    pub is_chain_root: bool,       // True if this is the first window in a chain
}

// SAFETY: Raw pointers are not Send/Sync, but we only use them within the main thread
// This is safe because all access is synchronized via WINDOW_STATES mutex
unsafe impl Send for WindowState {}
unsafe impl Sync for WindowState {}

pub static WINDOW_STATES: LazyLock<Mutex<HashMap<isize, WindowState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub enum WindowType {
    Primary,
    // Note: Secondary and SecondaryExplicit were removed as dead code
}

pub fn link_windows(hwnd1: HWND, hwnd2: HWND) {
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
    super::scene_compositor::sync_controls(hwnd1);
    super::scene_compositor::sync_controls(hwnd2);
}

use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW, WM_CLOSE};

/// Close all windows belonging to a chain (by chain_id).
/// Signals each window's cancellation token and posts WM_CLOSE.
/// Used in continuous input mode to destroy previous result overlays before spawning new ones.
pub fn close_chain_windows(chain_id: &str) {
    let mut to_close = Vec::new();
    {
        let states = WINDOW_STATES.lock().unwrap();
        for (&h_val, state) in states.iter() {
            if state.chain_id.as_deref() == Some(chain_id) {
                // Signal this window's token to stop its branch
                if let Some(ref token) = state.cancellation_token {
                    token.cancel();
                }
                to_close.push(HWND(h_val as *mut std::ffi::c_void));
            }
        }
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
    use super::ResultControlOptions;

    #[test]
    fn anchored_controls_follow_committed_result_movement() {
        let mut options = ResultControlOptions {
            anchor_rect: Some([10, 20, 300, 180]),
            ..Default::default()
        };

        assert!(options.shift_anchor(7, -4));
        assert_eq!(options.anchor_rect, Some([17, 16, 300, 180]));
    }
}
