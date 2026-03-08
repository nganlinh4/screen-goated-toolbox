// Tray Popup - Custom non-blocking popup window for tray icon menu
// Replaces native Windows tray context menu to avoid blocking the main UI thread

use crate::APP;
use std::cell::RefCell;
use std::sync::{
    Once,
    atomic::{AtomicIsize, Ordering},
};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;
use wry::{Rect, WebContext, WebView, WebViewBuilder};

static REGISTER_POPUP_CLASS: Once = Once::new();
static POPUP_HWND: AtomicIsize = AtomicIsize::new(0);
static IGNORE_FOCUS_LOSS_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// Warmup flag - tracks if the window has been created and is ready for instant display
static IS_WARMED_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static IS_WARMING_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WARMUP_START_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
// Flag to track if WebView has permanently failed to initialize
static WEBVIEW_INIT_FAILED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

// Custom window messages
const WM_APP_SHOW: u32 = WM_APP + 1;

thread_local! {
    static POPUP_WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
    // Shared WebContext for this thread using common data directory
    static POPUP_WEB_CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
}

const BASE_POPUP_WIDTH: i32 = 240;
const BASE_POPUP_HEIGHT: i32 = 186; // Base height at 100% scaling (96 DPI) - includes restore row
const POPUP_SURFACE_INSET: i32 = 6;
const RESTORE_FLYOUT_WIDTH: i32 = 236;
const RESTORE_FLYOUT_GAP: i32 = 10;
const RESTORE_FLYOUT_OPTION_HEIGHT: i32 = 28;
const RESTORE_FLYOUT_VERTICAL_PADDING: i32 = 8;
const RESTORE_FLYOUT_TOP_INSET: i32 = 6;
const RESTORE_FLYOUT_PREFERRED_TOP: i32 = 100;

/// Get DPI-scaled dimension
fn get_scaled_dimension(base: i32) -> i32 {
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() };
    // Scale: 96 DPI = 100%, 120 DPI = 125%, 144 DPI = 150%, etc.
    // Using 93 instead of 96 provides a small buffer (~3%) to ensure content fits comfortably
    (base * dpi as i32) / 93
}

fn popup_window_dimensions() -> (i32, i32) {
    let inset = get_scaled_dimension(POPUP_SURFACE_INSET);
    let width = get_scaled_dimension(BASE_POPUP_WIDTH)
        + inset * 2
        + if crate::overlay::result::recent_restore_option_counts().is_empty() {
            0
        } else {
            get_scaled_dimension(RESTORE_FLYOUT_GAP + RESTORE_FLYOUT_WIDTH)
        };
    let height = get_scaled_dimension(BASE_POPUP_HEIGHT) + inset * 2;
    (width, height)
}

unsafe fn set_popup_bounds(hwnd: HWND, x: i32, y: i32) {
    let (popup_width, popup_height) = popup_window_dimensions();
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let popup_x = x.max(0).min((screen_w - popup_width).max(0));
    let popup_y = y.max(0).min((screen_h - popup_height).max(0));

    POPUP_WEBVIEW.with(|cell| {
        if let Some(webview) = cell.borrow().as_ref() {
            let _ = webview.set_bounds(Rect {
                position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                    popup_width as u32,
                    popup_height as u32,
                )),
            });
        }
    });

    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            popup_x,
            popup_y,
            popup_width,
            popup_height,
            SWP_NOZORDER,
        );
    }
}

#[derive(serde::Serialize)]
struct PopupRestoreOption {
    batch_count: usize,
    label: String,
}

fn restore_flyout_height_logical(option_count: usize) -> i32 {
    if option_count == 0 {
        0
    } else {
        RESTORE_FLYOUT_VERTICAL_PADDING + option_count as i32 * RESTORE_FLYOUT_OPTION_HEIGHT
    }
}

fn restore_flyout_top_logical(option_count: usize) -> i32 {
    if option_count == 0 {
        return RESTORE_FLYOUT_TOP_INSET;
    }

    let flyout_height = restore_flyout_height_logical(option_count);
    let max_top = (BASE_POPUP_HEIGHT - flyout_height - RESTORE_FLYOUT_TOP_INSET)
        .max(RESTORE_FLYOUT_TOP_INSET);
    RESTORE_FLYOUT_PREFERRED_TOP.clamp(RESTORE_FLYOUT_TOP_INSET, max_top)
}

fn format_restore_option_label(ui_language: &str, overlay_count: usize) -> String {
    match ui_language {
        "vi" => format!("Khôi phục {overlay_count} overlay vừa đóng"),
        "ko" => format!("방금 닫은 오버레이 {overlay_count}개 복원"),
        _ => {
            let noun = if overlay_count == 1 {
                "overlay"
            } else {
                "overlays"
            };
            format!("Restore {overlay_count} recently closed {noun}")
        }
    }
}

fn get_restore_options(ui_language: &str) -> Vec<PopupRestoreOption> {
    crate::overlay::result::recent_restore_option_counts()
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(index, overlay_count)| PopupRestoreOption {
            batch_count: index + 1,
            label: format_restore_option_label(ui_language, overlay_count),
        })
        .collect()
}

