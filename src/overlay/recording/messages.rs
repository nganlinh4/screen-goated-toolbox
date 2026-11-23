use super::state::*;
use super::window::start_audio_thread;
use crate::APP;
use crate::overlay::store_support::now_ms;
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

thread_local! {
    static VISUAL_OWNER: RefCell<super::lifecycle::VisualOwner> = RefCell::default();
}
static NEXT_SHOW_TIMER: AtomicUsize = AtomicUsize::new(1_000);

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
            WM_TIMER if wparam.0 >= 1_000 => {
                let _ = KillTimer(Some(hwnd), wparam.0);
                let session = VISUAL_OWNER.with(|owner| owner.borrow_mut().take_timer(wparam.0));
                if let Some(session) = session
                    && is_current_recording_id(session)
                {
                    show_visual(hwnd);
                }
                LRESULT(0)
            }
            WM_APP_REAL_SHOW => {
                schedule_show(hwnd, wparam.0, 20);
                LRESULT(0)
            }
            WM_APP_HIDE => {
                if is_visual_owner(wparam.0) && is_current_recording_id(wparam.0) {
                    hide_visual(hwnd);
                }
                LRESULT(0)
            }
            WM_APP_UPDATE_STATE => {
                push_visual_state();
                LRESULT(0)
            }
            WM_APP_SESSION_CLOSE => {
                if is_visual_owner(wparam.0) && is_current_recording_id(wparam.0) {
                    hide_visual(hwnd);
                }
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
    let Some(session) = start_audio_thread(hwnd, preset_idx) else {
        crate::log_info!("[Recording] preset {preset_idx} no longer exists; show ignored");
        return;
    };
    let previous = VISUAL_OWNER.with(|owner| owner.borrow_mut().begin(session));
    unsafe {
        let _ = KillTimer(Some(hwnd), 1);
        if let Some(timer) = previous {
            let _ = KillTimer(Some(hwnd), timer);
        }
    }
    RECORDING_STATE.store(2, Ordering::SeqCst);

    let (hidden, subtext) = {
        let app = APP.lock().unwrap();
        let preset = app.config.presets.get(preset_idx);
        let locale = crate::gui::locale::LocaleText::get(&app.config.ui_language);
        (
            preset.is_some_and(|preset| preset.hide_recording_ui),
            super::hint::stop_hint(
                locale.shell.recording_subtext,
                preset.map_or(&[], |preset| preset.hotkeys.as_slice()),
            ),
        )
    };
    CURRENT_RECORDING_HIDDEN.store(hidden, Ordering::SeqCst);
    LAST_SHOW_TIME.store(now_ms(), Ordering::SeqCst);
    if hidden {
        crate::overlay::status_compositor::recording_hide();
    } else {
        crate::overlay::status_compositor::recording_prepare(recording_rect(), subtext, session);
        schedule_show(hwnd, session, 500);
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
    let timer = VISUAL_OWNER.with(|owner| {
        let mut owner = owner.borrow_mut();
        owner
            .current()
            .and_then(|session| owner.retire(session))
            .flatten()
    });
    unsafe {
        let _ = KillTimer(Some(hwnd), 1);
        if let Some(timer) = timer {
            let _ = KillTimer(Some(hwnd), timer);
        }
    }
    crate::overlay::status_compositor::recording_hide();
    RECORDING_STATE.store(1, Ordering::SeqCst);
}

fn is_visual_owner(session: usize) -> bool {
    VISUAL_OWNER.with(|owner| owner.borrow().current() == Some(session))
}

fn schedule_show(hwnd: HWND, session: usize, delay_ms: u32) {
    if !is_current_recording_id(session) {
        return;
    }
    let Ok(timer) = NEXT_SHOW_TIMER.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
        next.checked_add(1)
    }) else {
        return;
    };
    let hidden = CURRENT_RECORDING_HIDDEN.load(Ordering::SeqCst);
    let previous = VISUAL_OWNER.with(|owner| owner.borrow_mut().schedule(session, timer, hidden));
    let Some(previous) = previous else {
        return;
    };
    unsafe {
        if let Some(previous) = previous {
            let _ = KillTimer(Some(hwnd), previous);
        }
        if SetTimer(Some(hwnd), timer, delay_ms, None) == 0 {
            let session = VISUAL_OWNER.with(|owner| owner.borrow_mut().take_timer(timer));
            if session.is_some_and(is_current_recording_id) {
                show_visual(hwnd);
            }
        }
    }
}

fn push_visual_state() {
    let state = if is_current_recording_stopped() {
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
    let elapsed = now_ms().saturating_sub(LAST_SHOW_TIME.load(Ordering::SeqCst));
    if super::lifecycle::manual_close_allowed(
        is_current_recording_stopped(),
        AUDIO_ABORT_SIGNAL.load(Ordering::SeqCst),
        elapsed,
    ) {
        hide_visual(hwnd);
    }
}

fn recording_rect() -> crate::overlay::status_compositor::protocol::PhysicalRect {
    let (width, height) = get_ui_dimensions();
    let area = crate::overlay::display_placement::primary_work_area();
    let centered = area.centered(width, height);
    let rect = area.fit(
        centered.left,
        centered.top + (area.height / 10).min(100),
        width,
        height,
    );
    crate::overlay::status_compositor::physical_rect(
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    )
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
