// --- FAVORITES PANEL PUBLIC API ---
// Show, close, destroy, move, and refresh the favorites panel.
// Window/WebView creation is in panel_window.rs, actions in panel_actions.rs.

use super::html::{escape_js, generate_panel_css, get_favorite_presets_html};
use super::panel_actions;
use super::panel_window;
use super::render::update_bubble_visual;
use super::state::*;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, GetForegroundWindow, GetSystemMetrics, GetWindowRect, HWND_TOPMOST,
    PostMessageW, SetWindowPos, ShowWindow, SM_CXSCREEN, SW_HIDE, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_NOCOPYBITS, WM_APP,
};
use wry::Rect;

pub const WM_FORCE_SHOW_PANEL: u32 = WM_APP + 43;

// Re-export save_bubble_position for external callers
pub use super::panel_actions::save_bubble_position;

pub fn show_panel(bubble_hwnd: HWND) {
    if IS_EXPANDED.load(Ordering::SeqCst) {
        return;
    }

    // CRITICAL: Save the current foreground window BEFORE showing the panel.
    // The WebView will steal focus when clicked, but we need to restore focus
    // to the original window for text-select presets to work (they send Ctrl+C).
    unsafe {
        let fg = GetForegroundWindow();
        if !fg.is_invalid() {
            LAST_FOREGROUND_HWND.store(fg.0 as isize, Ordering::SeqCst);
        }
    }

    // Ensure window AND webview exist (webview creation is deferred to here to avoid focus steal)
    let just_created = ensure_panel_created(bubble_hwnd, true);

    let panel_val = PANEL_HWND.load(Ordering::SeqCst);
    if panel_val == 0 {
        return;
    }

    unsafe {
        let panel_hwnd = HWND(panel_val as *mut std::ffi::c_void);

        // CRITICAL: Set state to true BEFORE refreshing or showing,
        // so that any incoming 'close_now' IPC messages (from a previous close)
        // will see that we are now EXPANDED and ignore the hide command.
        IS_EXPANDED.store(true, Ordering::SeqCst);

        if just_created {
            // If just created, it might take a moment for WebView2 to be ready for scripts.
            let _ = ShowWindow(panel_hwnd, SW_SHOWNOACTIVATE);

            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(600));
                let panel_val = PANEL_HWND.load(Ordering::SeqCst);
                if panel_val != 0 {
                    let panel_hwnd = HWND(panel_val as *mut std::ffi::c_void);
                    let _ = PostMessageW(
                        Some(panel_hwnd),
                        panel_window::WM_REFRESH_PANEL,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            });
        } else if let Ok(app) = APP.lock() {
            let is_dark = match app.config.theme_mode {
                crate::config::ThemeMode::Dark => true,
                crate::config::ThemeMode::Light => false,
                crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
            };

            refresh_panel_layout_and_content(
                bubble_hwnd,
                panel_hwnd,
                &app.config.presets,
                &app.config.ui_language,
                is_dark,
            );
        }

        update_bubble_visual(bubble_hwnd);
    }
}

pub fn update_favorites_panel() {
    // Send a message to the Bubble Window (dedicated thread) to handle the update.
    let bubble_val = BUBBLE_HWND.load(Ordering::SeqCst);
    if bubble_val != 0 {
        let bubble_hwnd = HWND(bubble_val as *mut std::ffi::c_void);
        unsafe {
            let _ = PostMessageW(Some(bubble_hwnd), WM_FORCE_SHOW_PANEL, WPARAM(0), LPARAM(0));
        }
    }
}

/// Ensure the panel window exists.
/// If `with_webview` is true, also create the WebView2 (deferred to avoid focus stealing during warmup).
pub fn ensure_panel_created(bubble_hwnd: HWND, with_webview: bool) -> bool {
    let mut created = false;
    let panel_exists = PANEL_HWND.load(Ordering::SeqCst) != 0;

    if !panel_exists {
        panel_window::create_panel_window_internal(bubble_hwnd);
    }

    // Create WebView2 only when requested AND it doesn't exist yet
    if with_webview {
        let has_webview = PANEL_WEBVIEW.with(|wv| wv.borrow().is_some());
        if !has_webview {
            let panel_val = PANEL_HWND.load(Ordering::SeqCst);
            if panel_val != 0 {
                let panel_hwnd = HWND(panel_val as *mut std::ffi::c_void);
                panel_window::create_panel_webview(panel_hwnd);
                created = true;
            }
        }
    }
    created
}