fn render_restore_options_html(options: &[PopupRestoreOption]) -> String {
    options
        .iter()
        .map(|option| {
            format!(
                r#"<div class="restore-option" onclick="action('restore_recent:{batch_count}')"><div class="restore-option-label">{label}</div></div>"#,
                batch_count = option.batch_count,
                label = option.label,
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

// HWND wrapper for wry
struct HwndWrapper(HWND);
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}
impl raw_window_handle::HasWindowHandle for HwndWrapper {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let raw = raw_window_handle::Win32WindowHandle::new(
            std::num::NonZeroIsize::new(self.0.0 as isize).expect("HWND cannot be null"),
        );
        let handle = raw_window_handle::RawWindowHandle::Win32(raw);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(handle)) }
    }
}

/// Show the tray popup at cursor position
pub fn show_tray_popup() {
    unsafe {
        // Fallback to native menu if WebView failed completely
        if WEBVIEW_INIT_FAILED.load(Ordering::SeqCst) {
            show_native_context_menu();
            return;
        }

        // Check if warmed up and window exists
        if !IS_WARMED_UP.load(Ordering::SeqCst) {
            // Not ready yet - trigger warmup and show notification
            warmup_tray_popup();

            let ui_lang = APP.lock().unwrap().config.ui_language.clone();
            let locale = crate::gui::locale::LocaleText::get(&ui_lang);
            crate::overlay::auto_copy_badge::show_notification(locale.tray_popup_loading);

            // Spawn thread to wait for warmup completion and auto-show
            std::thread::spawn(move || {
                // Poll for 5 seconds (50 * 100ms)
                for _ in 0..50 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    // Check if ready
                    let ready = IS_WARMED_UP.load(Ordering::SeqCst)
                        && POPUP_HWND.load(Ordering::SeqCst) != 0;
                    if ready {
                        show_tray_popup();
                        return;
                    }
                }
            });
            return;
        }

        let hwnd_val = POPUP_HWND.load(Ordering::SeqCst);
        if hwnd_val == 0 {
            // Should be warmed up but handle missing? Retry warmup
            IS_WARMED_UP.store(false, Ordering::SeqCst);
            warmup_tray_popup();
            return;
        }

        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);

        // Check if window still valid logic...
        if !IsWindow(Some(hwnd)).as_bool() {
            // Window destroyed
            IS_WARMED_UP.store(false, Ordering::SeqCst);
            POPUP_HWND.store(0, Ordering::SeqCst);
            warmup_tray_popup();
            return;
        }

        // Check if already visible
        if IsWindowVisible(hwnd).as_bool() {
            hide_tray_popup();
            return;
        }

        // Post message to show
        let _ = PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0));
    }
}

