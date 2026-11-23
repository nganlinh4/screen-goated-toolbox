//! Real-time audio transcription using Gemini Live API
//!
//! This module handles streaming audio to Gemini's native audio model
//! and receives real-time transcriptions via WebSocket.
//!
//! Translation is handled separately via the centralized realtime translation model mapping
//! every 2 seconds for new sentence chunks.

#[cfg(test)]
mod cancel_tests;
mod cancel_transition;
use cancel_transition::commit_download_cancel;
pub(crate) use cancel_transition::renderer_cancel_is_current;
mod capture;
pub(crate) mod s2s;
mod state;
#[cfg(test)]
mod state_tests;
mod transcription;
mod translation;
mod utils;
pub mod websocket;

use windows::Win32::UI::WindowsAndMessaging::WM_APP;

// Re-export public items
pub use capture::{concrete_default_output_device, current_input_device_name, start_mic_capture};
pub use state::{RealtimeState, SharedRealtimeState, TranscriptionMethod};
pub(crate) use transcription::dedicated::{set_vocabulary, vocabulary_snapshot};
pub use transcription::{RealtimeSessionPlan, start_realtime_transcription};
pub use translation::translate_with_google_gtx;

/// Interval for triggering translation (milliseconds)
pub const TRANSLATION_INTERVAL_MS: u64 = 1500;

/// Custom message for updating overlay text
pub const WM_REALTIME_UPDATE: u32 = WM_APP + 200;
pub const WM_TRANSLATION_UPDATE: u32 = WM_APP + 201;
pub const WM_VOLUME_UPDATE: u32 = WM_APP + 202;
pub const WM_MODEL_SWITCH: u32 = WM_APP + 203;
pub const WM_DOWNLOAD_PROGRESS: u32 = WM_APP + 204;
pub const WM_UPDATE_TTS_SPEED: u32 = WM_APP + 210;
pub const WM_THEME_UPDATE: u32 = WM_APP + 212;

// Shared RMS value for volume visualization
pub static REALTIME_RMS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub static DEVICE_RECONNECT_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Cancel an in-overlay engine download and revert to the default realtime model.
pub(crate) fn cancel_download_and_revert_to_gemini(admit: impl FnOnce() -> bool) -> bool {
    use crate::overlay::realtime_webview::state::{
        NEW_TRANSCRIPTION_MODEL, REALTIME_SESSION_PUBLICATION, REALTIME_STATE,
        TRANSCRIPTION_ENGINE_REVISION, TRANSCRIPTION_MODEL_CHANGE, TRANSCRIPTION_TRANSITION,
        TRANSLATION_OPTIONS_REVISION, set_current_stop_signal_locked,
    };
    let transition = TRANSCRIPTION_TRANSITION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let default_model_id = crate::model_config::DEFAULT_REALTIME_TRANSCRIPTION_MODEL.to_string();
    let mut tts_epoch = None;
    if !commit_download_cancel(
        &REALTIME_SESSION_PUBLICATION,
        &TRANSCRIPTION_ENGINE_REVISION,
        &TRANSLATION_OPTIONS_REVISION,
        &TRANSCRIPTION_MODEL_CHANGE,
        admit,
        || {
            tts_epoch = Some(crate::api::tts::TTS_MANAGER.realtime_epoch());
            set_current_stop_signal_locked(true);
        },
        || {
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.is_downloading = false;
            }
            if let Ok(mut app) = crate::APP.lock() {
                app.config.realtime_transcription_model = default_model_id.clone();
            }
            if let Ok(mut model) = NEW_TRANSCRIPTION_MODEL.lock() {
                *model = default_model_id.clone();
            }
        },
    ) {
        return false;
    }
    crate::overlay::realtime_webview::download::notify();
    let retired = tts_epoch
        .and_then(crate::overlay::realtime_webview::controller::retire_read_playback_if_epoch)
        .unwrap_or_default();
    drop(transition);
    crate::overlay::realtime_webview::controller::notify_read_retired(retired);
    if let Ok(app) = crate::APP.lock() {
        crate::config::save_config(&app.config);
    }

    crate::log_info!("[ModelDownload] cancellation requested; reverting to default realtime model");
    true
}
