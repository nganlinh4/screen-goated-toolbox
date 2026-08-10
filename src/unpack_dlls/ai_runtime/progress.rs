use super::AiRuntimeUi;

#[cfg(not(feature = "recorder-worker"))]
static BADGE_PROGRESS: std::sync::LazyLock<
    std::sync::Mutex<Option<crate::overlay::auto_copy_badge::DownloadProgressBadge>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

#[cfg(not(feature = "recorder-worker"))]
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

pub(super) fn update_progress(ui: AiRuntimeUi, _label: &str, _progress: f32) {
    #[cfg(not(feature = "recorder-worker"))]
    let badge = crate::overlay::auto_copy_badge::locale_text();
    match ui {
        AiRuntimeUi::None => {}
        #[cfg(not(feature = "recorder-worker"))]
        AiRuntimeUi::RealtimeOverlay => {
            post_realtime_download_state(
                true,
                badge.installing_local_ai_runtime,
                _label,
                _progress,
            );
        }
        #[cfg(not(feature = "recorder-worker"))]
        AiRuntimeUi::Badge => {
            if let Ok(mut active) = BADGE_PROGRESS.lock() {
                let progress = active.get_or_insert_with(|| {
                    crate::overlay::auto_copy_badge::DownloadProgressBadge::with_text(
                        badge.installing_local_ai_runtime,
                        _label,
                    )
                });
                progress.report(_progress.clamp(0.0, 100.0).round() as u64, 100);
            }
        }
    }
}

pub(super) fn clear_progress(ui: AiRuntimeUi) {
    match ui {
        AiRuntimeUi::None => {}
        #[cfg(not(feature = "recorder-worker"))]
        AiRuntimeUi::RealtimeOverlay => {
            post_realtime_download_state(false, "", "", 0.0);
        }
        #[cfg(not(feature = "recorder-worker"))]
        AiRuntimeUi::Badge => {
            if let Ok(mut active) = BADGE_PROGRESS.lock()
                && let Some(progress) = active.take()
            {
                progress.finish();
            }
        }
    }
}
