// --- RECORDING STATE ---
// Global signals, atomics, and shared state for recording overlay.

use std::cell::RefCell;
use std::sync::{
    atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicU64, Ordering},
    Arc, Mutex, Once,
};
use windows::Win32::UI::WindowsAndMessaging::WM_USER;
use wry::{WebContext, WebView};

// --- GLOBAL SIGNALS ---
lazy_static::lazy_static! {
    pub static ref AUDIO_STOP_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref AUDIO_PAUSE_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref AUDIO_ABORT_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref AUDIO_WARMUP_COMPLETE: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    /// Signal for Gemini Live initialization phase (WebSocket setup)
    pub static ref AUDIO_INITIALIZING: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    pub static ref VISUALIZATION_BUFFER: Mutex<[f32; 40]> = Mutex::new([0.0; 40]);
}

// --- ATOMIC STATE ---
pub static LAST_SHOW_TIME: AtomicU64 = AtomicU64::new(0);
pub static CURRENT_RMS: AtomicU32 = AtomicU32::new(0);

// 0=Not Created, 1=Hidden/Warmup, 2=Visible/Recording
pub static RECORDING_STATE: AtomicI32 = AtomicI32::new(0);
pub static RECORDING_HWND_VAL: AtomicIsize = AtomicIsize::new(0);
pub static REGISTER_RECORDING_CLASS: Once = Once::new();
pub static LAST_THEME_IS_DARK: AtomicBool = AtomicBool::new(true);
pub static CURRENT_RECORDING_HIDDEN: AtomicBool = AtomicBool::new(false);

// --- THREAD LOCAL ---
thread_local! {
    pub static RECORDING_WEBVIEW: RefCell<Option<WebView>> = RefCell::new(None);
    pub static RECORDING_WEB_CONTEXT: RefCell<Option<WebContext>> = RefCell::new(None);
}

// --- WINDOW MESSAGES ---
pub const WM_APP_SHOW: u32 = WM_USER + 20;
pub const WM_APP_HIDE: u32 = WM_USER + 21;
pub const WM_APP_REAL_SHOW: u32 = WM_USER + 22;
pub const WM_APP_UPDATE_STATE: u32 = WM_USER + 23;
pub const WM_USER_FULL_CLOSE: u32 = WM_USER + 99;

// --- HELPER FUNCTIONS ---

pub fn update_audio_viz(rms: f32) {
    let bits = rms.to_bits();
    CURRENT_RMS.store(bits, Ordering::Relaxed);
}

/// Get adaptive UI dimensions based on screen aspect ratio
pub fn get_ui_dimensions() -> (i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };

    // Width scales inversely with aspect ratio for consistent UI appearance
    // At 16:9 (1.78:1): 450px width
    // At 21:9 (2.37:1): 375px width (narrower on ultrawide)
    let aspect_ratio = screen_w as f64 / screen_h as f64;
    let base_aspect = 16.0 / 9.0; // 1.778
    let width = (450.0 - (aspect_ratio - base_aspect) * 127.0).clamp(350.0, 500.0) as i32;

    // Height stays constant at 70px
    let height = 70;

    (width, height)
}
