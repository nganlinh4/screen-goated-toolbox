use super::{AiRuntimeStatus, AiRuntimeUi, set_status};

const INSTALL_TITLE: &str = "Installing local AI runtime";

fn post_realtime_download_state(active: bool, title: &str, message: &str, progress: f32) {
    use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
    use crate::overlay::realtime_webview::state::{REALTIME_HWND, REALTIME_STATE};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = active;
        state.download_title = title.to_string();
        state.download_message = message.to_string();
        state.download_progress = progress;
    }

    unsafe {
        if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
            let _ = PostMessageW(
                Some(REALTIME_HWND),
                WM_DOWNLOAD_PROGRESS,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

pub(super) fn update_progress(ui: AiRuntimeUi, label: &str, progress: f32) {
    set_status(AiRuntimeStatus::Installing {
        label: label.to_string(),
        progress,
    });

    match ui {
        AiRuntimeUi::None => {}
        AiRuntimeUi::RealtimeOverlay => {
            post_realtime_download_state(true, INSTALL_TITLE, label, progress);
        }
        AiRuntimeUi::Badge => {
            crate::overlay::auto_copy_badge::show_progress_notification(
                INSTALL_TITLE,
                label,
                progress,
            );
        }
    }
}

pub(super) fn clear_progress(ui: AiRuntimeUi) {
    match ui {
        AiRuntimeUi::None => {}
        AiRuntimeUi::RealtimeOverlay => {
            post_realtime_download_state(false, "", "", 0.0);
        }
        AiRuntimeUi::Badge => {
            crate::overlay::auto_copy_badge::hide_progress_notification();
        }
    }
}
