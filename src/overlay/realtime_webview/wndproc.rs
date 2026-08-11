//! Window procedure for the unified realtime compositor host.

use super::controller;
use super::layout::{self, CardRole};
use super::state::*;
use super::webview::{
    resize_to_virtual_desktop, run_all_cards_script, run_card_script, sync_compositor_layout,
    update_card_text, update_theme,
};
use crate::api::realtime_audio::{
    REALTIME_RMS, WM_COPY_TEXT, WM_DOWNLOAD_PROGRESS, WM_EXEC_SCRIPT, WM_MODEL_SWITCH,
    WM_REALTIME_UPDATE, WM_START_DRAG, WM_THEME_UPDATE, WM_TOGGLE_MIC, WM_TOGGLE_TRANS,
    WM_TRANSLATION_UPDATE, WM_UPDATE_TTS_SPEED, WM_VOLUME_UPDATE,
};
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

fn clamp_to_char_boundary(text: &str, index: usize) -> usize {
    let mut clamped = index.min(text.len());
    while clamped > 0 && !text.is_char_boundary(clamped) {
        clamped -= 1;
    }
    clamped
}

fn sync_tts_ui_state() {
    let enabled = REALTIME_TTS_ENABLED.load(Ordering::SeqCst);
    let speed = CURRENT_TTS_SPEED.load(Ordering::Relaxed);
    run_all_cards_script(&format!(
        "if(window.setTtsEnabled) window.setTtsEnabled({enabled});if(window.updateTtsSpeed)window.updateTtsSpeed({speed});"
    ));
}

fn close_card_modals_if_requested() {
    if CLOSE_TTS_MODAL_REQUEST.swap(false, Ordering::SeqCst) {
        run_all_cards_script(
            "for(const id of ['tts-modal','tts-modal-overlay']){const element=document.getElementById(id);if(element)element.classList.remove('show');}",
        );
    }
}

unsafe fn destroy_realtime_overlay_window() {
    unsafe {
        let hwnd = std::ptr::addr_of!(REALTIME_HWND).read();
        if !hwnd.is_invalid() && IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        } else {
            PostQuitMessage(0);
        }
    }
}

