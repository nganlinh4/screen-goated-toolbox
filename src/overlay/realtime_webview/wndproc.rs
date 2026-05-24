//! Window procedures for realtime overlay windows

use super::controller;
use super::state::*;
use super::webview::{update_webview_text, update_webview_theme};
use crate::api::realtime_audio::{
    REALTIME_RMS, WM_COPY_TEXT, WM_DOWNLOAD_PROGRESS, WM_EXEC_SCRIPT, WM_MODEL_SWITCH,
    WM_REALTIME_UPDATE, WM_START_DRAG, WM_THEME_UPDATE, WM_TOGGLE_MIC, WM_TOGGLE_TRANS,
    WM_TRANSLATION_UPDATE, WM_UPDATE_TTS_SPEED, WM_VOLUME_UPDATE,
};
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::Rect;

fn clamp_to_char_boundary(text: &str, index: usize) -> usize {
    let mut clamped = index.min(text.len());
    while clamped > 0 && !text.is_char_boundary(clamped) {
        clamped -= 1;
    }
    clamped
}

fn sync_tts_ui_state(hwnd: HWND) {
    let enabled = REALTIME_TTS_ENABLED.load(Ordering::SeqCst);
    let speed = CURRENT_TTS_SPEED.load(Ordering::Relaxed);
    let hwnd_key = hwnd.0 as isize;
    let script = format!(
        "if(window.setTtsEnabled) window.setTtsEnabled({}); if(window.updateTtsSpeed) window.updateTtsSpeed({});",
        if enabled { "true" } else { "false" },
        speed
    );

    REALTIME_WEBVIEWS.with(|wvs| {
        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
            let _ = webview.evaluate_script(&script);
        }
    });
}

unsafe fn destroy_realtime_overlay_windows() {
    unsafe {
        let main_hwnd = std::ptr::addr_of!(REALTIME_HWND).read();
        let translation_hwnd = std::ptr::addr_of!(TRANSLATION_HWND).read();

        if !translation_hwnd.is_invalid() && IsWindow(Some(translation_hwnd)).as_bool() {
            let _ = DestroyWindow(translation_hwnd);
        }
        if !main_hwnd.is_invalid() && IsWindow(Some(main_hwnd)).as_bool() {
            let _ = DestroyWindow(main_hwnd);
        } else {
            PostQuitMessage(0);
        }
    }
}

