//! Image Continuous Mode
//!
//! A non-blocking image selection mode that allows users to work normally while
//! being able to capture screen regions using right-click/drag gestures.

mod gesture;
mod graphics;
mod hooks;
mod lifecycle;
mod logic;
mod worker;

pub(crate) use graphics::{FrozenSelectionRender, dim_pixels, render_frozen_with_selection};

use crate::GdiCapture;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// === STATE ===

static IS_ACTIVE: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
static LIFECYCLE: LazyLock<Mutex<lifecycle::Lifecycle>> =
    LazyLock::new(|| Mutex::new(lifecycle::Lifecycle::default()));
static CAPTURE_TARGET: Mutex<crate::overlay::image_capture_target::ImageCaptureTarget> =
    Mutex::new(crate::overlay::image_capture_target::ImageCaptureTarget::Preset(0));
static TRIGGER_VK: AtomicU32 = AtomicU32::new(0);
static TRIGGER_ID: AtomicI32 = AtomicI32::new(0);
static HAS_RELEASED_SINCE_ACTIVATION: AtomicBool = AtomicBool::new(false);

// Gesture state
static GESTURE: gesture::Gesture = gesture::Gesture::new();
static DRAG_RENDER_PENDING: AtomicBool = AtomicBool::new(false);
static START_X: AtomicI32 = AtomicI32::new(0);
static START_Y: AtomicI32 = AtomicI32::new(0);
static LAST_X: AtomicI32 = AtomicI32::new(0);
static LAST_Y: AtomicI32 = AtomicI32::new(0);

// Animation State for Dim Screen
static mut CURRENT_DIM_ALPHA: u8 = 0;
const TARGET_DIM_ALPHA: u8 = 100;
const DIM_FADE_STEP: u8 = 20; // Fast fade (approx 5 frames @ 60fps)
const DIM_TIMER_ID: usize = 5;
const DRAG_RENDER_MESSAGE: u32 = WM_APP + 17;
const DRAG_CANCEL_MESSAGE: u32 = WM_APP + 18;
const GESTURE_BEGIN_MESSAGE: u32 = WM_APP + 19;
const GESTURE_END_MESSAGE: u32 = WM_APP + 20;
const GESTURE_ZOOM_MESSAGE: u32 = WM_APP + 21;

// Zoom state (while right is held + wheel)
static ZOOM_LEVEL: Mutex<f32> = Mutex::new(1.0);

// Per-gesture capture
struct CapturedGesture {
    image: GdiCapture,
    destination: crate::overlay::process::chain::AutoPasteDestination,
}
static GESTURE_CAPTURE: Mutex<Option<CapturedGesture>> = Mutex::new(None);

// Whether continuous mode holds a shared magnifier lease.
static MAG_LEASE_HELD: AtomicBool = AtomicBool::new(false);

// Cached overlay DIB section (avoid per-frame allocation — reused across renders)
static OVERLAY_DIB: Mutex<crate::overlay::cached_dib::CachedDib> =
    Mutex::new(crate::overlay::cached_dib::CachedDib::empty());
static mut LAST_RENDERED_SELECTION: Option<RECT> = None;
static mut LAST_RENDERED_DIM_ALPHA: u8 = 0;

// Window and Hook Handles (Managed by the thread)
static OVERLAY_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static RECT_OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);

// Hotkey tracking for exit
static HOTKEY_NAME: Mutex<String> = Mutex::new(String::new());

// === PUBLIC API ===

pub fn is_active() -> bool {
    IS_ACTIVE.load(Ordering::SeqCst)
}

pub fn enter(
    target: crate::overlay::image_capture_target::ImageCaptureTarget,
    hotkey_name: String,
    hotkey_id: i32,
) {
    let trigger_vk =
        crate::overlay::continuous_mode::get_current_hotkey_info().map_or(0, |(_, vk)| vk);
    let request = lifecycle::Request {
        target,
        hotkey_name,
        hotkey_id,
        trigger_vk,
    };
    let admission = {
        let mut state = LIFECYCLE.lock().unwrap();
        let admission = state.enter(request);
        if matches!(
            admission,
            lifecycle::Admission::Start(..) | lifecycle::Admission::Queued
        ) {
            IS_ACTIVE.store(true, Ordering::SeqCst);
        }
        admission
    };
    launch(admission);
}

