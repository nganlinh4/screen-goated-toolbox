use super::html::generate_items_html;
use super::liveness::{WheelWaitState, wait_for_selection};
use super::runtime::internal_create_window_loop;
use super::state::{
    IS_WARMED_UP, IS_WARMING_UP, PENDING_CSS, PENDING_DISMISS_LABEL, PENDING_ITEMS_HTML,
    PENDING_POS, PENDING_SIZE, WHEEL_ACTIVE, WHEEL_HEIGHT, WHEEL_HWND, WHEEL_LIVENESS,
    WHEEL_PROTOCOL_LOCK, WHEEL_READY_ID, WHEEL_REQUEST_ID, WHEEL_REQUEST_IN_PROGRESS, WHEEL_RESULT,
    WHEEL_WIDTH, WheelEntry,
};
use super::styles::generate_css;
use crate::APP;
use crate::config::Preset;
use crate::gui::settings_ui::get_localized_preset_name;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, IsWindow, MSG, PM_REMOVE, PeekMessageW, PostMessageW, PostQuitMessage,
    TranslateMessage, WM_CLOSE, WM_QUIT,
};

struct WheelRequest;

impl WheelRequest {
    fn try_acquire() -> Option<Self> {
        WHEEL_REQUEST_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| Self)
    }
}

