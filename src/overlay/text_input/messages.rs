// --- TEXT INPUT MESSAGES ---
// Window procedure and message handling.

use super::editor_text::apply_pending_text;
use super::passive;
use super::state::*;
use super::styles::get_editor_css;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;
use wry::Rect;

unsafe fn set_noactivate_mode(hwnd: HWND, enabled: bool) {
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
    let noactivate = WS_EX_NOACTIVATE.0;
    let target = if enabled {
        ex_style | noactivate
    } else {
        ex_style & !noactivate
    };

    if target != ex_style {
        let _ = unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, target as isize) };
    }
}

pub unsafe extern "system" fn input_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_SHOW => {
                let session = if wparam.0 == 0 {
                    let Some(show) = PENDING_SHOW.lock().unwrap().take() else {
                        return LRESULT(0);
                    };
                    if show.passive && INPUT_HOOK.lock().unwrap().0.is_invalid() {
                        let text = crate::gui::locale::LocaleText::get(&show.language).shell;
                        crate::overlay::auto_copy_badge::show_error_notification(
                            text.text_input_unavailable,
                        );
                        return LRESULT(0);
                    }
                    super::focus::cancel(hwnd);
                    EDITOR_SESSIONS.begin();
                    let session = EDITOR_SESSIONS.current().unwrap();
                    EDITOR_READY.store(0, std::sync::atomic::Ordering::SeqCst);
                    CALLBACK_OWNER.store(session, std::sync::atomic::Ordering::SeqCst);
                    *CFG_TITLE.lock().unwrap() = show.title;
                    *CFG_LANG.lock().unwrap() = show.language;
                    *CFG_CANCEL.lock().unwrap() = show.cancel_hotkey;
                    *CFG_CONTINUOUS.lock().unwrap() = show.continuous;
                    *CFG_CALLBACK.lock().unwrap() = Some(show.callback);
                    PASSIVE_CAPTURE_ENABLED
                        .store(show.passive, std::sync::atomic::Ordering::SeqCst);
                    passive::reset_state();
                    *SUBMITTED_TEXT.lock().unwrap() = None;
                    *SHOULD_CLOSE.lock().unwrap() = false;
                    *SHOULD_CLEAR_ONLY.lock().unwrap() = false;
                    HIDE_PENDING.with(|pending| pending.set(false));
                    let _ = KillTimer(Some(hwnd), 4);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    if EDITOR_SESSIONS.is_current(session) {
                        super::activation::start(hwnd, session);
                    }
                    return LRESULT(0);
                } else {
                    let session = lparam.0 as u64;
                    if !EDITOR_SESSIONS.is_current(session)
                        || EDITOR_READY.load(std::sync::atomic::Ordering::SeqCst) != session
                    {
                        return LRESULT(0);
                    }
                    session
                };
                // Restore History Navigation State
                crate::overlay::input_history::reset_history_navigation();

                // Moved playEntry to end of block to run AFTER text updates

                // 1. Position Logic - Center on the monitor where the cursor is
                if wparam.0 != 1 {
                    // Sized for the cursor's monitor, not the warm-up monitor:
                    // with mixed scales a fixed size would be half or double.
                    let (cursor, area) = crate::overlay::display_placement::cursor_work_area();
                    let (w, h) = super::window::editor_size(
                        area,
                        crate::overlay::display_placement::scale_at_point(cursor),
                    );
                    let target = area.centered(w, h);

                    let passive_capture =
                        PASSIVE_CAPTURE_ENABLED.load(std::sync::atomic::Ordering::SeqCst);
                    set_noactivate_mode(hwnd, passive_capture);
                    let flags = if passive_capture {
                        SWP_SHOWWINDOW | SWP_NOACTIVATE
                    } else {
                        SWP_SHOWWINDOW
                    };
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOP),
                        target.left,
                        target.top,
                        target.right - target.left,
                        target.bottom - target.top,
                        flags,
                    );
                    if !EDITOR_SESSIONS.is_current(session) {
                        return LRESULT(0);
                    }
                }

                let passive_capture =
                    PASSIVE_CAPTURE_ENABLED.load(std::sync::atomic::Ordering::SeqCst);

                // 2. Focus / Activation
                if !passive_capture {
                    let _ = SetForegroundWindow(hwnd);
                    if !EDITOR_SESSIONS.is_current(session) {
                        return LRESULT(0);
                    }
                    let _ = SetFocus(Some(hwnd));
                    if !EDITOR_SESSIONS.is_current(session) {
                        return LRESULT(0);
                    }
                    TEXT_INPUT_WEBVIEW.with(|webview| {
                        if let Some(wv) = webview.borrow().as_ref() {
                            let _ = wv.focus();
                        }
                    });
                    if !EDITOR_SESSIONS.is_current(session) {
                        return LRESULT(0);
                    }
                }

                // 3. Dynamic Update (Theme + Locales)
                let is_dark = if let Ok(app) = crate::APP.lock() {
                    app.config.theme_mode.is_dark()
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
                        locale.preset_editor.text_input_placeholder.to_string()
                    } else {
                        t
                    };
                    let hotkey = CFG_CANCEL.lock().unwrap();
                    let ch = crate::hotkey::names::with_escape(&hotkey);
                    (
                        title,
                        locale.preset_editor.text_input_footer_submit.to_string(),
                        locale.preset_editor.text_input_footer_newline.to_string(),
                        locale.preset_editor.text_input_footer_cancel.to_string(),
                        ch,
                        locale.preset_editor.text_input_placeholder.to_string(),
                    )
                };

                // Update window title
                let _ = SetWindowTextW(hwnd, &HSTRING::from(&title));

                let css = get_editor_css(is_dark);

                // Construct footer HTML
                let footer_html =
                    format!("{}  |  {}  |  {} {}", submit, newline, cancel_hint, cancel);

                let script = format!(
                    r#"(() => {{
                const sgtSession = '{}';
                if (window.__sgtTextSession !== sgtSession) return;
                if (document.getElementById('theme-style')) {{
                   document.getElementById('theme-style').textContent = {};
                }}
                if (document.getElementById('headerTitle')) {{
                   document.getElementById('headerTitle').textContent = {};
                }}
                if (document.getElementById('footerRegion')) {{
                   document.getElementById('footerRegion').textContent = {};
                }}
                if (document.getElementById('editor')) {{
                   document.getElementById('editor').placeholder = {};
                }}
                document.documentElement.setAttribute('data-theme', '{}');
                }})();
                "#,
                    session,
                    serde_json::json!(css),
                    serde_json::json!(title),
                    serde_json::json!(footer_html),
                    serde_json::json!(placeholder),
                    if is_dark { "dark" } else { "light" }
                );

                TEXT_INPUT_WEBVIEW.with(|webview| {
                    if let Some(wv) = webview.borrow().as_ref() {
                        let _ = wv.evaluate_script(&script);
                        // NOW trigger animation, after text has been updated
                        if EDITOR_SESSIONS.is_current(session) {
                            let _ = wv.evaluate_script("playEntry();");
                        }
                    }
                });
                if !EDITOR_SESSIONS.is_current(session) {
                    return LRESULT(0);
                }

                if passive_capture {
                    passive::sync_editor();
                }
                if wparam.0 == 2 {
                    if !passive_capture {
                        super::refocus_editor_for_session(session);
                    }
                    super::wake_editor_queue();
                }
                if !EDITOR_SESSIONS.is_current(session) {
                    return LRESULT(0);
                }

                // IPC check timer
                SetTimer(Some(hwnd), 2, 50, None);

                LRESULT(0)
            }

            WM_APP_ACTIVATION_ACK => {
                let session = wparam.0 as u64;
                if super::activation::ack(hwnd, session, lparam.0 == 1) {
                    return input_wnd_proc(hwnd, WM_APP_SHOW, WPARAM(2), LPARAM(session as isize));
                }
                LRESULT(0)
            }

            WM_APP_PRIOR_QUIET => {
                let session = wparam.0 as u64;
                if super::activation::prior_quiet(hwnd, session) {
                    return input_wnd_proc(hwnd, WM_APP_SHOW, WPARAM(2), LPARAM(session as isize));
                }
                LRESULT(0)
            }

            WM_APP_SET_TEXT => {
                // Apply pending text from cross-thread call
                if EDITOR_SESSIONS.current()
                    == Some(EDITOR_READY.load(std::sync::atomic::Ordering::SeqCst))
                {
                    apply_pending_text();
                }
                LRESULT(0)
            }

            WM_APP_REVOKE_TEXT => {
                super::editor_text::revoke_operation(wparam.0 as u64);
                LRESULT(0)
            }

            WM_APP_INPUT_LIMIT => {
                if EDITOR_SESSIONS.is_current(wparam.0 as u64) {
                    super::notify_text_delivery(false);
                }
                LRESULT(0)
            }

            WM_APP_PASSIVE_KEY => {
                passive::handle_deferred_key(wparam, lparam);
                LRESULT(0)
            }

            WM_APP_SYNC_PASSIVE_EDITOR => {
                if PASSIVE_CAPTURE_ENABLED.load(std::sync::atomic::Ordering::SeqCst) {
                    passive::sync_editor();
                }
                LRESULT(0)
            }

            WM_APP_UPDATE_THEME => {
                let is_dark = wparam.0 != 0;
                let css = serde_json::to_string(&get_editor_css(is_dark)).unwrap_or_default();
                let theme = if is_dark { "dark" } else { "light" };
                let script = format!(
                    "const style=document.getElementById('theme-style');\
                     if(style) style.textContent={css};\
                     document.documentElement.setAttribute('data-theme','{theme}');"
                );
                TEXT_INPUT_WEBVIEW.with(|webview| {
                    if let Some(webview) = webview.borrow().as_ref() {
                        let _ = webview.evaluate_script(&script);
                    }
                });
                LRESULT(0)
            }

            WM_APP_HIDE => {
                super::activation::retire(hwnd);
                if EDITOR_SESSIONS.current().is_some() {
                    return LRESULT(0);
                }
                let _ = KillTimer(Some(hwnd), 2);
                let _ = KillTimer(Some(hwnd), 3);
                PASSIVE_CAPTURE_ENABLED.store(false, std::sync::atomic::Ordering::SeqCst);
                // Trigger Fade Out Script & Delay Hide
                TEXT_INPUT_WEBVIEW.with(|webview| {
                    if let Some(wv) = webview.borrow().as_ref() {
                        let _ = wv.evaluate_script("playExit();");
                    }
                });
                if EDITOR_SESSIONS.current().is_some() {
                    return LRESULT(0);
                }
                // 150ms delay for animation (Timer ID 4)
                HIDE_PENDING.with(|pending| pending.set(true));
                SetTimer(Some(hwnd), 4, 150, None);
                LRESULT(0)
            }

            WM_CLOSE => {
                super::activation::retire(hwnd);
                if EDITOR_SESSIONS.current().is_some() {
                    return LRESULT(0);
                }
                HIDE_PENDING.with(|pending| pending.set(false));
                let _ = KillTimer(Some(hwnd), 4);
                PASSIVE_CAPTURE_ENABLED.store(false, std::sync::atomic::Ordering::SeqCst);
                let _ = ShowWindow(hwnd, SW_HIDE);
                let _ = KillTimer(Some(hwnd), 2);
                let _ = KillTimer(Some(hwnd), 3);
                LRESULT(0)
            }

            WM_DESTROY => {
                super::activation::stop(hwnd);
                super::focus::cancel(hwnd);
                EDITOR_READY.store(0, std::sync::atomic::Ordering::SeqCst);
                EDITOR_SESSIONS.end();
                let nonce = DOCUMENT_NONCE.lock().unwrap().take();
                if let Some(nonce) = nonce {
                    super::delivery::abandon_document(&nonce);
                }
                CALLBACK_OWNER.store(0, std::sync::atomic::Ordering::SeqCst);
                PostQuitMessage(0);
                LRESULT(0)
            }

            WM_APP_CLOSE_SESSION => {
                if super::is_callback_owner(wparam.0 as u64) {
                    return input_wnd_proc(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                LRESULT(0)
            }

            WM_ERASEBKGND => LRESULT(1),

            WM_SETFOCUS => {
                if !PASSIVE_CAPTURE_ENABLED.load(std::sync::atomic::Ordering::SeqCst) {
                    TEXT_INPUT_WEBVIEW.with(|webview| {
                        if let Some(wv) = webview.borrow().as_ref() {
                            let _ = wv.focus();
                        }
                    });
                }
                LRESULT(0)
            }

            WM_TIMER => {
                if wparam.0 == 7 {
                    super::activation::tick(hwnd);
                }

                if wparam.0 == 2 && !SUBMISSION_IN_PROGRESS.with(|active| active.get()) {
                    // IPC messages
                    let should_close = *SHOULD_CLOSE.lock().unwrap();
                    if should_close {
                        *SHOULD_CLOSE.lock().unwrap() = false;
                        let submitted = SUBMITTED_TEXT.lock().unwrap().take();
                        if let Some(SubmittedText {
                            text,
                            revision,
                            passive,
                            snapshot,
                        }) = submitted
                        {
                            let continuous = *CFG_CONTINUOUS.lock().unwrap();
                            let Some(session) = EDITOR_SESSIONS.current() else {
                                return LRESULT(0);
                            };
                            // Callbacks may run a nested message loop. Do not dispatch
                            // another submission until this one has returned.
                            let _ = KillTimer(Some(hwnd), 2);
                            if continuous {
                                dispatch_submit(&CFG_CALLBACK, text, hwnd, session);
                                if EDITOR_SESSIONS.is_current(session) {
                                    super::queue_editor_action(
                                        session,
                                        String::new(),
                                        EditorAction::Clear {
                                            revision,
                                            passive,
                                            snapshot,
                                        },
                                        None,
                                    );
                                    super::refocus_editor_for_session(session);
                                    if EDITOR_SESSIONS.is_current(session) {
                                        SetTimer(Some(hwnd), 2, 50, None);
                                    }
                                }
                            } else {
                                super::focus::cancel(hwnd);
                                EDITOR_READY.store(0, std::sync::atomic::Ordering::SeqCst);
                                EDITOR_SESSIONS.end();
                                super::activation::invalidate(session);
                                if !super::is_callback_owner(session) {
                                    return LRESULT(0);
                                }
                                HIDE_PENDING.with(|pending| pending.set(false));
                                PASSIVE_CAPTURE_ENABLED
                                    .store(false, std::sync::atomic::Ordering::SeqCst);
                                let _ = KillTimer(Some(hwnd), 3);
                                let _ = KillTimer(Some(hwnd), 4);
                                let _ = ShowWindow(hwnd, SW_HIDE);
                                if !super::is_callback_owner(session) {
                                    return LRESULT(0);
                                }
                                dispatch_submit(&CFG_CALLBACK, text, hwnd, session);
                                let _ = CALLBACK_OWNER.compare_exchange(
                                    session,
                                    0,
                                    std::sync::atomic::Ordering::SeqCst,
                                    std::sync::atomic::Ordering::SeqCst,
                                );
                            }
                        } else {
                            super::activation::retire(hwnd);
                            if EDITOR_SESSIONS.current().is_some() {
                                return LRESULT(0);
                            }
                            HIDE_PENDING.with(|pending| pending.set(false));
                            PASSIVE_CAPTURE_ENABLED
                                .store(false, std::sync::atomic::Ordering::SeqCst);
                            let _ = KillTimer(Some(hwnd), 1);
                            let _ = KillTimer(Some(hwnd), 2);
                            let _ = KillTimer(Some(hwnd), 3);
                            let _ = KillTimer(Some(hwnd), 4);
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        }
                    }
                }
                // Timer 3: focus logic (used by refocus_editor after preset wheel)
                if wparam.0 == 3 {
                    super::focus::fire(hwnd);
                }
                // Timer 4: Hide window after fade-out
                if wparam.0 == 4 && HIDE_PENDING.with(|pending| pending.replace(false)) {
                    let _ = KillTimer(Some(hwnd), 4);
                    EDITOR_SESSIONS.end();
                    let _ = ShowWindow(hwnd, SW_HIDE);

                    *SHOULD_CLOSE.lock().unwrap() = false;
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

            WM_DPICHANGED => {
                crate::overlay::display_placement::apply_suggested_dpi_rect(hwnd, lparam);
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
}
