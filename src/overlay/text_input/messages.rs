// --- TEXT INPUT MESSAGES ---
// Window procedure and message handling.

use super::state::*;
use super::styles::get_editor_css;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::Rect;

/// Internal function to apply pending text (called on the window's thread)
/// Inserts text at the current cursor position instead of replacing all content
pub fn apply_pending_text() {
    let text = PENDING_TEXT.lock().unwrap().take();
    if let Some(text) = text {
        // Check if this is a history replacement (replace all) or insertion
        let (is_replace_all, actual_text) =
            if let Some(stripped) = text.strip_prefix("__REPLACE_ALL__") {
                (true, stripped.to_string())
            } else {
                (false, text)
            };

        let escaped = actual_text
            .replace('\\', "\\\\")
            .replace('`', "\\`")
            .replace("${", "\\${")
            .replace('\n', "\\n")
            .replace('\r', "");

        TEXT_INPUT_WEBVIEW.with(|webview| {
            if let Some(wv) = webview.borrow().as_ref() {
                let script = if is_replace_all {
                    // Replace all text (for history navigation)
                    format!(
                        r#"(function() {{
                            const editor = document.getElementById('editor');
                            const text = `{}`;
                            editor.value = text;
                            editor.selectionStart = editor.selectionEnd = text.length;
                            editor.focus();
                        }})();"#,
                        escaped
                    )
                } else {
                    // Insert at cursor position (for paste/transcription)
                    format!(
                        r#"(function() {{
                            const editor = document.getElementById('editor');
                            const start = editor.selectionStart;
                            const end = editor.selectionEnd;
                            const text = `{}`;
                            editor.value = editor.value.substring(0, start) + text + editor.value.substring(end);
                            editor.selectionStart = editor.selectionEnd = start + text.length;
                            editor.focus();
                        }})();"#,
                        escaped
                    )
                };
                let _ = wv.evaluate_script(&script);
            }
        });
        println!("[Badge] Starting WebView initialization...");
    }
}

/// Clear the webview editor content and refocus (for continuous input mode)
pub fn clear_editor_text() {
    TEXT_INPUT_WEBVIEW.with(|webview| {
        if let Some(wv) = webview.borrow().as_ref() {
            let script = r#"document.getElementById('editor').value = ''; document.getElementById('editor').focus();"#;
            let _ = wv.evaluate_script(script);
        }
    });
}