pub unsafe extern "system" fn realtime_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_START_DRAG => {
                let _ = ReleaseCapture();
                let _ = SendMessageW(
                    hwnd,
                    WM_NCLBUTTONDOWN,
                    Some(WPARAM(HTCAPTION as usize)),
                    Some(LPARAM(0)),
                );
                LRESULT(0)
            }
            WM_TOGGLE_MIC => {
                let val = wparam.0 != 0;
                MIC_VISIBLE.store(val, Ordering::SeqCst);
                LRESULT(0)
            }
            WM_TOGGLE_TRANS => {
                let val = wparam.0 != 0;
                TRANS_VISIBLE.store(val, Ordering::SeqCst);
                LRESULT(0)
            }
            WM_COPY_TEXT => {
                let ptr = lparam.0 as *mut String;
                if !ptr.is_null() {
                    let text = Box::from_raw(ptr);
                    crate::overlay::utils::copy_to_clipboard(&text, hwnd);
                }
                LRESULT(0)
            }
            WM_EXEC_SCRIPT => {
                let ptr = lparam.0 as *mut String;
                if !ptr.is_null() {
                    let script_box = Box::from_raw(ptr);
                    let script = *script_box;
                    let hwnd_key = hwnd.0 as isize;
                    REALTIME_WEBVIEWS.with(|wvs| {
                        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                            let _ = webview.evaluate_script(&script);
                        }
                    });
                }
                LRESULT(0)
            }
            WM_REALTIME_UPDATE => {
                // Check if we need to close the modal (flag set by app selection)
                if CLOSE_TTS_MODAL_REQUEST.load(Ordering::SeqCst)
                    && CLOSE_TTS_MODAL_REQUEST.swap(false, Ordering::SeqCst)
                {
                    let hwnd_key = hwnd.0 as isize;
                    let script = "var m = document.getElementById('tts-modal'); if(m) m.classList.remove('show'); var o = document.getElementById('tts-modal-overlay'); if(o) o.classList.remove('show');";
                    REALTIME_WEBVIEWS.with(|wvs| {
                        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                            let _ = webview.evaluate_script(script);
                        }
                    });
                }

                // Get old (committed) and new (current sentence) text from state
                let (old_text, new_text) = {
                    if let Ok(state) = REALTIME_STATE.lock() {
                        // Everything before transcript_committed_pos is "old"
                        // Everything after is "new" (current sentence)
                        let full = &state.full_transcript;
                        let pos = clamp_to_char_boundary(
                            full,
                            state.transcript_committed_pos.min(full.len()),
                        );
                        let old_raw = &full[..pos];
                        let new_raw = &full[pos..];

                        let old = old_raw.trim_end();
                        let new = new_raw.trim_start();
                        if !old.is_empty() && !new.is_empty() {
                            (old.to_string(), format!(" {}", new))
                        } else {
                            (old.to_string(), new.to_string())
                        }
                    } else {
                        (String::new(), String::new())
                    }
                };
                sync_tts_ui_state(hwnd);
                update_webview_text(hwnd, &old_text, &new_text);
                LRESULT(0)
            }
            WM_DOWNLOAD_PROGRESS => {
                let (is_downloading, title, message, progress) = {
                    if let Ok(state) = REALTIME_STATE.lock() {
                        (
                            state.is_downloading,
                            state.download_title.clone(),
                            state.download_message.clone(),
                            state.download_progress,
                        )
                    } else {
                        (false, String::new(), String::new(), 0.0)
                    }
                };

                if is_downloading {
                    let script = format!(
                        "if(window.showDownloadModal) window.showDownloadModal('{}', '{}', {});",
                        title.replace("'", "\\'"),
                        message.replace("'", "\\'"),
                        progress
                    );
                    let hwnd_key = hwnd.0 as isize;
                    REALTIME_WEBVIEWS.with(|wvs| {
                        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                            let _ = webview.evaluate_script(&script);
                        }
                    });
                } else {
                    let script = "if(window.hideDownloadModal) window.hideDownloadModal();";
                    let hwnd_key = hwnd.0 as isize;
                    REALTIME_WEBVIEWS.with(|wvs| {
                        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                            let _ = webview.evaluate_script(script);
                        }
                    });
                }

                LRESULT(0)
            }
            WM_VOLUME_UPDATE => {
                // Read RMS from shared atomic and update visualizer
                let rms_bits = REALTIME_RMS.load(Ordering::Relaxed);
                let rms = f32::from_bits(rms_bits);

                let hwnd_key = hwnd.0 as isize;
                let script = format!("if(window.updateVolume) window.updateVolume({});", rms);

                REALTIME_WEBVIEWS.with(|wvs| {
                    if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                        let _ = webview.evaluate_script(&script);
                    }
                });
                LRESULT(0)
            }
            WM_UPDATE_TTS_SPEED => {
                let speed = wparam.0 as u32;
                let hwnd_key = hwnd.0 as isize;
                let script = format!(
                    "if(window.updateTtsSpeed) window.updateTtsSpeed({});",
                    speed
                );

                REALTIME_WEBVIEWS.with(|wvs| {
                    if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                        let _ = webview.evaluate_script(&script);
                    }
                });
                LRESULT(0)
            }
            WM_THEME_UPDATE => {
                update_webview_theme(hwnd);
                LRESULT(0)
            }
            WM_SIZE => {
                // Resize WebView to match window size
                let width = (lparam.0 & 0xFFFF) as u32;
                let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                let hwnd_key = hwnd.0 as isize;
                REALTIME_WEBVIEWS.with(|wvs| {
                    if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                        let _ = webview.set_bounds(Rect {
                            position: wry::dpi::Position::Physical(
                                wry::dpi::PhysicalPosition::new(0, 0),
                            ),
                            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                                width, height,
                            )),
                        });
                    }
                });
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = PostMessageW(Some(hwnd), WM_APP_REALTIME_HIDE, WPARAM(0), LPARAM(0));
                LRESULT(0)
            }
            WM_APP_REALTIME_HIDE => {
                // Check if download modal is active - if so, user wants to cancel and revert to Gemini
                let is_downloading = {
                    if let Ok(state) = REALTIME_STATE.lock() {
                        state.is_downloading
                    } else {
                        false
                    }
                };

                if is_downloading {
                    // Cancel download and revert to Gemini
                    crate::api::realtime_audio::cancel_download_and_revert_to_gemini();
                }

                // Stop transcription and TTS
                REALTIME_SESSION_STOPPING.store(true, Ordering::SeqCst);
                REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
                crate::api::tts::TTS_MANAGER.stop();

                IS_ACTIVE = false;
                destroy_realtime_overlay_windows();

                LRESULT(0)
            }

            WM_DESTROY => {
                let main_hwnd = std::ptr::addr_of!(REALTIME_HWND).read();
                if hwnd == main_hwnd {
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub unsafe extern "system" fn translation_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_COPY_TEXT => {
                let ptr = lparam.0 as *mut String;
                if !ptr.is_null() {
                    let text = Box::from_raw(ptr);
                    crate::overlay::utils::copy_to_clipboard(&text, hwnd);
                }
                LRESULT(0)
            }
            WM_TRANSLATION_UPDATE => {
                // Check if we need to close the modal (flag set by app selection)
                if CLOSE_TTS_MODAL_REQUEST.load(Ordering::SeqCst)
                    && CLOSE_TTS_MODAL_REQUEST.swap(false, Ordering::SeqCst)
                {
                    let hwnd_key = hwnd.0 as isize;
                    let script = "var m = document.getElementById('tts-modal'); if(m) m.classList.remove('show'); var o = document.getElementById('tts-modal-overlay'); if(o) o.classList.remove('show');";
                    REALTIME_WEBVIEWS.with(|wvs| {
                        if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                            let _ = webview.evaluate_script(script);
                        }
                    });
                }

                let (is_s2s, old_text, new_text): (bool, String, String) = {
                    if let Ok(state) = REALTIME_STATE.lock() {
                        let is_s2s = state.transcription_method
                            == crate::api::realtime_audio::TranscriptionMethod::GeminiLiveS2s;
                        let old = state.committed_translation.trim_end();
                        let new = state.uncommitted_translation.trim_start();
                        if !old.is_empty() && !new.is_empty() {
                            (is_s2s, old.to_string(), format!(" {}", new))
                        } else {
                            (is_s2s, old.to_string(), new.to_string())
                        }
                    } else {
                        (false, String::new(), String::new())
                    }
                };

                if is_s2s {
                    update_webview_text(hwnd, &old_text, &new_text);
                    return LRESULT(0);
                }

                controller::process_committed_translation_for_tts(&old_text, hwnd.0 as isize);
                sync_tts_ui_state(hwnd);
                update_webview_text(hwnd, &old_text, &new_text);
                LRESULT(0)
            }
            WM_MODEL_SWITCH => {
                // Animate the model switch in the UI
                // WPARAM: 0 = text-llm, 1 = google-gtx
                let model_name = match wparam.0 {
                    1 => "google-gtx",
                    _ => "text-llm",
                };
                let hwnd_key = hwnd.0 as isize;
                let script = format!(
                    "if(window.switchModel) window.switchModel('{}');",
                    model_name
                );

                REALTIME_WEBVIEWS.with(|wvs| {
                    if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                        let _ = webview.evaluate_script(&script);
                    }
                });
                LRESULT(0)
            }
            WM_UPDATE_TTS_SPEED => {
                let speed = wparam.0 as u32;
                let hwnd_key = hwnd.0 as isize;
                let script = format!(
                    "if(window.updateTtsSpeed) window.updateTtsSpeed({});",
                    speed
                );

                REALTIME_WEBVIEWS.with(|wvs| {
                    if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                        let _ = webview.evaluate_script(&script);
                    }
                });
                LRESULT(0)
            }
            WM_THEME_UPDATE => {
                update_webview_theme(hwnd);
                LRESULT(0)
            }
            WM_SIZE => {
                // Resize WebView to match window size
                let width = (lparam.0 & 0xFFFF) as u32;
                let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                let hwnd_key = hwnd.0 as isize;
                REALTIME_WEBVIEWS.with(|wvs| {
                    if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                        let _ = webview.set_bounds(Rect {
                            position: wry::dpi::Position::Physical(
                                wry::dpi::PhysicalPosition::new(0, 0),
                            ),
                            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                                width, height,
                            )),
                        });
                    }
                });
                LRESULT(0)
            }

            WM_CLOSE => {
                let _ = PostMessageW(
                    Some(REALTIME_HWND),
                    WM_APP_REALTIME_HIDE,
                    WPARAM(0),
                    LPARAM(0),
                );
                LRESULT(0)
            }
            WM_APP_REALTIME_HIDE => {
                let _ = PostMessageW(
                    Some(REALTIME_HWND),
                    WM_APP_REALTIME_HIDE,
                    WPARAM(0),
                    LPARAM(0),
                );
                LRESULT(0)
            }
            WM_DESTROY => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
