use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::HBITMAP;

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

// --- DYNAMIC PARTICLES ---
pub struct DustParticle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32, // 1.0 to 0.0
    pub size: f32,
    pub color: u32,
}

#[derive(Clone, Copy, PartialEq)]
pub enum AnimationMode {
    Idle, // Normal mouse movement
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ResizeEdge {
    None,
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, PartialEq)]
pub enum InteractionMode {
    None,
    DraggingWindow,
    DraggingGroup(Vec<(HWND, RECT)>),
    Resizing(ResizeEdge),
    ResizingGroup(Vec<(HWND, RECT)>, ResizeEdge),
}

pub struct CursorPhysics {
    pub x: f32,
    pub y: f32,

    // Spring Physics
    pub current_tilt: f32,  // Current angle in degrees
    pub tilt_velocity: f32, // Angular velocity

    // Deformation
    pub squish_factor: f32, // 1.0 = normal, 0.5 = flat
    pub bristle_bend: f32,  // Lag of bristles

    // Logic
    pub mode: AnimationMode,

    pub particles: Vec<DustParticle>,

    // Clean up
    pub initialized: bool,
    pub needs_cleanup_repaint: bool, // Flag to trigger one final repaint when entering DragOut
}

impl Default for CursorPhysics {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            current_tilt: 0.0,
            tilt_velocity: 0.0,
            squish_factor: 1.0,
            bristle_bend: 0.0,
            mode: AnimationMode::Idle,

            particles: Vec::new(),
            initialized: false,
            needs_cleanup_repaint: false,
        }
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
    pub is_hovered: bool,
    pub on_copy_btn: bool,
    pub copy_success: bool,
    pub on_edit_btn: bool,
    pub on_undo_btn: bool,
    pub on_redo_btn: bool, // Redo button hover state

    // Edit Mode
    pub is_editing: bool,            // Is the edit box open?
    pub context_data: RefineContext, // Data needed for API call
    pub full_text: String,           // Current full text content

    // Text History for Undo/Redo
    pub text_history: Vec<String>, // Stack of previous text states (for Undo)
    pub redo_history: Vec<String>, // Stack of undone text states (for Redo)

    // Refinement State
    pub is_refining: bool,
    pub animation_offset: f32,

    // Streaming state - true when actively receiving chunks (buttons hidden during streaming)
    pub is_streaming_active: bool,
    // Tracks previous streaming state to detect when streaming ends (for flush logic)
    pub was_streaming_active: bool,

    // Metadata for Refinement/Processing
    pub model_id: String,
    pub provider: String,
    pub streaming_enabled: bool,

    // NEW: Preset Prompt for "Type" mode logic
    pub preset_prompt: String,
    // NEW: Input text currently being refined/processed
    pub input_text: String,

    pub bg_color: u32,
    pub linked_window: Option<HWND>,
    pub physics: CursorPhysics,

    // --- INTERACTION STATE ---
    pub interaction_mode: InteractionMode,
    pub current_resize_edge: ResizeEdge, // Track edge hover state for painting
    pub drag_start_mouse: POINT,
    pub drag_start_window_rect: RECT,
    pub has_moved_significantly: bool, // To distinguish click vs drag

    // --- CACHING & THROTTLING ---
    pub font_cache_dirty: bool,
    pub cached_font_size: i32,
    pub content_bitmap: HBITMAP,
    pub last_w: i32,
    pub last_h: i32,

    // Handle pending updates to avoid flooding Paint
    pub pending_text: Option<String>,

    // Timestamp for throttling text updates (in milliseconds)
    pub last_text_update_time: u32,

    // Resize debounce: timestamp of last resize to skip expensive font calculations during active resize
    pub last_resize_time: u32,

    // Font recalc throttling: timestamp of last font recalculation (for 200ms streaming throttle)
    pub last_font_calc_time: u32,

    pub last_webview_update_time: u32,

    // BACKGROUND CACHING
    pub bg_bitmap: HBITMAP,
    pub bg_w: i32,
    pub bg_h: i32,

    // Graphics mode for refining animation (standard vs minimal)
    pub graphics_mode: String,

    // Cancellation token — hierarchical; cancel propagates to descendants
    pub cancellation_token: Option<Arc<ChainCancelToken>>,
    // Chain ID — shared by all windows in the same chain execution
    pub chain_id: Option<String>,

    // Markdown mode state
    pub is_markdown_mode: bool,      // True when showing markdown view
    pub is_markdown_streaming: bool, // True when using markdown_stream render mode (uses streaming update)
    pub on_markdown_btn: bool,       // Hover state for markdown button

    // Web Browsing State
    pub is_browsing: bool, // True when user has navigated away from initial content
    pub navigation_depth: usize, // How many pages deep from initial content (0 = at result)
    pub max_navigation_depth: usize, // Max depth reached (to know if forward is possible)
    pub on_back_btn: bool, // Hover state for back button
    pub on_forward_btn: bool, // Hover state for forward button

    // Download HTML button state
    pub on_download_btn: bool, // Hover state for download HTML button

    // Speaker/TTS button state
    pub on_speaker_btn: bool,      // Hover state for speaker button
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

lazy_static::lazy_static! {
    pub static ref WINDOW_STATES: Mutex<HashMap<isize, WindowState>> = Mutex::new(HashMap::new());
}

pub enum WindowType {
    Primary,
    // Note: Secondary and SecondaryExplicit were removed as dead code
}

pub fn link_windows(hwnd1: HWND, hwnd2: HWND) {
    let mut states = WINDOW_STATES.lock().unwrap();
    if let Some(s1) = states.get_mut(&(hwnd1.0 as isize)) {
        s1.linked_window = Some(hwnd2);
    }
    if let Some(s2) = states.get_mut(&(hwnd2.0 as isize)) {
        s2.linked_window = Some(hwnd1);
    }
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

/// Get a group of windows linked via `linked_window` BFS.
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
            if let Some(linked) = s.linked_window {
                if states.contains_key(&(linked.0 as isize)) && !visited.contains(&linked.0) {
                    visited.insert(linked.0);
                    queue.push_back(linked);
                }
            }
        }
    }

    group
}

/// Set the interaction mode for a specific window
pub fn set_window_interaction_mode(hwnd: HWND, mode: InteractionMode) {
    let mut states = WINDOW_STATES.lock().unwrap();
    if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
        state.interaction_mode = mode;
    }
}