pub unsafe extern "system" fn input_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // State variables for this window instance
    static mut FADE_ALPHA: i32 = 0;
    // IS_DRAGGING is no longer needed with native drag

    match msg {
        WM_APP_SHOW => {
            // Restore History Navigation State
            crate::overlay::input_history::reset_history_navigation();

            // Moved playEntry to end of block to run AFTER text updates

            // 1. Position Logic - Center on the monitor where the cursor is
            if wparam.0 != 1 {
                let mut cursor = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut cursor);
                    let hmonitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
                    let mut mi = MONITORINFO {
                        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                        ..Default::default()
                    };
                    let _ = GetMonitorInfoW(hmonitor, &mut mi);

                    let mut rect = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut rect);
                    let w = rect.right - rect.left;
                    let h = rect.bottom - rect.top;

                    let monitor_w = mi.rcWork.right - mi.rcWork.left;
                    let monitor_h = mi.rcWork.bottom - mi.rcWork.top;

                    let x = mi.rcWork.left + (monitor_w - w) / 2;
                    let y = mi.rcWork.top + (monitor_h - h) / 2;

                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOP),
                        x,
                        y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_SHOWWINDOW,
                    );
                }
            }

            // 2. Focus - Force window to foreground
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
            // Force Webview focus immediately
            TEXT_INPUT_WEBVIEW.with(|webview| {
                if let Some(wv) = webview.borrow().as_ref() {
                    let _ = wv.focus();
                }
            });

            // 3. Dynamic Update (Theme + Locales)
            let is_dark = if let Ok(app) = crate::APP.lock() {
                match app.config.theme_mode {
                    crate::config::ThemeMode::Dark => true,
                    crate::config::ThemeMode::Light => false,
                    crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
                }
            } else {
                true
            };

            // Re-fetch locales to ensure they are current
            let (title, submit, newline, cancel, cancel_hint, placeholder) = {
                let lang = CFG_LANG.lock().unwrap().clone();
                let locale = crate::gui::locale::LocaleText::get(&lang);
                let t = CFG_TITLE.lock().unwrap().clone();
                let title = if t.is_empty() {
                    let lang = CFG_LANG.lock().unwrap().clone();
                    let locale = crate::gui::locale::LocaleText::get(&lang);
                    locale.text_input_placeholder.to_string()
                } else {
                    t
                };
                let hotkey = CFG_CANCEL.lock().unwrap();
                let ch = if hotkey.is_empty() {
                    "Esc".to_string()
                } else {
                    format!("Esc / {}", hotkey)
                };
                (
                    title,
                    locale.text_input_footer_submit.to_string(),
                    locale.text_input_footer_newline.to_string(),
                    locale.text_input_footer_cancel.to_string(),
                    ch,
                    locale.text_input_placeholder.to_string(),
                )
            };

            // Update window title
            let _ = SetWindowTextW(hwnd, &HSTRING::from(&title));

            let css = get_editor_css(is_dark);
            let css_escaped = css.replace("`", "\\`");

            // Construct footer HTML
            let footer_html = format!("{}  |  {}  |  {} {}", submit, newline, cancel_hint, cancel);
            let placeholder_escaped = placeholder.replace("'", "\\'"); // rudimentary escape

            let script = format!(
                r#"
                if (document.getElementById('theme-style')) {{
                   document.getElementById('theme-style').innerHTML = `{}`;
                }}
                if (document.getElementById('headerTitle')) {{
                   document.getElementById('headerTitle').innerText = `{}`;
                }}
                if (document.getElementById('footerRegion')) {{
                   document.getElementById('footerRegion').innerHTML = `{}`;
                }}
                if (document.getElementById('editor')) {{
                   document.getElementById('editor').placeholder = '{}';
                }}
                document.documentElement.setAttribute('data-theme', '{}');
                // Force focus on editor
                setTimeout(() => {{
                    const el = document.getElementById('editor');
                    if (el) {{
                        el.focus();
                        el.select();
                        el.selectionStart = el.selectionEnd = el.value.length;
                    }}
                }}, 10);
                "#,
                css_escaped,
                title,
                footer_html,
                placeholder_escaped,
                if is_dark { "dark" } else { "light" }
            );

            TEXT_INPUT_WEBVIEW.with(|webview| {
                if let Some(wv) = webview.borrow().as_ref() {
                    let _ = wv.evaluate_script(&script);
                    // NOW trigger animation, after text has been updated
                    let _ = wv.evaluate_script("playEntry();");
                }
            });

            // Reset state
            FADE_ALPHA = 0;

            // IPC check timer
            SetTimer(Some(hwnd), 2, 50, None);

            LRESULT(0)
        }

        WM_APP_SET_TEXT => {
            // Apply pending text from cross-thread call
            apply_pending_text();
            LRESULT(0)
        }

        WM_APP_HIDE => {
            // Trigger Fade Out Script & Delay Hide
            TEXT_INPUT_WEBVIEW.with(|webview| {
                if let Some(wv) = webview.borrow().as_ref() {
                    let _ = wv.evaluate_script("playExit();");
                }
            });
            // 150ms delay for animation (Timer ID 4)
            SetTimer(Some(hwnd), 4, 150, None);
            LRESULT(0)
        }

        WM_CLOSE => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            let _ = KillTimer(Some(hwnd), 1);
            let _ = KillTimer(Some(hwnd), 2);
            let _ = KillTimer(Some(hwnd), 3);
            LRESULT(0)
        }

        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }

        WM_ERASEBKGND => LRESULT(1),

        WM_SETFOCUS => {
            TEXT_INPUT_WEBVIEW.with(|webview| {
                if let Some(wv) = webview.borrow().as_ref() {
                    let _ = wv.focus();
                }
            });
            LRESULT(0)
        }

        WM_TIMER => {
            if wparam.0 == 1 {
                // Fade Timer Logic removed
                let _ = KillTimer(Some(hwnd), 1);
            }

            if wparam.0 == 2 {
                // IPC messages
                let should_close = *SHOULD_CLOSE.lock().unwrap();
                if should_close {
                    *SHOULD_CLOSE.lock().unwrap() = false;
                    let submitted = SUBMITTED_TEXT.lock().unwrap().take();
                    if let Some(text) = submitted {
                        let continuous = *CFG_CONTINUOUS.lock().unwrap();
                        if continuous {
                            let cb_lock = CFG_CALLBACK.lock().unwrap();
                            if let Some(cb) = cb_lock.as_ref() {
                                cb(text, hwnd);
                            }
                            clear_editor_text();
                            super::refocus_editor();
                        } else {
                            let _ = ShowWindow(hwnd, SW_HIDE);
                            let cb_lock = CFG_CALLBACK.lock().unwrap();
                            if let Some(cb) = cb_lock.as_ref() {
                                cb(text, hwnd);
                            }
                        }
                    } else {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    }
                }
            }
            // Timer 3: focus logic (used by refocus_editor after preset wheel)
            if wparam.0 == 3 {
                let _ = KillTimer(Some(hwnd), 3);
                TEXT_INPUT_WEBVIEW.with(|webview| {
                    if let Some(wv) = webview.borrow().as_ref() {
                        let _ = wv.focus();
                        let _ = wv.evaluate_script("document.getElementById('editor').focus();");
                    }
                });
            }
            // Timer 4: Hide window after fade-out
            if wparam.0 == 4 {
                let _ = KillTimer(Some(hwnd), 4);
                let _ = ShowWindow(hwnd, SW_HIDE);

                // Signal closure if needed
                let mut should_close = SHOULD_CLOSE.lock().unwrap();
                if *should_close {
                    *should_close = false;
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            LRESULT(0)
        }

        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let w = rect.right;

            // Close Button
            let close_x = w - 30;
            let close_y = 20;
            if (x - close_x).abs() < 15 && (y - close_y).abs() < 15 {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                return LRESULT(0);
            }

            // Title Bar Drag - Use Native Drag (Fix drifting issues)
            if y < 50 {
                let _ = ReleaseCapture();
                SendMessageW(hwnd, WM_SYSCOMMAND, Some(WPARAM(0xF012)), Some(LPARAM(0)));
                return LRESULT(0);
            }
            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            // WM_MOUSEMOVE drag logic removed in favor of native drag
            LRESULT(0)
        }

        WM_LBUTTONUP => {
            // No capture cleanup needed for native drag
            LRESULT(0)
        }

        WM_SIZE => {
            // Resize WebView to match the new client area
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            if width > 0 && height > 0 {
                TEXT_INPUT_WEBVIEW.with(|wv| {
                    if let Some(webview) = wv.borrow().as_ref() {
                        let _ = webview.set_bounds(Rect {
                            position: wry::dpi::Position::Physical(
                                wry::dpi::PhysicalPosition::new(0, 0),
                            ),
                            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                                width as u32,
                                height as u32,
                            )),
                        });
                    }
                });
            }
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