/// Hide the tray popup (preserves window for reuse)
pub fn hide_tray_popup() {
    let hwnd_val = POPUP_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe {
            // Just hide - don't destroy. Preserves WebView state for instant redisplay.
            let _ = KillTimer(Some(hwnd), 888);
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// Warmup the tray popup - creates hidden window with WebView for instant display later
pub fn warmup_tray_popup() {
    // Check if dead stuck (timestamp check)
    unsafe {
        let start_time = WARMUP_START_TIME.load(Ordering::SeqCst);
        let now = windows::Win32::System::SystemInformation::GetTickCount64();
        if start_time > 0 && (now - start_time) > 10000 {
            // Stuck for > 10s - force reset
            IS_WARMED_UP.store(false, Ordering::SeqCst);
            IS_WARMING_UP.store(false, Ordering::SeqCst);
            POPUP_HWND.store(0, Ordering::SeqCst);
        }
    }

    // Only allow one warmup thread at a time
    if IS_WARMING_UP
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    // Update timestamp
    unsafe {
        WARMUP_START_TIME.store(
            windows::Win32::System::SystemInformation::GetTickCount64(),
            Ordering::SeqCst,
        );
    }

    std::thread::spawn(|| {
        create_popup_window();
    });
}

/// Check if the tray popup is currently visible
/// Used by warmup logic to defer WebView2 initialization until popup closes
pub fn is_popup_open() -> bool {
    let hwnd_val = POPUP_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        unsafe { IsWindowVisible(hwnd).as_bool() }
    } else {
        false
    }
}

fn generate_popup_html() -> String {
    use crate::config::ThemeMode;

    let mut ui_language = String::from("en");
    let (
        settings_text,
        bubble_text,
        stop_tts_text,
        restore_overlay_text,
        quit_text,
        bubble_checked,
        is_dark_mode,
    ) = if let Ok(app) = APP.lock() {
        ui_language = app.config.ui_language.clone();
        let (settings, bubble, stop_tts, restore_overlay, quit) =
            get_popup_labels(&app.config.ui_language);
        let checked = app.config.show_favorite_bubble;

        // Theme detection
        let is_dark = match app.config.theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
        };

        (
            settings,
            bubble,
            stop_tts,
            restore_overlay,
            quit,
            checked,
            is_dark,
        )
    } else {
        (
            "Settings",
            "Favorite Bubble",
            "Stop All TTS",
            "Restore Last Closed Overlay",
            "Quit",
            false,
            true,
        )
    };

    // Check if TTS has pending audio
    let has_tts_pending = crate::api::tts::TTS_MANAGER.has_pending_audio();

    // Define Colors based on theme
    let (bg_color, text_color, hover_color, border_color, separator_color) = if is_dark_mode {
        (
            "#2c2c2c",
            "#ffffff",
            "#3c3c3c",
            "#454545",
            "rgba(255,255,255,0.08)",
        )
    } else {
        (
            "#f9f9f9",
            "#1a1a1a",
            "#eaeaea",
            "#dcdcdc",
            "rgba(0,0,0,0.06)",
        )
    };

    let check_mark = if bubble_checked {
        r#"<svg class="check-icon" viewBox="0 0 16 16" fill="currentColor"><path d="M13.86 3.66a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.6 9.03a.75.75 0 1 1 1.06-1.06l2.42 2.42 6.72-6.72a.75.75 0 0 1 1.06 0z"/></svg>"#
    } else {
        ""
    };

    let active_class = if bubble_checked { "active" } else { "" };

    let stop_tts_disabled_class = if has_tts_pending { "" } else { "disabled" };
    let restore_overlay_disabled_class = if crate::overlay::result::can_restore_last_closed() {
        ""
    } else {
        "disabled"
    };
    let restore_options = get_restore_options(&ui_language);
    let restore_options_html = render_restore_options_html(&restore_options);
    let restore_flyout_top = restore_flyout_top_logical(restore_options.len());

    // Get font CSS to preload fonts into WebView2 cache (tray popup warms up first)
    let font_css = crate::overlay::html_components::font_manager::get_font_css();

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
{font_css}
:root {{
    --bg-color: {bg};
    --text-color: {text};
    --hover-bg: {hover};
    --border-color: {border};
    --separator-color: {separator};
}}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
html, body {{
    width: 100%;
    height: 100%;
    overflow: visible;
    background: transparent;
    font-family: 'Google Sans Flex', 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif;
    font-variation-settings: 'ROND' 100;
    user-select: none;
    color: var(--text-color);
}}

.container {{
    position: relative;
    width: 100%;
    height: 100%;
    padding: {popup_surface_inset}px;
    overflow: visible;
}}

.menu-panel {{
    display: flex;
    flex-direction: column;
    width: {base_popup_width}px;
    height: 186px;
    padding: 4px;
    background: var(--bg-color);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    overflow: visible;
}}

.menu-item {{
    display: flex;
    align-items: center;
    padding: 6px 10px;
    border-radius: 4px;
    cursor: default;
    font-size: 13px;
    margin-bottom: 2px;
    background: transparent;
    transition: background 0.1s ease;
    height: 32px;
}}

.menu-item:hover {{
    background: var(--hover-bg);
}}

.icon {{
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    margin-right: 12px;
    opacity: 0.8;
}}

.label {{
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    padding-bottom: 1px; /* Visual alignment */
}}

.check {{
    display: flex;
    align-items: center;
    justify-content: center;
    width: 0;
    flex: 0 0 0;
    margin-left: 0;
}}

.separator {{
    height: 1px;
    background: var(--separator-color);
    margin: 4px 10px;
}}

svg {{
    width: 16px;
    height: 16px;
}}


.bubble-item .label {{
    transition: font-variation-settings 0.4s cubic-bezier(0.33, 1, 0.68, 1);
    font-variation-settings: 'wght' 400, 'wdth' 100, 'ROND' 100;
}}
.bubble-item .check {{
    width: 16px;
    flex: 0 0 16px;
    margin-left: 8px;
}}
.bubble-item.active .label {{
    font-variation-settings: 'wght' 700, 'wdth' 96, 'ROND' 100;
    color: var(--text-color);
}}

.restore-item {{
    position: relative;
    margin-bottom: 0;
}}

.restore-item .label {{
    font-variation-settings: 'wght' 420, 'wdth' 78, 'ROND' 100;
}}

.restore-chevron {{
    display: flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    flex: 0 0 14px;
    margin-left: 8px;
    opacity: 0.64;
    transition: transform 0.12s ease, opacity 0.12s ease;
}}

.restore-chevron svg {{
    width: 14px;
    height: 14px;
}}

.restore-item.expanded {{
    background: var(--hover-bg);
}}

.restore-item.expanded .restore-chevron {{
    transform: translateX(1px);
    opacity: 0.92;
}}

.restore-flyout {{
    display: none;
    position: absolute;
    left: {restore_flyout_left}px;
    flex-direction: column;
    width: {restore_flyout_width}px;
    padding: 4px;
    background: var(--bg-color);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    box-shadow: 0 14px 28px rgba(0, 0, 0, 0.18);
    z-index: 5;
}}

.restore-flyout.visible {{
    display: flex;
}}

.restore-option {{
    display: flex;
    align-items: center;
    min-height: 28px;
    padding: 5px 12px;
    border-radius: 4px;
    cursor: default;
    transition: background 0.1s ease;
}}

.restore-option:hover {{
    background: var(--hover-bg);
}}

.restore-option-label {{
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
    font-variation-settings: 'wght' 420, 'wdth' 82, 'ROND' 100;
    padding-bottom: 1px;
}}

.menu-item.disabled {{
    opacity: 0.4;
    pointer-events: none;
}}

.restore-flyout.disabled {{
    opacity: 0.4;
    pointer-events: none;
}}
</style>
</head>
<body>
<div class="container">
    <div class="menu-panel" id="menu-panel">
    <div class="menu-item" onclick="action('settings')">
        <div class="icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.09a2 2 0 0 1-1-1.74v-.47a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.39a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"></path>
                <circle cx="12" cy="12" r="3"></circle>
            </svg>
        </div>
        <div class="label">{settings}</div>
        <div class="check"></div>
    </div>

    <div class="menu-item bubble-item {active_class}" data-state="{active_class}" onclick="action('bubble')">
        <div class="icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>
        </div>
        <div class="label">{bubble}</div>
        <div class="check" id="bubble-check-container">{check}</div>
    </div>

    <div class="menu-item {stop_tts_disabled}" id="stop-tts-item" onclick="action('stop_tts')">
        <div class="icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H2v6h4l5 4V5z"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>
        </div>
        <div class="label">{stop_tts}</div>
        <div class="check"></div>
    </div>

        <div class="menu-item restore-item {restore_overlay_disabled}" id="restore-overlay-item" onmouseenter="setRestoreMenuExpanded(true)" onmouseleave="setRestoreMenuExpanded(false)">
            <div class="icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7"/><polyline points="3 3 3 9 9 9"/></svg>
            </div>
            <div class="label">{restore_overlay}</div>
            <div class="restore-chevron" id="restore-chevron">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 6 15 12 9 18"/></svg>
            </div>
        </div>

    <div class="separator"></div>

    <div class="menu-item" onclick="action('quit')">
        <div class="icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>
        </div>
        <div class="label">{quit}</div>
        <div class="check"></div>
    </div>
    </div>
    <div class="restore-flyout {restore_overlay_disabled}" id="restore-flyout" style="top: {restore_flyout_top}px" onmouseenter="setRestoreMenuExpanded(true)" onmouseleave="setRestoreMenuExpanded(false)">{restore_options_html}</div>
</div>
<script>
window.ignoreBlur = false;
window.restoreExpanded = false;
window.restoreHideTimer = null;

function collapseRestoreMenu() {{
    window.restoreExpanded = false;
    if (window.restoreHideTimer) {{
        clearTimeout(window.restoreHideTimer);
        window.restoreHideTimer = null;
    }}
    const item = document.getElementById('restore-overlay-item');
    const flyout = document.getElementById('restore-flyout');
    if (item) {{
        item.classList.remove('expanded');
    }}
    if (flyout) {{
        flyout.classList.remove('visible');
    }}
}}

function positionRestoreMenu(topPx) {{
    const flyout = document.getElementById('restore-flyout');
    if (!flyout) {{
        return;
    }}
    flyout.style.top = topPx + 'px';
}}

function renderRestoreOptions(options) {{
    const menu = document.getElementById('restore-flyout');
    if (!menu) {{
        return;
    }}

    menu.innerHTML = '';
    for (const option of options) {{
        const item = document.createElement('div');
        item.className = 'restore-option';
        item.onclick = function() {{
            action('restore_recent:' + option.batch_count);
        }};

        const label = document.createElement('div');
        label.className = 'restore-option-label';
        label.textContent = option.label;
        item.appendChild(label);
        menu.appendChild(item);
    }}
}}

function setRestoreMenuExpanded(expanded) {{
    const item = document.getElementById('restore-overlay-item');
    const flyout = document.getElementById('restore-flyout');
    if (!item || !flyout) {{
        return;
    }}
    if (expanded && item.classList.contains('disabled')) {{
        return;
    }}
    if (expanded) {{
        if (window.restoreHideTimer) {{
            clearTimeout(window.restoreHideTimer);
            window.restoreHideTimer = null;
        }}
        if (!window.restoreExpanded) {{
            window.restoreExpanded = true;
            item.classList.add('expanded');
            flyout.classList.add('visible');
        }}
    }} else {{
        if (window.restoreHideTimer) {{
            clearTimeout(window.restoreHideTimer);
        }}
        window.restoreHideTimer = setTimeout(function() {{
            window.restoreHideTimer = null;
            if (!window.restoreExpanded) {{
                return;
            }}
            window.restoreExpanded = false;
            item.classList.remove('expanded');
            flyout.classList.remove('visible');
        }}, 90);
    }}
}}

function action(cmd) {{
    if (cmd === 'bubble') {{
        window.ignoreBlur = true;
        setTimeout(function() {{ window.ignoreBlur = false; }}, 1200);
        const el = document.querySelector('.bubble-item');
        if (el) {{
            if (el.classList.contains('active')) {{
                el.classList.remove('active');
            }} else {{
                el.classList.add('active');
            }}
        }}
    }} else if (cmd.startsWith('restore_recent:')) {{
        collapseRestoreMenu();
    }}
    window.ipc.postMessage(cmd);
}}

// Update popup state without reloading (preserves font cache)
window.updatePopupState = function(config) {{
    // Update CSS variables for theme
    document.documentElement.style.setProperty('--bg-color', config.bgColor);
    document.documentElement.style.setProperty('--text-color', config.textColor);
    document.documentElement.style.setProperty('--hover-bg', config.hoverColor);
    document.documentElement.style.setProperty('--border-color', config.borderColor);
    document.documentElement.style.setProperty('--separator-color', config.separatorColor);

    // Update label texts for language changes
    const labels = document.querySelectorAll('.menu-item .label');
    if (labels.length >= 5) {{
        labels[0].textContent = config.settingsText;
        labels[1].textContent = config.bubbleText;
        labels[2].textContent = config.stopTtsText;
        labels[3].textContent = config.restoreOverlayText;
        labels[4].textContent = config.quitText;
    }}

    // Update bubble active state
    const bubbleItem = document.querySelector('.bubble-item');
    if (bubbleItem) {{
        if (config.bubbleActive) {{
            bubbleItem.classList.add('active');
            document.getElementById('bubble-check-container').innerHTML = '<svg class="check-icon" viewBox="0 0 16 16" fill="currentColor"><path d="M13.86 3.66a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.6 9.03a.75.75 0 1 1 1.06-1.06l2.42 2.42 6.72-6.72a.75.75 0 0 1 1.06 0z"/></svg>';
        }} else {{
            bubbleItem.classList.remove('active');
            document.getElementById('bubble-check-container').innerHTML = '';
        }}
    }}

    // Update stop TTS disabled state
    const stopTtsItem = document.getElementById('stop-tts-item');
    if (stopTtsItem) {{
        if (config.ttsDisabled) {{
            stopTtsItem.classList.add('disabled');
        }} else {{
            stopTtsItem.classList.remove('disabled');
        }}
    }}

    renderRestoreOptions(config.restoreOptions);
    positionRestoreMenu(config.restoreFlyoutTop);
    collapseRestoreMenu();

    const restoreOverlayItem = document.getElementById('restore-overlay-item');
    const restoreFlyout = document.getElementById('restore-flyout');
    if (restoreOverlayItem) {{
        if (config.restoreDisabled) {{
            restoreOverlayItem.classList.add('disabled');
        }} else {{
            restoreOverlayItem.classList.remove('disabled');
        }}
    }}
    if (restoreFlyout) {{
        if (config.restoreDisabled) {{
            restoreFlyout.classList.add('disabled');
        }} else {{
            restoreFlyout.classList.remove('disabled');
        }}
    }}
}};

window.addEventListener('blur', function() {{
    if (window.ignoreBlur) return;
    collapseRestoreMenu();
    window.ipc.postMessage('close');
}});
</script>
</body>
</html>"#,
        bg = bg_color,
        text = text_color,
        hover = hover_color,
        border = border_color,
        separator = separator_color,
        base_popup_width = BASE_POPUP_WIDTH,
        settings = settings_text,
        bubble = bubble_text,
        stop_tts = stop_tts_text,
        stop_tts_disabled = stop_tts_disabled_class,
        restore_overlay = restore_overlay_text,
        restore_overlay_disabled = restore_overlay_disabled_class,
        restore_flyout_left = BASE_POPUP_WIDTH + RESTORE_FLYOUT_GAP,
        restore_flyout_top = restore_flyout_top,
        restore_flyout_width = RESTORE_FLYOUT_WIDTH,
        restore_options_html = restore_options_html,
        popup_surface_inset = POPUP_SURFACE_INSET,
        quit = quit_text,
        check = check_mark
    )
}

