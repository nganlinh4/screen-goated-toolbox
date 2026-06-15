//! Utility functions and static variables for realtime audio

use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{WM_REALTIME_UPDATE, WM_TRANSLATION_UPDATE};

pub static REALTIME_DISPLAY_TEXT: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
pub static TRANSLATION_DISPLAY_TEXT: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));

pub fn update_overlay_text(hwnd: HWND, text: &str) {
    if let Ok(mut display) = REALTIME_DISPLAY_TEXT.lock() {
        *display = text.to_string();
    }
    if hwnd.is_invalid() {
        request_realtime_egui_repaint();
    } else {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_REALTIME_UPDATE, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn update_translation_text(hwnd: HWND, text: &str) {
    if let Ok(mut display) = TRANSLATION_DISPLAY_TEXT.lock() {
        *display = text.to_string();
    }
    if hwnd.is_invalid() {
        request_realtime_egui_repaint();
    } else {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_TRANSLATION_UPDATE, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn request_realtime_egui_repaint() {
    use std::sync::atomic::Ordering;

    if !crate::overlay::realtime_egui::MINIMAL_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    if let Ok(guard) = crate::gui::GUI_CONTEXT.lock()
        && let Some(ctx) = guard.as_ref()
    {
        ctx.request_repaint();
    }
}

/// Join two transcript segments with a smart space (respects existing whitespace).
pub fn join_transcript_segments(left: &str, right: &str) -> String {
    let left = sanitize_transcript_segment(left);
    let right = sanitize_transcript_segment(right);
    match (left.is_empty(), right.is_empty()) {
        (true, true) => String::new(),
        (true, false) => right.trim_start().to_string(),
        (false, true) => left,
        (false, false) => {
            let left_has_space = left.chars().last().is_some_and(char::is_whitespace);
            let right_has_space = right.chars().next().is_some_and(char::is_whitespace);
            if left_has_space || right_has_space {
                format!("{left}{right}")
            } else {
                format!("{left} {right}")
            }
        }
    }
}

/// Append a segment to history, joining with smart spacing.
pub fn append_history_segment(history: &mut String, segment: &str) {
    let segment = sanitize_transcript_segment(segment);
    if segment.is_empty() {
        return;
    }
    if history.is_empty() {
        history.push_str(segment.trim_start());
    } else {
        let combined = join_transcript_segments(history, &segment);
        history.clear();
        history.push_str(&combined);
    }
}

fn sanitize_transcript_segment(segment: &str) -> String {
    segment.replace(['\n', '\t'], " ")
}

/// Split draft at the last sentence boundary (.?!) that has text after it.
/// Returns `(committed_part, remaining_draft)` or `None` if no clean boundary.
pub fn split_at_sentence_boundary(text: &str) -> Option<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut last_boundary: Option<usize> = None;
    let mut byte_pos = 0usize;
    for (i, &ch) in chars.iter().enumerate() {
        let ch_len = ch.len_utf8();
        if ch == '.' || ch == '?' || ch == '!' {
            let rest = &text[byte_pos + ch_len..];
            let rest_trimmed = rest.trim_start();
            if !rest_trimmed.is_empty()
                && rest_trimmed
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c.is_numeric())
            {
                last_boundary = Some(byte_pos + ch_len);
            }
        }
        byte_pos += ch_len;
        let _ = i;
    }
    last_boundary.map(|pos| {
        let before = text[..pos].trim_end().to_string();
        let after = text[pos..].trim_start().to_string();
        (before, after)
    })
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
        } else {
            request_realtime_egui_repaint();
        }
    }
}