impl Drop for WheelRequest {
    fn drop(&mut self) {
        let _protocol = WHEEL_PROTOCOL_LOCK.lock().unwrap();
        WHEEL_ACTIVE.store(false, Ordering::SeqCst);
        WHEEL_LIVENESS.lock().unwrap().clear();
        WHEEL_REQUEST_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

pub fn warmup() {
    if IS_WARMED_UP.load(Ordering::SeqCst) {
        return;
    }
    if IS_WARMING_UP
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    std::thread::spawn(|| {
        internal_create_window_loop();
    });
}

pub fn show_preset_wheel(
    filter_type: &str,
    filter_mode: Option<&str>,
    center_pos: POINT,
) -> Option<usize> {
    show_preset_wheel_with_extra(filter_type, filter_mode, center_pos, Vec::new())
}

/// Return the identity from the snapshot displayed to the user, even if the
/// current configuration changes while the wheel is open.
pub fn show_preset_wheel_id(
    filter_type: &str,
    filter_mode: Option<&str>,
    center_pos: POINT,
) -> Option<String> {
    let (presets, ui_lang) = {
        let app = APP.lock().unwrap();
        (app.config.presets.clone(), app.config.ui_language.clone())
    };
    let entries = presets
        .iter()
        .enumerate()
        .filter(|(_, preset)| should_show_preset(preset, filter_type, filter_mode))
        .map(|(idx, preset)| WheelEntry::new(idx, get_localized_preset_name(&preset.id, &ui_lang)))
        .collect();
    let selected = show_entries(entries, center_pos, &ui_lang)?;
    presets.get(selected).map(|preset| preset.id.clone())
}

pub fn show_preset_wheel_with_extra(
    filter_type: &str,
    filter_mode: Option<&str>,
    center_pos: POINT,
    extra_entries: Vec<(usize, String)>,
) -> Option<usize> {
    let (presets, ui_lang) = {
        let app = APP.lock().unwrap();
        (app.config.presets.clone(), app.config.ui_language.clone())
    };

    let mut entries: Vec<WheelEntry> = presets
        .iter()
        .enumerate()
        .filter(|(_, preset)| should_show_preset(preset, filter_type, filter_mode))
        .map(|(idx, preset)| WheelEntry::new(idx, get_localized_preset_name(&preset.id, &ui_lang)))
        .collect();
    entries.extend(
        extra_entries
            .into_iter()
            .map(|(selection_id, label)| WheelEntry::new(selection_id, label)),
    );

    show_entries(entries, center_pos, &ui_lang)
}

pub fn show_custom_wheel(entries: Vec<(usize, String)>, center_pos: POINT) -> Option<usize> {
    let ui_lang = APP.lock().unwrap().config.ui_language.clone();
    let entries = entries
        .into_iter()
        .map(|(selection_id, label)| WheelEntry::new(selection_id, label))
        .collect();
    show_entries(entries, center_pos, &ui_lang)
}

pub fn dismiss_wheel() {
    let request_id = {
        let _protocol = WHEEL_PROTOCOL_LOCK.lock().unwrap();
        let request_id = WHEEL_REQUEST_ID.load(Ordering::SeqCst);
        let _ = WHEEL_RESULT.compare_exchange(-1, -2, Ordering::SeqCst, Ordering::SeqCst);
        WHEEL_ACTIVE.store(false, Ordering::SeqCst);
        request_id
    };
    unsafe {
        let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
        let wheel_hwnd = windows::Win32::Foundation::HWND(hwnd_val as *mut _);
        if !wheel_hwnd.is_invalid() {
            let _ = PostMessageW(
                Some(wheel_hwnd),
                super::state::WM_APP_HIDE,
                WPARAM(request_id as usize),
                LPARAM(0),
            );
        }
    }
}

pub fn is_wheel_active() -> bool {
    WHEEL_ACTIVE.load(Ordering::SeqCst)
}

pub(crate) fn should_show_preset(
    preset: &Preset,
    filter_type: &str,
    filter_mode: Option<&str>,
) -> bool {
    if preset.is_master || preset.is_upcoming || preset.preset_type != filter_type {
        return false;
    }

    if let Some(mode) = filter_mode {
        match filter_type {
            "text" => preset.text_input_mode == mode,
            "audio" => preset.audio_source == mode,
            _ => true,
        }
    } else {
        true
    }
}

fn show_entries(entries: Vec<WheelEntry>, center_pos: POINT, ui_lang: &str) -> Option<usize> {
    if entries.is_empty() {
        return None;
    }
    let _request = WheelRequest::try_acquire()?;
    if !ensure_wheel_ready() {
        return None;
    }

    unsafe {
        let request_id = {
            let _protocol = WHEEL_PROTOCOL_LOCK.lock().unwrap();
            WHEEL_RESULT.store(-1, Ordering::SeqCst);
            let id = WHEEL_REQUEST_ID
                .fetch_add(1, Ordering::SeqCst)
                .wrapping_add(1);
            WHEEL_ACTIVE.store(true, Ordering::SeqCst);
            WHEEL_LIVENESS.lock().unwrap().begin(id, Instant::now());
            id
        };

        let area = crate::overlay::display_placement::work_area_at_point(center_pos);
        let rect = area.fit(
            center_pos.x - WHEEL_WIDTH / 2,
            center_pos.y - WHEEL_HEIGHT / 2,
            WHEEL_WIDTH,
            WHEEL_HEIGHT,
        );

        *PENDING_ITEMS_HTML.lock().unwrap() = generate_items_html(&entries);
        *PENDING_DISMISS_LABEL.lock().unwrap() = dismiss_label(ui_lang).to_string();
        *PENDING_CSS.lock().unwrap() = generate_css(crate::overlay::is_dark_mode());
        *PENDING_POS.lock().unwrap() = (rect.left, rect.top);
        *PENDING_SIZE.lock().unwrap() = (rect.right - rect.left, rect.bottom - rect.top);

        let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
        let wheel_hwnd = windows::Win32::Foundation::HWND(hwnd_val as *mut _);
        if wheel_hwnd.is_invalid()
            || !IsWindow(Some(wheel_hwnd)).as_bool()
            || PostMessageW(
                Some(wheel_hwnd),
                super::state::WM_APP_SHOW,
                WPARAM(request_id as usize),
                LPARAM(0),
            )
            .is_err()
        {
            return None;
        }

        let started = Instant::now();
        wait_for_selection(
            || WheelWaitState {
                result: WHEEL_RESULT.load(Ordering::SeqCst),
                window_alive: WHEEL_HWND.load(Ordering::SeqCst) == hwnd_val
                    && IS_WARMED_UP.load(Ordering::SeqCst)
                    && IsWindow(Some(wheel_hwnd)).as_bool(),
                page_ready: WHEEL_READY_ID.load(Ordering::SeqCst) == request_id,
                elapsed: started.elapsed(),
                heartbeat_alive: WHEEL_LIVENESS
                    .lock()
                    .unwrap()
                    .is_alive(request_id, Instant::now()),
            },
            || {
                dismiss_wheel();
                if WHEEL_HWND.load(Ordering::SeqCst) == hwnd_val {
                    let _ = PostMessageW(Some(wheel_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            },
            pump_caller_messages,
            || std::thread::sleep(Duration::from_millis(5)),
        )
    }
}

fn ensure_wheel_ready() -> bool {
    let capability = crate::runtime_support::require_webview2(
        crate::runtime_support::WebViewFeature::PresetWheel,
    );
    if !capability.is_supported() {
        crate::runtime_support::notify_capability_issue(&capability);
        return false;
    }

    if wheel_ready() {
        return true;
    }

    warmup();

    let ui_lang = APP.lock().unwrap().config.ui_language.clone();
    let locale = crate::gui::locale::LocaleText::get(&ui_lang);
    crate::overlay::auto_copy_badge::show_notification(locale.shell.preset_wheel_loading);

    for _ in 0..500 {
        if !pump_caller_messages() {
            return false;
        }

        std::thread::sleep(Duration::from_millis(10));
        if wheel_ready() {
            return true;
        }
        if !IS_WARMING_UP.load(Ordering::SeqCst) {
            return false;
        }
    }

    false
}

fn wheel_ready() -> bool {
    let hwnd = windows::Win32::Foundation::HWND(WHEEL_HWND.load(Ordering::SeqCst) as *mut _);
    IS_WARMED_UP.load(Ordering::SeqCst)
        && !hwnd.is_invalid()
        && unsafe { IsWindow(Some(hwnd)).as_bool() }
}

fn pump_caller_messages() -> bool {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                PostQuitMessage(msg.wParam.0 as i32);
                return false;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    true
}

fn dismiss_label(ui_lang: &str) -> &'static str {
    crate::gui::locale::LocaleText::get(ui_lang)
        .overlay
        .preset_wheel_cancel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_pump_preserves_quit_and_exit_code_for_outer_loop() {
        unsafe {
            PostQuitMessage(73);
            assert!(!pump_caller_messages());
            let mut message = MSG::default();
            assert!(PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool());
            assert_eq!(message.message, WM_QUIT);
            assert_eq!(message.wParam.0, 73);
        }
    }
}
