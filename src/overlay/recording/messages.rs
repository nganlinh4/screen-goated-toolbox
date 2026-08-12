use super::state::*;
use super::window::start_audio_thread;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub unsafe extern "system" fn recording_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_APP_SHOW => {
                begin_session(hwnd, wparam.0);
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == 1 => {
                push_visual_state();
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == 2 || wparam.0 == 99 => {
                let _ = KillTimer(Some(hwnd), wparam.0);
                let _ = PostMessageW(Some(hwnd), WM_APP_REAL_SHOW, WPARAM(0), LPARAM(0));
                LRESULT(0)
            }
            WM_APP_REAL_SHOW => {
                show_visual(hwnd);
                LRESULT(0)
            }
            WM_APP_HIDE => {
                hide_visual(hwnd);
                LRESULT(0)
            }
            WM_APP_UPDATE_STATE => {
                push_visual_state();
                LRESULT(0)
            }
            WM_CLOSE => {
                handle_close(hwnd);
                LRESULT(0)
            }
            WM_USER_FULL_CLOSE => {
                let _ = DestroyWindow(hwnd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

fn begin_session(hwnd: HWND, preset_idx: usize) {
    AUDIO_STOP_SIGNAL.store(false, Ordering::SeqCst);
    AUDIO_PAUSE_SIGNAL.store(false, Ordering::SeqCst);
    AUDIO_ABORT_SIGNAL.store(false, Ordering::SeqCst);
    AUDIO_WARMUP_COMPLETE.store(false, Ordering::SeqCst);
    CURRENT_RMS.store(0, Ordering::Relaxed);
    start_audio_thread(hwnd, preset_idx);
    RECORDING_STATE.store(2, Ordering::SeqCst);

    let hidden = APP
        .lock()
        .unwrap()
        .config
        .presets
        .get(preset_idx)
        .is_some_and(|preset| preset.hide_recording_ui);
    CURRENT_RECORDING_HIDDEN.store(hidden, Ordering::SeqCst);
    LAST_SHOW_TIME.store(now_ms(), Ordering::SeqCst);
    if hidden {
        crate::overlay::status_compositor::recording_hide();
    } else {
        crate::overlay::status_compositor::recording_prepare(recording_rect());
        unsafe {
            let _ = SetTimer(Some(hwnd), 99, 500, None);
        }
    }
}

fn show_visual(hwnd: HWND) {
    if CURRENT_RECORDING_HIDDEN.load(Ordering::SeqCst) {
        return;
    }
    crate::overlay::status_compositor::recording_show(recording_rect());
    unsafe {
        let _ = SetTimer(Some(hwnd), 1, 16, None);
    }
    push_visual_state();
}

fn hide_visual(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(Some(hwnd), 1);
        let _ = KillTimer(Some(hwnd), 2);
        let _ = KillTimer(Some(hwnd), 99);
    }
    crate::overlay::status_compositor::recording_hide();
    RECORDING_STATE.store(1, Ordering::SeqCst);
}

fn push_visual_state() {
    let state = if AUDIO_STOP_SIGNAL.load(Ordering::SeqCst) {
        "processing"
    } else if AUDIO_PAUSE_SIGNAL.load(Ordering::SeqCst) {
        "paused"
    } else if AUDIO_INITIALIZING.load(Ordering::SeqCst) {
        "initializing"
    } else if !AUDIO_WARMUP_COMPLETE.load(Ordering::SeqCst) {
        "warmup"
    } else {
        "recording"
    };
    let rms = f32::from_bits(CURRENT_RMS.load(Ordering::Relaxed));
    crate::overlay::status_compositor::recording_update(state, rms);

    if let Ok(app) = APP.try_lock() {
        let is_dark = app.config.theme_mode.is_dark();
        if LAST_THEME_IS_DARK.swap(is_dark, Ordering::SeqCst) != is_dark {
            crate::overlay::status_compositor::update_theme(is_dark);
        }
    }
}

fn handle_close(hwnd: HWND) {
    if AUDIO_STOP_SIGNAL.load(Ordering::SeqCst) || AUDIO_ABORT_SIGNAL.load(Ordering::SeqCst) {
        hide_visual(hwnd);
        return;
    }
    let elapsed = now_ms().saturating_sub(LAST_SHOW_TIME.load(Ordering::SeqCst));
    if elapsed >= 2_000 {
        hide_visual(hwnd);
    }
}

fn recording_rect() -> crate::overlay::status_compositor::protocol::PhysicalRect {
    let (width, height) = get_ui_dimensions();
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    crate::overlay::status_compositor::physical_rect(
        (screen_width - width) / 2,
        (screen_height - height) / 2 + 100,
        width,
        height,
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub unsafe extern "system" fn recording_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 {
            let keyboard = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            if (wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize)
                && keyboard.vkCode == VK_ESCAPE.0 as u32
                && super::is_recording_overlay_active()
            {
                super::stop_recording_and_submit();
                return LRESULT(1);
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}