/// Generate JavaScript to update popup state without reloading HTML
fn generate_popup_update_script() -> String {
    use crate::config::ThemeMode;

    let mut ui_language = String::from("en");
    let (
        bubble_checked,
        is_dark_mode,
        settings_text,
        bubble_text,
        stop_tts_text,
        restore_overlay_text,
        quit_text,
    ) = if let Ok(app) = APP.lock() {
        ui_language = app.config.ui_language.clone();
        let is_dark = match app.config.theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
        };
        let (settings, bubble, stop_tts, restore_overlay, quit) =
            get_popup_labels(&app.config.ui_language);
        (
            app.config.show_favorite_bubble,
            is_dark,
            settings,
            bubble,
            stop_tts,
            restore_overlay,
            quit,
        )
    } else {
        (
            false,
            true,
            "Settings",
            "Favorite Bubble",
            "Stop All Playing TTS",
            "Restore Last Closed Overlay",
            "Quit",
        )
    };

    let has_tts_pending = crate::api::tts::TTS_MANAGER.has_pending_audio();
    let can_restore_last_closed = crate::overlay::result::can_restore_last_closed();
    let restore_options = get_restore_options(&ui_language);
    let restore_options_json =
        serde_json::to_string(&restore_options).unwrap_or_else(|_| "[]".into());
    let restore_flyout_top = restore_flyout_top_logical(restore_options.len());

    let (bg_color, text_color, hover_color, border_color, separator_color) = if is_dark_mode {
        (
            "#2c2c2c",
            "#ffffff",
            "#3c3c3c",
            "#454545",
            "rgba(255,255,255,0.08)",
        )
    } else {
        (
            "#f9f9f9",
            "#1a1a1a",
            "#eaeaea",
            "#dcdcdc",
            "rgba(0,0,0,0.06)",
        )
    };

    format!(
        r#"window.updatePopupState({{
            bgColor: '{}',
            textColor: '{}',
            hoverColor: '{}',
            borderColor: '{}',
            separatorColor: '{}',
            bubbleActive: {},
            ttsDisabled: {},
            restoreDisabled: {},
            restoreOptions: {},
            restoreFlyoutTop: {},
            settingsText: '{}',
            bubbleText: '{}',
            stopTtsText: '{}',
            restoreOverlayText: '{}',
            quitText: '{}'
        }});"#,
        bg_color,
        text_color,
        hover_color,
        border_color,
        separator_color,
        bubble_checked,
        !has_tts_pending,
        !can_restore_last_closed,
        restore_options_json,
        restore_flyout_top,
        settings_text,
        bubble_text,
        stop_tts_text,
        restore_overlay_text,
        quit_text
    )
}

