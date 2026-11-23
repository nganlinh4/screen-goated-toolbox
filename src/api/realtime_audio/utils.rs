//! Utility functions and static variables for realtime audio

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{WM_REALTIME_UPDATE, WM_TRANSLATION_UPDATE};

/// Wakes the transcription card after the shared realtime state changed. The
/// card reads the text it shows from that state, so a transcript update never
/// copies the transcript to publish it.
pub fn notify_transcript_changed(hwnd: HWND) {
    if !hwnd.is_invalid() {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_REALTIME_UPDATE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Wakes the translation card after the shared realtime state changed.
pub fn notify_translation_changed(hwnd: HWND) {
    if !hwnd.is_invalid() {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_TRANSLATION_UPDATE, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn refresh_transcription_window() {
    unsafe {
        let realtime_hwnd = crate::overlay::realtime_webview::REALTIME_HWND;
        if !realtime_hwnd.is_invalid() {
            let _ = PostMessageW(
                Some(realtime_hwnd),
                WM_REALTIME_UPDATE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}
