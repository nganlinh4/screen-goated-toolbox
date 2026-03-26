// --- PANEL ACTIONS ---
// Contains trigger_preset, activate_continuous_from_panel, save_bubble_position,
// resize_panel_height, and update_panel_content.

use super::html::escape_js;
use super::state::*;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetSystemMetrics, GetWindowRect, PostMessageW, SetForegroundWindow, SetWindowPos,
    SM_CXSCREEN, SWP_NOACTIVATE, SWP_NOCOPYBITS, SWP_NOZORDER, WM_HOTKEY,
};
use windows::core::w;
use wry::Rect;

pub(super) fn trigger_preset(preset_idx: usize) {
    unsafe {
        // CRITICAL: Restore focus to the original foreground window before triggering.
        // This ensures that text-select presets can send Ctrl+C to the correct window
        // (the one that had text selected before the user clicked on the bubble panel).
        let saved_fg = LAST_FOREGROUND_HWND.load(Ordering::SeqCst);
        if saved_fg != 0 {
            let fg_hwnd = HWND(saved_fg as *mut std::ffi::c_void);
            if !fg_hwnd.is_invalid() {
                let _ = SetForegroundWindow(fg_hwnd);
                let _ = SetFocus(Some(fg_hwnd));
                // Small delay to allow focus to settle before triggering the preset
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        }

        let class = w!("HotkeyListenerClass");
        let title = w!("Listener");
        let hwnd = FindWindowW(class, title).unwrap_or_default();

        if !hwnd.is_invalid() {
            let hotkey_id = (preset_idx as i32 * 1000) + 1;
            let _ = PostMessageW(Some(hwnd), WM_HOTKEY, WPARAM(hotkey_id as usize), LPARAM(0));
        }
    }
}

pub(super) fn activate_continuous_from_panel(preset_idx: usize) {
    let (p_type, p_id, is_master) = {
        if let Ok(app) = APP.lock() {
            if let Some(p) = app.config.presets.get(preset_idx) {
                (p.preset_type.clone(), p.id.clone(), p.is_master)
            } else {
                return;
            }
        } else {
            return;
        }
    };

    if !crate::overlay::continuous_mode::supports_continuous_mode(&p_type) || is_master {
        return;
    }

    // Use "Bubble" as the hotkey name for panel-triggered continuous mode
    let hotkey_name = "Bubble".to_string();

    if p_type == "image" {
        // IMAGE CONTINUOUS MODE: Directly enter non-blocking image continuous mode
        crate::overlay::image_continuous_mode::enter(
            preset_idx,
            hotkey_name.clone(),
            (preset_idx as i32 * 1000) + 1,
        );
    } else if p_type == "text" {
        // TEXT CONTINUOUS MODE: Show badge and activate continuous mode

        // 1. Activate continuous mode FIRST
        crate::overlay::continuous_mode::activate(preset_idx, hotkey_name.clone());

        // 2. Show the badge with continuous mode text
        crate::overlay::text_selection::show_text_selection_tag(preset_idx);

        // 3. Update badge to show continuous mode suffix
        crate::overlay::text_selection::update_badge_for_continuous_mode();

        // 4. Show activation notification
        crate::overlay::continuous_mode::show_activation_notification(&p_id, &hotkey_name);
    }
}

pub fn save_bubble_position() {
    let bubble_val = BUBBLE_HWND.load(Ordering::SeqCst);
    if bubble_val == 0 {
        return;
    }

    unsafe {
        let bubble_hwnd = HWND(bubble_val as *mut std::ffi::c_void);
        let mut rect = RECT::default();
        let _ = GetWindowRect(bubble_hwnd, &mut rect);

        if let Ok(mut app) = APP.lock() {
            app.config.favorite_bubble_position = Some((rect.left, rect.top));
            crate::config::save_config(&app.config);
        }
    }
}

pub(super) fn resize_panel_height(content_height: i32) {
    let panel_val = PANEL_HWND.load(Ordering::SeqCst);
    if panel_val == 0 {
        return;
    }

    unsafe {
        let panel_hwnd = HWND(panel_val as *mut std::ffi::c_void);

        // Get DPI to scale the CSS pixels (content_height) to Physical pixels
        let dpi = GetDpiForWindow(panel_hwnd);
        let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };

        // Small buffer for DPI rounding
        let new_height_pixels = (content_height as f32 * scale).ceil() as i32 + 16;

        let mut panel_rect = RECT::default();
        let _ = GetWindowRect(panel_hwnd, &mut panel_rect);
        let current_width = panel_rect.right - panel_rect.left;
        let current_height = panel_rect.bottom - panel_rect.top;

        // Only resize if significantly different to avoid jitter loops
        if (current_height - new_height_pixels).abs() < 4 {
            return;
        }

        let bubble_val = BUBBLE_HWND.load(Ordering::SeqCst);
        let bubble_hwnd = if bubble_val != 0 {
            HWND(bubble_val as *mut std::ffi::c_void)
        } else {
            return;
        };

        let mut bubble_rect = RECT::default();
        let _ = GetWindowRect(bubble_hwnd, &mut bubble_rect);

        // Recalculate Y position to keep centered on bubble
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let bubble_size = BUBBLE_SIZE.load(Ordering::SeqCst);
        let (_panel_x, panel_y) = if bubble_rect.left > screen_w / 2 {
            (
                bubble_rect.left - current_width - 4,
                bubble_rect.top - new_height_pixels / 2 + bubble_size / 2,
            )
        } else {
            (
                bubble_rect.right + 4,
                bubble_rect.top - new_height_pixels / 2 + bubble_size / 2,
            )
        };

        // Clamp Y
        let actual_panel_y = panel_y.max(10);

        let _ = SetWindowPos(
            panel_hwnd,
            None,
            panel_rect.left, // Keep X
            actual_panel_y,
            current_width,
            new_height_pixels,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS,
        );

        // Update WebView bounds
        PANEL_WEBVIEW.with(|wv| {
            if let Some(webview) = wv.borrow().as_ref() {
                let _ = webview.set_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        current_width as u32,
                        new_height_pixels as u32,
                    )),
                });
            }
        });
    }
}

pub(super) fn update_panel_content(html: &str, cols: usize, keep_open_label: &str) {
    PANEL_WEBVIEW.with(|wv| {
        if let Some(webview) = wv.borrow().as_ref() {
            let escaped = escape_js(html);
            let escaped_label = escape_js(keep_open_label);
            let script = format!(
                "document.querySelector('.list').style.columnCount = '{}'; document.querySelector('.list').innerHTML = \"{}\"; document.getElementById('keepOpenLabel').textContent = \"{}\"; if(window.fitText) window.fitText();",
                cols, escaped, escaped_label
            );
            let _ = webview.evaluate_script(&script);
        }
    });
}