// Triggers the animation-based close
pub fn close_panel() {
    // Set expanded to false immediately to allow re-opening
    if !IS_EXPANDED.swap(false, Ordering::SeqCst) {
        return;
    }

    let webview_exists = PANEL_WEBVIEW.with(|wv| {
        if let Some(webview) = wv.borrow().as_ref() {
            let _ = webview.evaluate_script("if(window.closePanel) window.closePanel();");
            true
        } else {
            false
        }
    });

    if !webview_exists {
        close_panel_internal();
    }
}

// Actually hides the window
pub(super) fn close_panel_internal() {
    // CRITICAL: If IS_EXPANDED was set to true (e.g. by a quick click to re-open),
    // do NOT hide the window.
    if IS_EXPANDED.load(Ordering::SeqCst) {
        return;
    }

    let panel_val = PANEL_HWND.load(Ordering::SeqCst);
    if panel_val != 0 {
        unsafe {
            let panel_hwnd = HWND(panel_val as *mut std::ffi::c_void);
            let _ = ShowWindow(panel_hwnd, SW_HIDE);
        }
    }

    // Update bubble visual
    let bubble_val = BUBBLE_HWND.load(Ordering::SeqCst);
    if bubble_val != 0 {
        let bubble_hwnd = HWND(bubble_val as *mut std::ffi::c_void);
        update_bubble_visual(bubble_hwnd);
    }

    // Save position
    panel_actions::save_bubble_position();
}

// Actually destroys the panel (cleanup)
pub fn destroy_panel() {
    let panel_val = PANEL_HWND.swap(0, Ordering::SeqCst);
    if panel_val != 0 {
        PANEL_WEBVIEW.with(|wv| {
            *wv.borrow_mut() = None;
        });

        unsafe {
            let panel_hwnd = HWND(panel_val as *mut std::ffi::c_void);
            let _ = DestroyWindow(panel_hwnd);
        }
    }
}