fn get_popup_labels(
    ui_language: &str,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match ui_language {
        "vi" => (
            "Cài đặt",
            "Hiện bong bóng",
            "Dừng đọc",
            "Khôi phục overlay vừa đóng",
            "Thoát",
        ),
        "ko" => (
            "설정",
            "즐겨찾기 버블",
            "재생 중인 모든 음성 중지",
            "방금 닫은 오버레이 복원",
            "종료",
        ),
        _ => (
            "Settings",
            "Favorite Bubble",
            "Stop All Playing TTS",
            "Restore Last Closed Overlay",
            "Quit",
        ),
    }
}

// Cleanup guard removed - window persists for entire app lifetime

/// Creates the popup window and runs its message loop forever.
/// This is called once during warmup - the window is kept alive hidden for reuse.
fn create_popup_window() {
    unsafe {
        // Initialize COM for the thread (Critical for WebView2/Wry)
        let coinit = windows::Win32::System::Com::CoInitialize(None);
        crate::log_info!("[TrayPopup] Loop Start - CoInit: {:?}", coinit);

        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class_name = w!("SGTTrayPopup");

        REGISTER_POPUP_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(popup_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            RegisterClassW(&wc);
        });
        crate::log_info!("[TrayPopup] Class Registered");

        // Pre-size the transparent window for the optional restore flyout.
        let (popup_width, popup_height) = popup_window_dimensions();

        // Create hidden off-screen (will be repositioned when shown)
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name,
            w!("TrayPopup"),
            WS_POPUP,
            -3000,
            -3000,
            popup_width,
            popup_height,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        crate::log_info!("[TrayPopup] Window created with HWND: {:?}", hwnd);

        if hwnd.is_invalid() {
            return;
        }

        POPUP_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        // Make transparent initially (invisible)
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);

        // Disable native rounding/borders; CSS handles the visible card corners.
        let corner_pref = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner_pref) as *const _,
            std::mem::size_of_val(&corner_pref) as u32,
        );
        let border_color = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            std::ptr::addr_of!(border_color) as *const _,
            std::mem::size_of_val(&border_color) as u32,
        );
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        // Create WebView using shared context for RAM efficiency
        let wrapper = HwndWrapper(hwnd);
        let html = generate_popup_html();

        // Initialize shared WebContext if needed (uses same data dir as other modules)
        POPUP_WEB_CONTEXT.with(|ctx| {
            if ctx.borrow().is_none() {
                // Consolidate all minor overlays to 'common' to share one browser process and keep RAM at ~80MB
                let shared_data_dir = crate::overlay::get_shared_webview_data_dir(Some("common"));
                *ctx.borrow_mut() = Some(WebContext::new(Some(shared_data_dir)));
            }
        });

        crate::log_info!("[TrayPopup] Starting WebView initialization...");

        let mut final_webview: Option<WebView> = None;

        // Stagger startup to avoid collision
        std::thread::sleep(std::time::Duration::from_millis(250));

        for attempt in 1..=3 {
            let res = {
                // LOCK SCOPE: Only one WebView builds at a time to prevent "Not enough quota"
                let _init_lock = crate::overlay::GLOBAL_WEBVIEW_MUTEX.lock().unwrap();
                crate::log_info!(
                    "[TrayPopup] (Attempt {}) Acquired init lock. Building...",
                    attempt
                );

                POPUP_WEB_CONTEXT.with(|ctx| {
                    let mut ctx_ref = ctx.borrow_mut();
                    let builder = if let Some(web_ctx) = ctx_ref.as_mut() {
                        WebViewBuilder::new_with_web_context(web_ctx)
                    } else {
                        WebViewBuilder::new()
                    };
                    let builder = crate::overlay::html_components::font_manager::configure_webview(builder);

                    // Store HTML in font server and get URL for same-origin font loading
                    let page_url = crate::overlay::html_components::font_manager::store_html_page(html.clone())
                        .unwrap_or_else(|| format!("data:text/html,{}", urlencoding::encode(&html)));

                    builder
                        .with_bounds(Rect {
                            position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
                            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                                popup_width as u32,
                                popup_height as u32,
                            )),
                        })
                        .with_transparent(true)
                        .with_background_color((0, 0, 0, 0))
                        .with_url(&page_url)
                        .with_ipc_handler(move |msg: wry::http::Request<String>| {
                            let body = msg.body();
                            match body.as_str() {
                                "settings" => {
                                    // Hide popup and restore main window
                                    hide_tray_popup();
                                    crate::gui::signal_restore_window();
                                }
                                "bubble" => {
                                    // Toggle bubble state
                                    let new_state = if let Ok(mut app) = APP.lock() {
                                        app.config.show_favorite_bubble = !app.config.show_favorite_bubble;
                                        let enabled = app.config.show_favorite_bubble;
                                        crate::config::save_config(&app.config);

                                        if enabled {
                                            crate::overlay::favorite_bubble::show_favorite_bubble();
                                            std::thread::spawn(|| {
                                                std::thread::sleep(std::time::Duration::from_millis(150));
                                                crate::overlay::favorite_bubble::trigger_blink_animation();
                                            });
                                        } else {
                                            crate::overlay::favorite_bubble::hide_favorite_bubble();
                                        }
                                        enabled
                                    } else {
                                        false
                                    };

                                    // Update checkmark in popup via JavaScript (keep popup open)
                                    POPUP_WEBVIEW.with(|cell| {
                                        if let Some(webview) = cell.borrow().as_ref() {
                                            let js = format!(
                                                "document.getElementById('bubble-check-container').innerHTML = '{}';",
                                                if new_state {
                                                    r#"<svg class="check-icon" viewBox="0 0 16 16" fill="currentColor"><path d="M13.86 3.66a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.6 9.03a.75.75 0 1 1 1.06-1.06l2.42 2.42 6.72-6.72a.75.75 0 0 1 1.06 0z"/></svg>"#
                                                } else { "" }
                                            );
                                            let _ = webview.evaluate_script(&js);
                                        }
                                    });
                                }
                                "stop_tts" => {
                                    // Stop all TTS playback and clear queues
                                    crate::api::tts::TTS_MANAGER.stop();
                                    // Hide popup after action
                                    hide_tray_popup();
                                }
                                "restore_overlay" => {
                                    hide_tray_popup();
                                    std::thread::spawn(|| {
                                        std::thread::sleep(std::time::Duration::from_millis(60));
                                        let _ = crate::overlay::result::restore_last_closed();
                                    });
                                }
                                body if body.starts_with("restore_recent:") => {
                                    let batch_count = body
                                        .split_once(':')
                                        .and_then(|(_, value)| value.parse::<usize>().ok())
                                        .unwrap_or(1);
                                    hide_tray_popup();
                                    std::thread::spawn(move || {
                                        std::thread::sleep(std::time::Duration::from_millis(60));
                                        let _ = crate::overlay::result::restore_recent(batch_count);
                                    });
                                }
                                "quit" => {
                                    // Hide popup first, then exit
                                    hide_tray_popup();
                                    std::thread::spawn(|| {
                                        std::thread::sleep(std::time::Duration::from_millis(50));
                                        std::process::exit(0);
                                    });
                                }
                                "close" => {
                                    hide_tray_popup();
                                }
                                _ => {}
                            }
                        })
                        .build(&wrapper)
                })
            };

            crate::log_info!(
                "[TrayPopup] (Attempt {}) Release lock. Result: {}",
                attempt,
                if res.is_ok() { "OK" } else { "ERR" }
            );

            match res {
                Ok(wv) => {
                    final_webview = Some(wv);
                    break;
                }
                Err(e) => {
                    crate::log_info!(
                        "[TrayPopup] WebView init attempt {} failed: {:?}",
                        attempt,
                        e
                    );
                    std::thread::sleep(std::time::Duration::from_millis(2000));
                }
            }
        }

        if let Some(wv) = final_webview {
            crate::log_info!("[TrayPopup] WebView initialization SUCCESSFUL");
            POPUP_WEBVIEW.with(|cell| {
                *cell.borrow_mut() = Some(wv);
            });

            // Mark as warmed up - ready for instant display
            IS_WARMED_UP.store(true, Ordering::SeqCst);
            IS_WARMING_UP.store(false, Ordering::SeqCst); // Done warming up
            WARMUP_START_TIME.store(0, Ordering::SeqCst);

            // Message loop runs forever to keep window alive
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else {
            crate::log_info!("[TrayPopup] FAILED to initialize WebView after 3 attempts.");
            WEBVIEW_INIT_FAILED.store(true, Ordering::SeqCst);
        }

        // Clean up on thread exit
        IS_WARMED_UP.store(false, Ordering::SeqCst);
        IS_WARMING_UP.store(false, Ordering::SeqCst);
        POPUP_HWND.store(0, Ordering::SeqCst);
        WARMUP_START_TIME.store(0, Ordering::SeqCst);
        POPUP_WEBVIEW.with(|cell| {
            *cell.borrow_mut() = None;
        });

        windows::Win32::System::Com::CoUninitialize();
    }
}

