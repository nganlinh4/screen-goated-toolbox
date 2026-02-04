// --- TEXT INPUT STATE ---
// Shared state, atomics, and constants for text input overlay.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize};
use std::sync::{Mutex, Once};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::WM_USER;
use wry::WebContext;

// --- REGISTRATION ---
pub static REGISTER_INPUT_CLASS: Once = Once::new();

// --- ATOMIC STATE ---
pub static INPUT_HWND: AtomicIsize = AtomicIsize::new(0);
pub static IS_WARMING_UP: AtomicBool = AtomicBool::new(false);
pub static IS_WARMED_UP: AtomicBool = AtomicBool::new(false);
pub static IS_SHOWING: AtomicBool = AtomicBool::new(false);

// --- WINDOW MESSAGES ---
pub const WM_APP_SHOW: u32 = WM_USER + 99;
pub const WM_APP_SET_TEXT: u32 = WM_USER + 100;
pub const WM_APP_HIDE: u32 = WM_USER + 101;

// --- LAZY STATIC ---
lazy_static::lazy_static! {
    pub static ref SUBMITTED_TEXT: Mutex<Option<String>> = Mutex::new(None);
    pub static ref SHOULD_CLOSE: Mutex<bool> = Mutex::new(false);
    pub static ref SHOULD_CLEAR_ONLY: Mutex<bool> = Mutex::new(false);

    // Config Storage (Thread-safe for persistent window)
    pub static ref CFG_TITLE: Mutex<String> = Mutex::new(String::new());
    pub static ref CFG_LANG: Mutex<String> = Mutex::new(String::new());
    pub static ref CFG_CANCEL: Mutex<String> = Mutex::new(String::new());
    pub static ref CFG_CALLBACK: Mutex<Option<Box<dyn Fn(String, HWND) + Send>>> = Mutex::new(None);
    pub static ref CFG_CONTINUOUS: Mutex<bool> = Mutex::new(false);

    // Cross-thread text injection (for auto-paste from transcription)
    pub static ref PENDING_TEXT: Mutex<Option<String>> = Mutex::new(None);
}

// --- THREAD LOCAL ---
thread_local! {
    pub static TEXT_INPUT_WEBVIEW: RefCell<Option<wry::WebView>> = RefCell::new(None);
    pub static TEXT_INPUT_WEB_CONTEXT: RefCell<Option<WebContext>> = RefCell::new(None);
}
