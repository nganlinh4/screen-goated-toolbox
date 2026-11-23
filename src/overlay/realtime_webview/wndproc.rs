//! Message relay that keeps realtime audio state in the desktop parent.

use std::sync::atomic::Ordering;

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::layout::CardRole;
use super::protocol::DownloadState;
use super::state::*;
use crate::api::realtime_audio::{
    REALTIME_RMS, WM_DOWNLOAD_PROGRESS, WM_MODEL_SWITCH, WM_REALTIME_UPDATE, WM_THEME_UPDATE,
    WM_TRANSLATION_UPDATE, WM_UPDATE_TTS_SPEED, WM_VOLUME_UPDATE,
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
                publish_transcription();
                // Transcript corrections can clear the translation before the next
                // translation result posts its own update.
                speak_committed_translation(hwnd);
                LRESULT(0)
            }
            WM_TRANSLATION_UPDATE => {
                close_card_modals_if_requested();
                if speak_committed_translation(hwnd) {
                    sync_tts_ui_state();
                }
                publish_translation();
                LRESULT(0)
            }
            WM_MODEL_SWITCH => {
                refresh_translation_settings(&REALTIME_SESSION_PUBLICATION, || {
                    super::parent::update_settings();
                });
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
            WM_CLOSE => {
                stop_session();
                LRESULT(0)
            }
            WM_APP_REALTIME_HIDE => {
                dispatch_close(
                    wparam.0,
                    REALTIME_SESSION_ID.load(Ordering::SeqCst),
                    stop_session,
                );
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

fn dispatch_close(tag: usize, current: u64, stop: impl FnOnce()) {
    // Zero is reserved for the pre-existing unconditional non-renderer stop.
    if tag == 0 || u64::try_from(tag).ok() == Some(current) {
        stop();
    }
}

fn refresh_translation_settings(publication: &std::sync::Mutex<()>, refresh: impl FnOnce()) {
    let _publication = publication
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    refresh();
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

pub(super) fn post_model_and_download_refresh() {
    let hwnd = unsafe { std::ptr::addr_of!(REALTIME_HWND).read() };
    if hwnd.is_invalid() {
        return;
    }
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_MODEL_SWITCH, WPARAM(0), LPARAM(0));
        let _ = PostMessageW(Some(hwnd), WM_DOWNLOAD_PROGRESS, WPARAM(0), LPARAM(0));
    }
}

fn stop_session() {
    if super::download::current().active {
        super::controller::cancel_download_for_stop();
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

/// Publishes the transcription card's change since its last update. The
/// state lock is held while the parent diffs against the shown text, so the
/// transcript is never copied.
fn publish_transcription() {
    let Ok(state) = REALTIME_STATE.lock() else {
        return;
    };
    let full = &state.full_transcript;
    let position = clamp_to_char_boundary(full, state.transcript_committed_pos);
    super::parent::publish_text(
        CardRole::Transcription,
        &full[..position],
        &full[position..],
    );
}

fn publish_translation() {
    let Ok(state) = REALTIME_STATE.lock() else {
        return;
    };
    super::parent::publish_text(
        CardRole::Translation,
        &state.committed_translation,
        &state.uncommitted_translation,
    );
}

/// Hands committed translation to read-aloud, copying it only while
/// read-aloud is on. Returns false in direct speech (S2S) sessions.
fn speak_committed_translation(hwnd: HWND) -> bool {
    let (revision, committed) = {
        let Ok(state) = REALTIME_STATE.lock() else {
            return true;
        };
        if state.transcription_method
            == crate::api::realtime_audio::TranscriptionMethod::GeminiLiveS2s
        {
            return false;
        }
        if !REALTIME_TTS_ENABLED.load(Ordering::SeqCst) || !TRANS_VISIBLE.load(Ordering::SeqCst) {
            return true;
        }
        (
            state.translation_revision,
            state.committed_translation.trim_end().to_owned(),
        )
    };
    super::controller::process_committed_translation_for_tts(&committed, revision, hwnd.0 as isize);
    true
}

fn clamp_to_char_boundary(text: &str, index: usize) -> usize {
    let mut clamped = index.min(text.len());
    while clamped > 0 && !text.is_char_boundary(clamped) {
        clamped -= 1;
    }
    clamped
}

fn download_state() -> DownloadState {
    super::download::current()
}

#[cfg(test)]
mod tests {
    use super::{clamp_to_char_boundary, dispatch_close, refresh_translation_settings};
    use std::cell::Cell;
    use std::sync::{Arc, Mutex, mpsc};

    #[test]
    fn delayed_renderer_close_cannot_stop_a_replacement_session() {
        let stops = Cell::new(0);
        dispatch_close(7, 8, || stops.set(stops.get() + 1));
        assert_eq!(stops.get(), 0);
        dispatch_close(8, 8, || stops.set(stops.get() + 1));
        assert_eq!(stops.get(), 1);
        dispatch_close(0, 9, || stops.set(stops.get() + 1));
        assert_eq!(stops.get(), 2);
    }

    #[test]
    fn delayed_fallback_notification_refreshes_current_model() {
        let publication = Arc::new(Mutex::new(()));
        let selected = Arc::new(Mutex::new("fallback"));
        let displayed = Arc::new(Mutex::new("initial"));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (deliver_tx, deliver_rx) = mpsc::channel();
        let worker = {
            let publication = publication.clone();
            let selected = selected.clone();
            let displayed = displayed.clone();
            std::thread::spawn(move || {
                ready_tx.send(()).unwrap();
                deliver_rx.recv().unwrap();
                refresh_translation_settings(&publication, || {
                    *displayed.lock().unwrap() = *selected.lock().unwrap();
                });
            })
        };
        ready_rx.recv().unwrap();
        {
            let _publication = publication.lock().unwrap();
            *selected.lock().unwrap() = "user selection";
        }
        deliver_tx.send(()).unwrap();
        worker.join().unwrap();
        assert_eq!(*displayed.lock().unwrap(), "user selection");
    }

    #[test]
    fn text_splits_preserve_utf8_boundaries() {
        assert_eq!(clamp_to_char_boundary("a한", 2), 1);
        assert_eq!(clamp_to_char_boundary("a한", 9), 4);
    }
}