unsafe extern "system" fn popup_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_SHOW => {
                // Reposition window to cursor and show
                let (popup_width, popup_height) = popup_window_dimensions();
                let popup_inset = get_scaled_dimension(POPUP_SURFACE_INSET);
                let main_width = get_scaled_dimension(BASE_POPUP_WIDTH);
                let main_height = get_scaled_dimension(BASE_POPUP_HEIGHT);

                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                let screen_h = GetSystemMetrics(SM_CYSCREEN);

                let main_x = (pt.x - main_width / 2)
                    .max(0)
                    .min((screen_w - main_width).max(0));
                let popup_x = (main_x - popup_inset)
                    .max(0)
                    .min((screen_w - popup_width).max(0));
                let popup_y = (pt.y - main_height - popup_inset - 10)
                    .max(0)
                    .min((screen_h - popup_height).max(0));

                // Update state via JavaScript (preserves font cache - no reload flash)
                POPUP_WEBVIEW.with(|cell| {
                    if let Some(webview) = cell.borrow().as_ref() {
                        let update_script = generate_popup_update_script();
                        let _ = webview.evaluate_script(&update_script);
                    }
                });

                set_popup_bounds(hwnd, popup_x, popup_y);

                // Make fully visible (undo the warmup transparency)
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);

                // Show and focus
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);

                // Start focus-polling timer
                let _ = SetTimer(Some(hwnd), 888, 100, None);

                LRESULT(0)
            }

            WM_ACTIVATE => LRESULT(0),

            WM_TIMER => {
                if wparam.0 == 888 {
                    // Focus polling: check if we're still the active window
                    let fg = GetForegroundWindow();
                    let root = GetAncestor(fg, GA_ROOT);

                    // If focus is on this popup or its children (WebView2), stay open
                    if fg == hwnd || root == hwnd {
                        return LRESULT(0);
                    }

                    // Focus is elsewhere - check grace period
                    let now = windows::Win32::System::SystemInformation::GetTickCount64();
                    if now > IGNORE_FOCUS_LOSS_UNTIL.load(Ordering::SeqCst) {
                        let _ = KillTimer(Some(hwnd), 888);
                        hide_tray_popup();
                    }
                }
                LRESULT(0)
            }

            WM_CLOSE => {
                // Just hide - don't destroy. Preserves WebView for instant redisplay.
                let _ = KillTimer(Some(hwnd), 888);
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }

            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }

            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// Fallback native context menu when WebView fails