/// Ensures the bubble window stays above the panel window in Z-order.
pub(super) fn ensure_bubble_on_top() {
    let bubble_val = BUBBLE_HWND.load(Ordering::SeqCst);
    if bubble_val != 0 {
        unsafe {
            let bubble_hwnd = HWND(bubble_val as *mut std::ffi::c_void);
            let _ = SetWindowPos(
                bubble_hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

pub fn move_panel_to_bubble(bubble_x: i32, bubble_y: i32) {
    let panel_val = PANEL_HWND.load(Ordering::SeqCst);
    if panel_val == 0 {
        return;
    }

    unsafe {
        let panel_hwnd = HWND(panel_val as *mut std::ffi::c_void);
        let mut panel_rect = RECT::default();
        let _ = GetWindowRect(panel_hwnd, &mut panel_rect);
        let panel_w = panel_rect.right - panel_rect.left;
        let panel_h = panel_rect.bottom - panel_rect.top;

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let bubble_size = BUBBLE_SIZE.load(Ordering::SeqCst);
        let bubble_overlap = bubble_size + 4;

        let dpi = GetDpiForWindow(panel_hwnd);
        let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };

        // Panel extends behind bubble - calculate position accordingly
        let (panel_x, panel_y, side) = if bubble_x > screen_w / 2 {
            // Bubble on right - panel content is to the left, overlap extends right
            (
                bubble_x - (panel_w - bubble_overlap) - 4,
                bubble_y - panel_h / 2 + bubble_size / 2,
                "right",
            )
        } else {
            // Bubble on left - panel starts at bubble's left edge
            (bubble_x, bubble_y - panel_h / 2 + bubble_size / 2, "left")
        };

        let actual_panel_y = panel_y.max(10);

        let _ = SetWindowPos(
            panel_hwnd,
            None,
            panel_x,
            actual_panel_y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );

        // Update bubble center in JS for correct collapse direction
        let panel_w_css = (panel_w as f32 / scale) as i32;
        let bx = if side == "left" {
            bubble_size / 2
        } else {
            panel_w_css + (bubble_size / 2) + 4
        };
        let by = (bubble_y + bubble_size / 2) - actual_panel_y;

        PANEL_WEBVIEW.with(|wv| {
            if let Some(webview) = wv.borrow().as_ref() {
                let script = format!(
                    "if(window.updateBubbleCenter) window.updateBubbleCenter({}, {});",
                    bx, by
                );
                let _ = webview.evaluate_script(&script);
            }
        });

        // Ensure bubble stays above panel
        let bubble_val = BUBBLE_HWND.load(Ordering::SeqCst);
        if bubble_val != 0 {
            let bubble_hwnd = HWND(bubble_val as *mut std::ffi::c_void);
            let _ = SetWindowPos(
                bubble_hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

pub(super) unsafe fn refresh_panel_layout_and_content(
    bubble_hwnd: HWND,
    panel_hwnd: HWND,
    presets: &[crate::config::Preset],
    lang: &str,
    is_dark: bool,
) {
    unsafe {
        let mut bubble_rect = RECT::default();
        let _ = GetWindowRect(bubble_hwnd, &mut bubble_rect);

        let height_per_item = 48;

        let favs: Vec<_> = presets
            .iter()
            .filter(|p| p.is_favorite && !p.is_upcoming)
            .collect();

        let fav_count = favs.len();
        let num_cols = if fav_count > 15 {
            fav_count.div_ceil(15)
        } else {
            1
        };

        let items_per_col = if fav_count > 0 {
            fav_count.div_ceil(num_cols)
        } else {
            0
        };

        // Buffer for padding (no bounce overshoot with smooth easing)
        let buffer_x = 40;
        let buffer_y = 60;

        let panel_width = if fav_count == 0 {
            (PANEL_WIDTH * 2).max(320)
        } else {
            (PANEL_WIDTH as usize * num_cols) as i32 + buffer_x
        };

        // Height for the keep-open toggle row
        let keep_open_row_height = 40;

        let panel_height = if fav_count == 0 {
            80 + buffer_y + keep_open_row_height
        } else {
            (items_per_col as i32 * height_per_item) + 24 + buffer_y + keep_open_row_height + 16
        };
        let panel_height = panel_height.max(50);

        // Get DPI scale
        let dpi = GetDpiForWindow(panel_hwnd);
        let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };

        let panel_width_physical = (panel_width as f32 * scale).ceil() as i32;
        let panel_height_physical = (panel_height as f32 * scale).ceil() as i32;

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let bubble_size = BUBBLE_SIZE.load(Ordering::SeqCst);

        // Extend panel to overlap behind bubble for seamless bloom/collapse animations
        let bubble_overlap = bubble_size + 4;
        let panel_width_with_overlap = panel_width_physical + bubble_overlap;

        let (panel_x, panel_y, side) = if bubble_rect.left > screen_w / 2 {
            (
                bubble_rect.left - panel_width_physical - 4,
                bubble_rect.top - panel_height_physical / 2 + bubble_size / 2,
                "right",
            )
        } else {
            (
                bubble_rect.left,
                bubble_rect.top - panel_height_physical / 2 + bubble_size / 2,
                "left",
            )
        };

        let actual_panel_y = panel_y.max(10);

        let _ = SetWindowPos(
            panel_hwnd,
            None,
            panel_x,
            actual_panel_y,
            panel_width_with_overlap,
            panel_height_physical,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS,
        );

        // Explicitly show the window
        let _ = ShowWindow(panel_hwnd, SW_SHOWNOACTIVATE);

        // CRITICAL: Ensure bubble stays above the panel window
        let bubble_val = BUBBLE_HWND.load(Ordering::SeqCst);
        if bubble_val != 0 {
            let bubble_hwnd_local = HWND(bubble_val as *mut std::ffi::c_void);
            let _ = SetWindowPos(
                bubble_hwnd_local,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }

        PANEL_WEBVIEW.with(|wv| {
            if let Some(webview) = wv.borrow().as_ref() {
                let _ = webview.set_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        panel_width_with_overlap as u32,
                        panel_height_physical as u32,
                    )),
                });
            }
        });

        // Check if theme changed and inject new CSS if needed
        let last_dark = LAST_THEME_IS_DARK.load(Ordering::SeqCst);
        if last_dark != is_dark {
            let new_css = generate_panel_css(is_dark);
            let escaped_css = escape_js(&new_css);
            PANEL_WEBVIEW.with(|wv| {
                if let Some(webview) = wv.borrow().as_ref() {
                    let script = format!(
                        "document.querySelector('style').innerHTML = \"{}\";",
                        escaped_css
                    );
                    let _ = webview.evaluate_script(&script);
                }
            });
            LAST_THEME_IS_DARK.store(is_dark, Ordering::SeqCst);
        }

        let favorites_html = get_favorite_presets_html(presets, lang, is_dark);
        let keep_open_label = crate::gui::locale::LocaleText::get(lang).favorites_keep_open;
        panel_actions::update_panel_content(&favorites_html, num_cols, keep_open_label);

        let bx = if side == "left" {
            bubble_size / 2
        } else {
            (panel_width_physical / scale as i32) + (bubble_size / 2) + 4
        };
        let by = (bubble_rect.top + bubble_size / 2) - actual_panel_y;

        PANEL_WEBVIEW.with(|wv| {
            if let Some(webview) = wv.borrow().as_ref() {
                let script = format!(
                    "if(window.setSide) window.setSide('{}', {}); if(window.animateIn) window.animateIn({}, {});",
                    side, bubble_overlap, bx, by
                );
                let _ = webview.evaluate_script(&script);
            }
        });
    }
}
