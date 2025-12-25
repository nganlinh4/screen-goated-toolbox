//! Utility functions and static variables for realtime audio

use std::sync::Mutex;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{WM_REALTIME_UPDATE, WM_TRANSLATION_UPDATE};

lazy_static::lazy_static! {
    pub static ref REALTIME_DISPLAY_TEXT: Mutex<String> = Mutex::new(String::new());
    pub static ref TRANSLATION_DISPLAY_TEXT: Mutex<String> = Mutex::new(String::new());
}

pub fn update_overlay_text(hwnd: HWND, text: &str) {
    if let Ok(mut display) = REALTIME_DISPLAY_TEXT.lock() {
        *display = text.to_string();
    }
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_REALTIME_UPDATE, WPARAM(0), LPARAM(0));
    }
}

pub fn update_translation_text(hwnd: HWND, text: &str) {
    if let Ok(mut display) = TRANSLATION_DISPLAY_TEXT.lock() {
        *display = text.to_string();
    }
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_TRANSLATION_UPDATE, WPARAM(0), LPARAM(0));
    }
}

pub fn refresh_transcription_window() {
    unsafe {
        let realtime_hwnd = crate::overlay::realtime_webview::REALTIME_HWND;
        if !realtime_hwnd.is_invalid() {
            let _ = PostMessageW(Some(realtime_hwnd), WM_REALTIME_UPDATE, WPARAM(0), LPARAM(0));
        }
    }
}
