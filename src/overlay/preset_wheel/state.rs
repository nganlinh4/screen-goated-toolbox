use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize};
use std::sync::{Mutex, Once};

use windows::Win32::Foundation::HWND;
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

lazy_static::lazy_static! {
    pub(crate) static ref PENDING_ITEMS_HTML: Mutex<String> = Mutex::new(String::new());
    pub(crate) static ref PENDING_DISMISS_LABEL: Mutex<String> = Mutex::new(String::new());
    pub(crate) static ref PENDING_CSS: Mutex<String> = Mutex::new(String::new());
    pub(crate) static ref PENDING_POS: Mutex<(i32, i32)> = Mutex::new((0, 0));
}

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

pub(crate) struct HwndWrapper(pub HWND);

unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}

impl raw_window_handle::HasWindowHandle for HwndWrapper {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let raw = raw_window_handle::Win32WindowHandle::new(
            std::num::NonZeroIsize::new(self.0.0 as isize).expect("HWND cannot be null"),
        );
        let handle = raw_window_handle::RawWindowHandle::Win32(raw);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(handle)) }
    }
}
