// --- TEXT INPUT STATE ---
// Shared state, atomics, and constants for text input overlay.

use crate::win_types::SendHhook;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize};
use std::sync::{LazyLock, Mutex, Once};
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
pub static PASSIVE_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);

// --- WINDOW MESSAGES ---
pub const WM_APP_SHOW: u32 = WM_USER + 99;
pub const WM_APP_SET_TEXT: u32 = WM_USER + 100;
pub const WM_APP_HIDE: u32 = WM_USER + 101;
pub const WM_APP_SYNC_PASSIVE_EDITOR: u32 = WM_USER + 102;

type TextSubmitCallback = Box<dyn Fn(String, HWND) + Send>;

// --- LAZY STATICS ---
pub static SUBMITTED_TEXT: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
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
pub static PENDING_TEXT: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

// --- THREAD LOCAL ---
thread_local! {
    pub static TEXT_INPUT_WEBVIEW: RefCell<Option<wry::WebView>> = const { RefCell::new(None) };
    pub static TEXT_INPUT_WEB_CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
}
