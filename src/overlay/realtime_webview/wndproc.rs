//! Message relay that keeps realtime audio state in the desktop parent.

use std::sync::atomic::Ordering;

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::layout::CardRole;
use super::protocol::{CardText, DownloadState, HostCommand};
use super::state::*;
use crate::api::realtime_audio::{
    REALTIME_RMS, WM_COPY_TEXT, WM_DOWNLOAD_PROGRESS, WM_EXEC_SCRIPT, WM_MODEL_SWITCH,
    WM_REALTIME_UPDATE, WM_THEME_UPDATE, WM_TOGGLE_MIC, WM_TOGGLE_TRANS, WM_TRANSLATION_UPDATE,
    WM_UPDATE_TTS_SPEED, WM_VOLUME_UPDATE,
};

pub unsafe extern "system" fn realtime_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_REALTIME_UPDATE => {
                close_card_modals_if_requested();
                sync_tts_ui_state();
                super::parent::update_text(CardRole::Transcription, transcription_text());
                LRESULT(0)
            }
            WM_TRANSLATION_UPDATE => {
                close_card_modals_if_requested();
                let (is_s2s, text) = translation_text();
                if !is_s2s {
                    super::controller::process_committed_translation_for_tts(
                        &text.committed,
                        hwnd.0 as isize,
                    );
                    sync_tts_ui_state();
                }
                super::parent::update_text(CardRole::Translation, text);
                LRESULT(0)
            }
            WM_MODEL_SWITCH => {
                let model = if wparam.0 == 1 {
                    "google-gtx"
                } else {
                    "text-llm"
                };
                super::parent::update_translation_model(model.to_string());
                LRESULT(0)
            }
            WM_DOWNLOAD_PROGRESS => {
                super::parent::update_download(download_state());
                LRESULT(0)
            }
            WM_VOLUME_UPDATE => {
                let rms = f32::from_bits(REALTIME_RMS.load(Ordering::Relaxed));
                super::parent::update_volume(rms);
                LRESULT(0)
            }
            WM_UPDATE_TTS_SPEED => {
                super::parent::update_tts(
                    REALTIME_TTS_ENABLED.load(Ordering::SeqCst),
                    wparam.0 as u32,
                );
                LRESULT(0)
            }
            WM_THEME_UPDATE => {
                let (is_dark, font_size) = crate::APP
                    .lock()
                    .map(|app| {
                        (
                            app.config.theme_mode.is_dark(),
                            app.config.realtime_font_size,
                        )
                    })
                    .unwrap_or((true, 24));
                super::parent::update_theme(is_dark, font_size);
                LRESULT(0)
            }
            WM_EXEC_SCRIPT => {
                let ptr = lparam.0 as *mut String;
                if !ptr.is_null() {
                    super::parent::run_script(None, &Box::from_raw(ptr));
                }
                LRESULT(0)
            }
            WM_COPY_TEXT => {
                let ptr = lparam.0 as *mut String;
                if !ptr.is_null() {
                    crate::overlay::utils::copy_to_clipboard(&Box::from_raw(ptr), hwnd);
                }
                LRESULT(0)
            }
            WM_TOGGLE_MIC => {
                set_visibility(CardRole::Transcription, wparam.0 != 0);
                LRESULT(0)
            }
            WM_TOGGLE_TRANS => {
                set_visibility(CardRole::Translation, wparam.0 != 0);
                LRESULT(0)
            }
            WM_CLOSE | WM_APP_REALTIME_HIDE => {
                stop_session();
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

pub(super) fn post_text_refresh(role: CardRole) {
    let hwnd = unsafe { std::ptr::addr_of!(REALTIME_HWND).read() };
    if hwnd.is_invalid() {
        return;
    }
    let message = match role {
        CardRole::Transcription => WM_REALTIME_UPDATE,
        CardRole::Translation => WM_TRANSLATION_UPDATE,
    };
    unsafe {
        let _ = PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0));
    }
}

fn set_visibility(role: CardRole, visible: bool) {
    match role {
        CardRole::Transcription => MIC_VISIBLE.store(visible, Ordering::SeqCst),
        CardRole::Translation => TRANS_VISIBLE.store(visible, Ordering::SeqCst),
    }
    let layout = {
        let mut scene = super::parent::SCENE.lock().unwrap();
        match role {
            CardRole::Transcription => scene.layout.transcription.visible = visible,
            CardRole::Translation => scene.layout.translation.visible = visible,
        }
        scene.layout
    };
    super::parent::send(HostCommand::Layout { layout });
    if !MIC_VISIBLE.load(Ordering::SeqCst) && !TRANS_VISIBLE.load(Ordering::SeqCst) {
        stop_session();
    } else if visible {
        post_text_refresh(role);
    }
}

fn stop_session() {
    if REALTIME_STATE
        .lock()
        .is_ok_and(|state| state.is_downloading)
    {
        crate::api::realtime_audio::cancel_download_and_revert_to_gemini();
    }
    super::controller::stop_runtime_flags();
    super::manager::finish_stop();
}

fn sync_tts_ui_state() {
    super::parent::update_tts(
        REALTIME_TTS_ENABLED.load(Ordering::SeqCst),
        CURRENT_TTS_SPEED.load(Ordering::Relaxed),
    );
}

fn close_card_modals_if_requested() {
    if CLOSE_TTS_MODAL_REQUEST.swap(false, Ordering::SeqCst) {
        super::parent::run_script(
            None,
            "for(const id of ['tts-modal','tts-modal-overlay']){const element=document.getElementById(id);if(element)element.classList.remove('show');}",
        );
    }
}

fn transcription_text() -> CardText {
    let Ok(state) = REALTIME_STATE.lock() else {
        return CardText::default();
    };
    let full = &state.full_transcript;
    let position = clamp_to_char_boundary(full, state.transcript_committed_pos.min(full.len()));
    join_committed_and_draft(&full[..position], &full[position..])
}

fn translation_text() -> (bool, CardText) {
    let Ok(state) = REALTIME_STATE.lock() else {
        return (false, CardText::default());
    };
    let is_s2s = state.transcription_method
        == crate::api::realtime_audio::TranscriptionMethod::GeminiLiveS2s;
    (
        is_s2s,
        join_committed_and_draft(&state.committed_translation, &state.uncommitted_translation),
    )
}

fn join_committed_and_draft(committed: &str, draft: &str) -> CardText {
    let committed = committed.trim_end();
    let draft = draft.trim_start();
    CardText {
        committed: committed.to_string(),
        draft: if !committed.is_empty() && !draft.is_empty() {
            format!(" {draft}")
        } else {
            draft.to_string()
        },
    }
}

fn clamp_to_char_boundary(text: &str, index: usize) -> usize {
    let mut clamped = index.min(text.len());
    while clamped > 0 && !text.is_char_boundary(clamped) {
        clamped -= 1;
    }
    clamped
}

fn download_state() -> DownloadState {
    REALTIME_STATE
        .lock()
        .map(|state| DownloadState {
            active: state.is_downloading,
            title: state.download_title.clone(),
            message: state.download_message.clone(),
            progress: state.download_progress,
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{clamp_to_char_boundary, join_committed_and_draft};

    #[test]
    fn text_splits_preserve_utf8_boundaries_and_word_spacing() {
        assert_eq!(clamp_to_char_boundary("a한", 2), 1);
        let text = join_committed_and_draft("done ", " next");
        assert_eq!(text.committed, "done");
        assert_eq!(text.draft, " next");
    }
}