pub unsafe extern "system" fn realtime_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_START_DRAG => {
                crate::overlay::utils::begin_window_drag(hwnd);
                LRESULT(0)
            }
            WM_TOGGLE_MIC => {
                let visible = wparam.0 != 0;
                MIC_VISIBLE.store(visible, Ordering::SeqCst);
                layout::set_visible(CardRole::Transcription, visible);
                sync_compositor_layout(hwnd);
                LRESULT(0)
            }
            WM_TOGGLE_TRANS => {
                let visible = wparam.0 != 0;
                TRANS_VISIBLE.store(visible, Ordering::SeqCst);
                layout::set_visible(CardRole::Translation, visible);
                sync_compositor_layout(hwnd);
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
                    run_all_cards_script(&Box::from_raw(ptr));
                }
                LRESULT(0)
            }
            WM_REALTIME_UPDATE => {
                close_card_modals_if_requested();
                let (old_text, new_text) = transcription_text();
                sync_tts_ui_state();
                update_card_text(CardRole::Transcription, &old_text, &new_text);
                LRESULT(0)
            }
            WM_TRANSLATION_UPDATE => {
                close_card_modals_if_requested();
                let (is_s2s, old_text, new_text) = translation_text();
                if !is_s2s {
                    controller::process_committed_translation_for_tts(&old_text, hwnd.0 as isize);
                    sync_tts_ui_state();
                }
                update_card_text(CardRole::Translation, &old_text, &new_text);
                LRESULT(0)
            }
            WM_MODEL_SWITCH => {
                let model = if wparam.0 == 1 {
                    "google-gtx"
                } else {
                    "text-llm"
                };
                run_card_script(
                    CardRole::Translation,
                    &format!("if(window.switchModel)window.switchModel('{model}');"),
                );
                LRESULT(0)
            }
            WM_DOWNLOAD_PROGRESS => {
                update_download_modal();
                LRESULT(0)
            }
            WM_VOLUME_UPDATE => {
                let rms = f32::from_bits(REALTIME_RMS.load(Ordering::Relaxed));
                run_card_script(
                    CardRole::Transcription,
                    &format!("if(window.updateVolume)window.updateVolume({rms});"),
                );
                LRESULT(0)
            }
            WM_UPDATE_TTS_SPEED => {
                let speed = wparam.0 as u32;
                run_all_cards_script(&format!(
                    "if(window.updateTtsSpeed)window.updateTtsSpeed({speed});"
                ));
                LRESULT(0)
            }
            WM_THEME_UPDATE => {
                update_theme();
                LRESULT(0)
            }
            WM_SIZE => {
                resize_to_virtual_desktop(hwnd);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                resize_host_to_virtual_desktop(hwnd);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = PostMessageW(Some(hwnd), WM_APP_REALTIME_HIDE, WPARAM(0), LPARAM(0));
                LRESULT(0)
            }
            WM_APP_REALTIME_HIDE => {
                if REALTIME_STATE
                    .lock()
                    .is_ok_and(|state| state.is_downloading)
                {
                    crate::api::realtime_audio::cancel_download_and_revert_to_gemini();
                }
                REALTIME_SESSION_STOPPING.store(true, Ordering::SeqCst);
                REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
                crate::api::tts::TTS_MANAGER.stop();
                IS_ACTIVE = false;
                destroy_realtime_overlay_window();
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

fn transcription_text() -> (String, String) {
    let Ok(state) = REALTIME_STATE.lock() else {
        return (String::new(), String::new());
    };
    let full = &state.full_transcript;
    let position = clamp_to_char_boundary(full, state.transcript_committed_pos.min(full.len()));
    join_committed_and_draft(&full[..position], &full[position..])
}

fn translation_text() -> (bool, String, String) {
    let Ok(state) = REALTIME_STATE.lock() else {
        return (false, String::new(), String::new());
    };
    let is_s2s = state.transcription_method
        == crate::api::realtime_audio::TranscriptionMethod::GeminiLiveS2s;
    let (old, new) =
        join_committed_and_draft(&state.committed_translation, &state.uncommitted_translation);
    (is_s2s, old, new)
}

fn join_committed_and_draft(committed: &str, draft: &str) -> (String, String) {
    let old = committed.trim_end();
    let new = draft.trim_start();
    if !old.is_empty() && !new.is_empty() {
        (old.to_string(), format!(" {new}"))
    } else {
        (old.to_string(), new.to_string())
    }
}

fn update_download_modal() {
    let (downloading, title, message, progress) = REALTIME_STATE
        .lock()
        .map(|state| {
            (
                state.is_downloading,
                state.download_title.clone(),
                state.download_message.clone(),
                state.download_progress,
            )
        })
        .unwrap_or_default();
    if downloading {
        let title = serde_json::to_string(&title).unwrap_or_else(|_| "\"\"".into());
        let message = serde_json::to_string(&message).unwrap_or_else(|_| "\"\"".into());
        run_card_script(
            CardRole::Transcription,
            &format!(
                "if(window.showDownloadModal)window.showDownloadModal({title},{message},{progress});"
            ),
        );
    } else {
        run_card_script(
            CardRole::Transcription,
            "if(window.hideDownloadModal)window.hideDownloadModal();",
        );
    }
}

unsafe fn resize_host_to_virtual_desktop(hwnd: HWND) {
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE,
        );
        resize_to_virtual_desktop(hwnd);
        sync_compositor_layout(hwnd);
    }
}

#[cfg(test)]
mod tests {
    use super::{clamp_to_char_boundary, join_committed_and_draft};

    #[test]
    fn text_splits_preserve_utf8_boundaries_and_word_spacing() {
        let text = "a한";
        assert_eq!(clamp_to_char_boundary(text, 2), 1);
        assert_eq!(
            join_committed_and_draft("done ", " next"),
            ("done".into(), " next".into())
        );
    }
}
