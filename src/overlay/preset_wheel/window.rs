use super::html::generate_items_html;
use super::runtime::internal_create_window_loop;
use super::state::{
    IS_WARMED_UP, IS_WARMING_UP, PENDING_CSS, PENDING_DISMISS_LABEL, PENDING_ITEMS_HTML,
    PENDING_POS, WHEEL_ACTIVE, WHEEL_HEIGHT, WHEEL_HWND, WHEEL_RESULT, WHEEL_WIDTH, WheelEntry,
};
use super::styles::generate_css;
use crate::APP;
use crate::config::Preset;
use crate::gui::settings_ui::get_localized_preset_name;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetSystemMetrics, MSG, PM_REMOVE, PeekMessageW, PostMessageW,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, TranslateMessage,
};

#[derive(Clone, Debug)]
pub struct WheelOption {
    pub id: usize,
    pub label: String,
}

impl WheelOption {
    pub fn new(id: usize, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
        }
    }
}

pub fn warmup() {
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
    let (presets, ui_lang) = {
        let app = APP.lock().unwrap();
        (app.config.presets.clone(), app.config.ui_language.clone())
    };

    let entries: Vec<WheelEntry> = presets
        .iter()
        .enumerate()
        .filter(|(_, preset)| should_show_preset(preset, filter_type, filter_mode))
        .map(|(idx, preset)| WheelEntry::new(idx, get_localized_preset_name(&preset.id, &ui_lang)))
        .collect();

    show_entries(entries, center_pos, &ui_lang)
}

pub fn show_option_wheel(options: &[WheelOption], center_pos: POINT) -> Option<usize> {
    let ui_lang = APP.lock().unwrap().config.ui_language.clone();
    let entries: Vec<WheelEntry> = options
        .iter()
        .map(|option| WheelEntry::new(option.id, option.label.clone()))
        .collect();

    show_entries(entries, center_pos, &ui_lang)
}

pub fn dismiss_wheel() {
    unsafe {
        let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
        let wheel_hwnd = windows::Win32::Foundation::HWND(hwnd_val as *mut _);
        if !wheel_hwnd.is_invalid() {
            let _ = PostMessageW(
                Some(wheel_hwnd),
                super::state::WM_APP_HIDE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
    WHEEL_RESULT.store(-2, Ordering::SeqCst);
    WHEEL_ACTIVE.store(false, Ordering::SeqCst);
}

pub fn is_wheel_active() -> bool {
    WHEEL_ACTIVE.load(Ordering::SeqCst)
}

fn should_show_preset(preset: &Preset, filter_type: &str, filter_mode: Option<&str>) -> bool {
    if preset.is_master || preset.is_upcoming || preset.preset_type != filter_type {
        return false;
    }

    if filter_type == "audio" && preset.audio_processing_mode == "realtime" {
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
    if entries.is_empty() || !ensure_wheel_ready() {
        return None;
    }

    unsafe {
        WHEEL_RESULT.store(-1, Ordering::SeqCst);
        WHEEL_ACTIVE.store(true, Ordering::SeqCst);

        let screen_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let screen_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let screen_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let screen_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let win_x = (center_pos.x - WHEEL_WIDTH / 2)
            .max(screen_x)
            .min(screen_x + screen_w - WHEEL_WIDTH);
        let win_y = (center_pos.y - WHEEL_HEIGHT / 2)
            .max(screen_y)
            .min(screen_y + screen_h - WHEEL_HEIGHT);

        *PENDING_ITEMS_HTML.lock().unwrap() = generate_items_html(&entries);
        *PENDING_DISMISS_LABEL.lock().unwrap() = dismiss_label(ui_lang).to_string();
        *PENDING_CSS.lock().unwrap() = generate_css(crate::overlay::is_dark_mode());
        *PENDING_POS.lock().unwrap() = (win_x, win_y);

        let hwnd_val = WHEEL_HWND.load(Ordering::SeqCst);
        let wheel_hwnd = windows::Win32::Foundation::HWND(hwnd_val as *mut _);
        if !wheel_hwnd.is_invalid() {
            let _ = PostMessageW(
                Some(wheel_hwnd),
                super::state::WM_APP_SHOW,
                WPARAM(0),
                LPARAM(0),
            );
        }

        let mut msg = MSG::default();
        loop {
            let res = WHEEL_RESULT.load(Ordering::SeqCst);
            if res != -1 {
                WHEEL_ACTIVE.store(false, Ordering::SeqCst);
                return (res >= 0).then_some(res as usize);
            }

            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

fn ensure_wheel_ready() -> bool {
    if IS_WARMED_UP.load(Ordering::SeqCst) {
        return true;
    }

    warmup();

    let ui_lang = APP.lock().unwrap().config.ui_language.clone();
    let locale = crate::gui::locale::LocaleText::get(&ui_lang);
    crate::overlay::auto_copy_badge::show_notification(locale.preset_wheel_loading);

    for _ in 0..500 {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
        if IS_WARMED_UP.load(Ordering::SeqCst) {
            return true;
        }
    }

    false
}

fn dismiss_label(ui_lang: &str) -> &'static str {
    match ui_lang {
        "vi" => "HỦY",
        "ko" => "취소",
        _ => "CANCEL",
    }
}
