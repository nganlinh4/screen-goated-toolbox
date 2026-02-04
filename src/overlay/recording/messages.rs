// --- RECORDING MESSAGES ---
// Window procedure and keyboard hook for recording overlay.

use super::state::*;
use super::window::start_audio_thread;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub unsafe extern "system" fn recording_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_APP_SHOW => {
            let preset_idx = wparam.0;

            // Reset JS state
            RECORDING_WEBVIEW.with(|cell| {
                if let Some(wv) = cell.borrow().as_ref() {
                    let _ = wv.evaluate_script("resetState();");
                }
            });

            // Start Audio Logic
            start_audio_thread(hwnd, preset_idx);

            // Mark state as Active (Visible)
            RECORDING_STATE.store(2, Ordering::SeqCst);

            // Check if we should hide the UI
            let is_hidden = {
                let app = APP.lock().unwrap();
                if preset_idx < app.config.presets.len() {
                    app.config.presets[preset_idx].hide_recording_ui
                } else {
                    false
                }
            };
            CURRENT_RECORDING_HIDDEN.store(is_hidden, Ordering::SeqCst);

            // Fallback Timer (99) - If IPC ready signal doesn't come in 500ms, show anyway
            if !is_hidden {
                SetTimer(Some(hwnd), 99, 500, None);
            }

            // Record Show Time to prevent race with old threads closing
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            LAST_SHOW_TIME.store(now, Ordering::SeqCst);

            LRESULT(0)
        }

        WM_TIMER => {
            if wparam.0 == 2 {
                // REAL SHOW TIMER (from IPC "ready")
                let _ = KillTimer(Some(hwnd), 2);
                let _ = PostMessageW(Some(hwnd), WM_APP_REAL_SHOW, WPARAM(0), LPARAM(0));
            } else if wparam.0 == 99 {
                // FALLBACK TIMER (IPC timed out)
                let _ = KillTimer(Some(hwnd), 99);
                println!("Warning: Recording overlay IPC timed out, forcing show");
                let _ = PostMessageW(Some(hwnd), WM_APP_REAL_SHOW, WPARAM(0), LPARAM(0));
            } else if wparam.0 == 1 {
                // VIZ UPDATE TIMER
                let is_processing = AUDIO_STOP_SIGNAL.load(Ordering::SeqCst);
                let is_paused = AUDIO_PAUSE_SIGNAL.load(Ordering::SeqCst);
                let is_initializing = AUDIO_INITIALIZING.load(Ordering::SeqCst);
                let warming_up = !AUDIO_WARMUP_COMPLETE.load(Ordering::SeqCst);

                let rms_bits = CURRENT_RMS.load(Ordering::Relaxed);
                let rms = f32::from_bits(rms_bits);

                let state_str = if is_processing {
                    "processing"
                } else if is_paused {
                    "paused"
                } else if is_initializing {
                    "initializing"
                } else if warming_up {
                    "warmup"
                } else {
                    "recording"
                };

                let script = format!("updateState('{}', {});", state_str, rms);

                RECORDING_WEBVIEW.with(|cell| {
                    if let Some(wv) = cell.borrow().as_ref() {
                        let _ = wv.evaluate_script(&script);
                    }
                });

                // Check for theme changes
                if let Ok(app) = APP.try_lock() {
                    let current_is_dark = match app.config.theme_mode {
                        crate::config::ThemeMode::Dark => true,
                        crate::config::ThemeMode::Light => false,
                        crate::config::ThemeMode::System => {
                            crate::gui::utils::is_system_in_dark_mode()
                        }
                    };
                    let last_dark = LAST_THEME_IS_DARK.load(Ordering::SeqCst);

                    if current_is_dark != last_dark {
                        LAST_THEME_IS_DARK.store(current_is_dark, Ordering::SeqCst);

                        let (
                            container_bg,
                            container_border,
                            text_color,
                            subtext_color,
                            btn_bg,
                            btn_hover_bg,
                            btn_color,
                            text_shadow,
                        ) = if current_is_dark {
                            (
                                "rgba(18, 18, 18, 0.85)",
                                "rgba(255, 255, 255, 0.1)",
                                "white",
                                "rgba(255, 255, 255, 0.7)",
                                "rgba(255, 255, 255, 0.05)",
                                "rgba(255, 255, 255, 0.15)",
                                "rgba(255, 255, 255, 0.8)",
                                "0 1px 2px rgba(0, 0, 0, 0.3)",
                            )
                        } else {
                            (
                                "rgba(255, 255, 255, 0.92)",
                                "rgba(0, 0, 0, 0.1)",
                                "#222222",
                                "rgba(0, 0, 0, 0.6)",
                                "rgba(0, 0, 0, 0.05)",
                                "rgba(0, 0, 0, 0.1)",
                                "rgba(0, 0, 0, 0.7)",
                                "0 1px 2px rgba(255, 255, 255, 0.3)",
                            )
                        };

                        let theme_script = format!(
                            "if(window.updateTheme) window.updateTheme({}, '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}');",
                            current_is_dark, container_bg, container_border, text_color, subtext_color, btn_bg, btn_hover_bg, btn_color, text_shadow
                        );

                        RECORDING_WEBVIEW.with(|cell| {
                            if let Some(wv) = cell.borrow().as_ref() {
                                let _ = wv.evaluate_script(&theme_script);
                            }
                        });
                    }
                }
            }
            LRESULT(0)
        }

        WM_APP_REAL_SHOW => {
            if CURRENT_RECORDING_HIDDEN.load(Ordering::SeqCst) {
                return LRESULT(0);
            }
            // Move to Center Screen
            let (ui_width, ui_height) = get_ui_dimensions();
            let screen_x = GetSystemMetrics(SM_CXSCREEN);
            let screen_y = GetSystemMetrics(SM_CYSCREEN);
            let center_x = (screen_x - ui_width) / 2;
            let center_y = (screen_y - ui_height) / 2 + 100;

            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                center_x,
                center_y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );

            // Start Visualization Updates NOW that we are visible and ready
            let _ = SetTimer(Some(hwnd), 1, 16, None);

            // Trigger Fade In
            RECORDING_WEBVIEW.with(|cell| {
                if let Some(wv) = cell.borrow().as_ref() {
                    let _ = wv.evaluate_script(
                        "setTimeout(() => document.body.classList.add('visible'), 50);",
                    );
                }
            });

            LRESULT(0)
        }

        WM_APP_HIDE => {
            // Stop logic
            let _ = KillTimer(Some(hwnd), 1);
            let _ = KillTimer(Some(hwnd), 2);
            let _ = KillTimer(Some(hwnd), 99);

            // Reset opacity
            RECORDING_WEBVIEW.with(|cell| {
                if let Some(wv) = cell.borrow().as_ref() {
                    let _ = wv.evaluate_script("hideState();");
                }
            });

            // Move Off-screen
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                -4000,
                -4000,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );

            RECORDING_STATE.store(1, Ordering::SeqCst); // Back to Warmup/Hidden

            LRESULT(0)
        }

        WM_APP_UPDATE_STATE => {
            // Force an immediate update cycle if needed
            LRESULT(0)
        }

        WM_CLOSE => {
            let is_stop = AUDIO_STOP_SIGNAL.load(Ordering::SeqCst);
            let is_abort = AUDIO_ABORT_SIGNAL.load(Ordering::SeqCst);

            if is_stop || is_abort {
                let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
            } else {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let last = LAST_SHOW_TIME.load(Ordering::SeqCst);
                if now > last && (now - last) < 2000 {
                    // Ignore Close during first 2 seconds if not explicitly stopped
                } else {
                    let _ = PostMessageW(Some(hwnd), WM_APP_HIDE, WPARAM(0), LPARAM(0));
                }
            }
            LRESULT(0)
        }

        WM_USER_FULL_CLOSE => {
            let _ = DestroyWindow(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub unsafe extern "system" fn recording_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kbd = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize {
            if kbd.vkCode == VK_ESCAPE.0 as u32 {
                if super::is_recording_overlay_active() {
                    super::stop_recording_and_submit();
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
