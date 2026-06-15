use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize};
use std::sync::{LazyLock, Mutex, Once};

use windows::Win32::UI::WindowsAndMessaging::WM_USER;
use wry::{WebContext, WebView};

pub(crate) static REGISTER_WHEEL_CLASS: Once = Once::new();
pub(crate) static REGISTER_OVERLAY_CLASS: Once = Once::new();

pub(crate) const WM_APP_SHOW: u32 = WM_USER + 10;
pub(crate) const WM_APP_HIDE: u32 = WM_USER + 11;
pub(crate) const WM_APP_REAL_SHOW: u32 = WM_USER + 12;

pub(crate) const WHEEL_WIDTH: i32 = 1200;
pub(crate) const WHEEL_HEIGHT: i32 = 700;

pub(crate) static WHEEL_RESULT: AtomicI32 = AtomicI32::new(-1);
pub(crate) static WHEEL_ACTIVE: AtomicBool = AtomicBool::new(false);

pub(crate) static WHEEL_HWND: AtomicIsize = AtomicIsize::new(0);
pub(crate) static OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);
pub(crate) static IS_WARMING_UP: AtomicBool = AtomicBool::new(false);
pub(crate) static IS_WARMED_UP: AtomicBool = AtomicBool::new(false);

pub(crate) static PENDING_ITEMS_HTML: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
pub(crate) static PENDING_DISMISS_LABEL: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
pub(crate) static PENDING_CSS: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
pub(crate) static PENDING_POS: LazyLock<Mutex<(i32, i32)>> = LazyLock::new(|| Mutex::new((0, 0)));

thread_local! {
    pub(crate) static WHEEL_WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
    pub(crate) static WHEEL_WEB_CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug)]
pub(crate) struct WheelEntry {
    pub selection_id: usize,
    pub label: String,
}

impl WheelEntry {
    pub(crate) fn new(selection_id: usize, label: impl Into<String>) -> Self {
        Self {
            selection_id,
            label: label.into(),
        }
    }
}

pub(crate) use crate::win_types::HwndWrapper;