unsafe fn show_native_context_menu() {
    unsafe {
        use crate::config::ThemeMode;
        use windows::core::{HSTRING, PCWSTR};

        let mut ui_language = String::from("en");
        let (
            settings_text,
            bubble_text,
            stop_tts_text,
            restore_overlay_text,
            quit_text,
            bubble_checked,
            _is_dark,
        ) = if let Ok(app) = APP.lock() {
            ui_language = app.config.ui_language.clone();
            let (settings, bubble, stop_tts, restore_overlay, quit) =
                get_popup_labels(&app.config.ui_language);
            let checked = app.config.show_favorite_bubble;

            let is_dark = match app.config.theme_mode {
                ThemeMode::Dark => true,
                ThemeMode::Light => false,
                ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
            };

            (
                settings,
                bubble,
                stop_tts,
                restore_overlay,
                quit,
                checked,
                is_dark,
            )
        } else {
            (
                "Settings",
                "Favorite Bubble",
                "Stop All TTS",
                "Restore Last Closed Overlay",
                "Quit",
                false,
                true,
            )
        };

        let has_tts_pending = crate::api::tts::TTS_MANAGER.has_pending_audio();
        let restore_options = get_restore_options(&ui_language);

        // Create a dummy window to handle menu messages
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("SGTNativeMenu"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        if hwnd.is_invalid() {
            return;
        }

        let _ = SetForegroundWindow(hwnd);

        let hmenu = CreatePopupMenu().unwrap_or_default();

        fn add_item(hmenu: HMENU, id: usize, text: &str, checked: bool, disabled: bool) {
            let mut flags = MF_STRING;
            if checked {
                flags |= MF_CHECKED;
            }
            if disabled {
                flags |= MF_DISABLED | MF_GRAYED;
            }

            let h_text = HSTRING::from(text);
            unsafe {
                let _ = AppendMenuW(hmenu, flags, id, PCWSTR(h_text.as_ptr()));
            }
        }

        add_item(hmenu, 1, settings_text, false, false);
        add_item(hmenu, 2, bubble_text, bubble_checked, false);
        add_item(hmenu, 3, stop_tts_text, false, !has_tts_pending);
        if restore_options.is_empty() {
            add_item(hmenu, 4, restore_overlay_text, false, true);
        } else {
            let restore_menu = CreatePopupMenu().unwrap_or_default();
            for option in &restore_options {
                add_item(
                    restore_menu,
                    40 + option.batch_count,
                    &option.label,
                    false,
                    false,
                );
            }

            let h_text = HSTRING::from(restore_overlay_text);
            let _ = AppendMenuW(
                hmenu,
                MF_POPUP,
                restore_menu.0 as usize,
                PCWSTR(h_text.as_ptr()),
            );
        }
        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
        add_item(hmenu, 5, quit_text, false, false);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        let cmd_id = TrackPopupMenu(
            hmenu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_BOTTOMALIGN | TPM_LEFTALIGN,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );

        let _ = DestroyMenu(hmenu);
        let _ = DestroyWindow(hwnd);

        match cmd_id.0 as u32 {
            1 => {
                // Settings
                crate::gui::signal_restore_window();
            }
            2 => {
                // Toggle Bubble
                if let Ok(mut app) = APP.lock() {
                    app.config.show_favorite_bubble = !app.config.show_favorite_bubble;
                    let enabled = app.config.show_favorite_bubble;
                    crate::config::save_config(&app.config);

                    if enabled {
                        crate::overlay::favorite_bubble::show_favorite_bubble();
                        std::thread::spawn(|| {
                            std::thread::sleep(std::time::Duration::from_millis(150));
                            crate::overlay::favorite_bubble::trigger_blink_animation();
                        });
                    } else {
                        crate::overlay::favorite_bubble::hide_favorite_bubble();
                    }
                }
            }
            3 => {
                // Stop TTS
                crate::api::tts::TTS_MANAGER.stop();
            }
            41..=45 => {
                let batch_count = cmd_id.0 as usize - 40;
                std::thread::spawn(move || {
                    let _ = crate::overlay::result::restore_recent(batch_count);
                });
            }
            5 => {
                // Quit
                std::process::exit(0);
            }
            _ => {}
        }
    }
}