pub fn exit() {
    if !IS_ACTIVE.load(Ordering::SeqCst) {
        return;
    }

    // If currently dragging, cancel the drag instead of exiting the mode.
    // A replacement waiting for the old owner is cancelled directly instead.
    let mut state = LIFECYCLE.lock().unwrap();
    if !state.running() {
        let thread_id = state.stop();
        IS_ACTIVE.store(false, Ordering::SeqCst);
        RUNNING.store(false, Ordering::SeqCst);
        wake_stopping(thread_id);
        return;
    }
    match GESTURE.cancel() {
        gesture::Cancel::Cancelled => {
            DRAG_RENDER_PENDING.store(false, Ordering::SeqCst);
            let hwnd = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
            if hwnd != 0 {
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd as *mut _)),
                        DRAG_CANCEL_MESSAGE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
            crate::log_info!("[ImageContinuous] Drag cancelled via exit()");
            return;
        }
        gesture::Cancel::AlreadyCancelled => return,
        gesture::Cancel::Idle => {}
    }

    crate::log_info!("[ImageContinuous] Mode exit() called");

    let thread_id = state.stop();
    IS_ACTIVE.store(false, Ordering::SeqCst);
    RUNNING.store(false, Ordering::SeqCst);
    crate::overlay::text_selection::set_image_continuous_badge(false);
    wake_stopping(thread_id);
    drop(state);

    crate::log_info!("[ImageContinuous] Mode exited (Quit signal posted)");
}

fn wake_stopping(thread_id: Option<u32>) {
    if let Some(thread_id) = thread_id.filter(|id| *id != 0) {
        unsafe {
            if PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)).is_err() {
                let hwnd = RECT_OVERLAY_HWND.load(Ordering::SeqCst);
                if hwnd != 0 {
                    let _ =
                        PostMessageW(Some(HWND(hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }
    }
}

fn launch(admission: lifecycle::Admission) {
    match admission {
        lifecycle::Admission::Start(id, request) => {
            if std::thread::Builder::new()
                .name("image-continuous".into())
                .spawn(move || {
                    let result = std::panic::catch_unwind(|| worker::run(id, request));
                    let failed = !matches!(result, Ok(Ok(())));
                    retire(id, failed);
                })
                .is_err()
            {
                retire(id, true);
            }
        }
        lifecycle::Admission::Exhausted => {
            IS_ACTIVE.store(false, Ordering::SeqCst);
            report_failure();
        }
        lifecycle::Admission::Queued | lifecycle::Admission::Ignored => {}
    }
}

fn retire(id: u64, failed: bool) {
    let (next, report) = {
        let mut state = LIFECYCLE.lock().unwrap();
        if !state.owns(id) {
            return;
        }
        let report = failed && state.live(id);
        RUNNING.store(false, Ordering::SeqCst);
        crate::overlay::text_selection::set_image_continuous_badge(false);
        let next = state.finish(id);
        if !matches!(next, lifecycle::Admission::Start(..)) {
            IS_ACTIVE.store(false, Ordering::SeqCst);
        }
        (next, report)
    };
    if report {
        report_failure();
    }
    launch(next);
}

fn report_failure() {
    crate::log_info!("[ImageContinuous] Could not start capture mode");
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let message = match language.as_str() {
        "vi" => "Không thể bắt đầu chụp ảnh liên tục",
        "ko" => "이미지 연속 캡처를 시작할 수 없습니다",
        _ => "Could not start continuous image capture",
    };
    crate::overlay::auto_copy_badge::show_error_notification(message);
}

pub fn get_target() -> crate::overlay::image_capture_target::ImageCaptureTarget {
    LIFECYCLE
        .lock()
        .unwrap()
        .desired_binding()
        .map_or_else(|| *CAPTURE_TARGET.lock().unwrap(), |(target, _, _)| target)
}

pub fn get_trigger_id() -> i32 {
    LIFECYCLE.lock().unwrap().desired_binding().map_or_else(
        || TRIGGER_ID.load(Ordering::SeqCst),
        |(_, trigger_id, _)| trigger_id,
    )
}

pub fn can_exit_now() -> bool {
    // We can exit if the hotkey was released at least once since we entered the mode.
    // This prevents the initial "hold" that activated the mode from immediately toggling it off.
    LIFECYCLE
        .lock()
        .unwrap()
        .desired_binding()
        .is_some_and(|(_, _, ready)| ready && HAS_RELEASED_SINCE_ACTIVATION.load(Ordering::SeqCst))
}
