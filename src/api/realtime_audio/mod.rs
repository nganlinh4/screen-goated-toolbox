//! Real-time audio transcription using Gemini Live API
//!
//! This module handles streaming audio to Gemini's native audio model
//! and receives real-time transcriptions via WebSocket.
//!
//! Translation is handled separately via the centralized realtime translation model mapping
//! every 2 seconds for new sentence chunks.

mod capture;
pub mod kokoro_assets;
pub mod magpie_assets;
pub mod magpie_runtime;
pub mod model_loader;
pub mod offline_asr_commit;
pub mod parakeet;
pub mod parakeet_tdt_assets;
pub mod qwen3;
pub(crate) mod s2s;
pub mod sherpa_onnx;
mod state;
#[cfg(test)]
mod state_tests;
pub mod step_audio_assets;
pub mod step_audio_runtime;
pub mod supertonic_assets;
pub(crate) mod transcript_state;
mod transcription;
mod translation;
pub mod tts_libtorch_runtime;
pub mod tts_libtorch_runtime_assets;
mod utils;
pub mod vieneu_runtime;
pub mod voxtral_assets;
pub mod websocket;

use windows::Win32::UI::WindowsAndMessaging::WM_APP;

// Re-export public items
pub use capture::{concrete_default_output_device, current_input_device_name, start_mic_capture};
pub use state::{RealtimeState, SharedRealtimeState, TranscriptionMethod};
pub use transcription::start_realtime_transcription;
pub use translation::translate_with_google_gtx;

/// Interval for triggering translation (milliseconds)
pub const TRANSLATION_INTERVAL_MS: u64 = 1500;

/// Custom message for updating overlay text
pub const WM_REALTIME_UPDATE: u32 = WM_APP + 200;
pub const WM_TRANSLATION_UPDATE: u32 = WM_APP + 201;
pub const WM_VOLUME_UPDATE: u32 = WM_APP + 202;
pub const WM_MODEL_SWITCH: u32 = WM_APP + 203;
pub const WM_DOWNLOAD_PROGRESS: u32 = WM_APP + 204;
pub const WM_START_DRAG: u32 = WM_APP + 205;
pub const WM_TOGGLE_MIC: u32 = WM_APP + 206;
pub const WM_TOGGLE_TRANS: u32 = WM_APP + 207;
pub const WM_COPY_TEXT: u32 = WM_APP + 208;
pub const WM_EXEC_SCRIPT: u32 = WM_APP + 209;
pub const WM_UPDATE_TTS_SPEED: u32 = WM_APP + 210;
pub const WM_THEME_UPDATE: u32 = WM_APP + 212;

// Shared RMS value for volume visualization
pub static REALTIME_RMS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub static DEVICE_RECONNECT_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Cancel Parakeet download and revert to the default realtime model.
pub fn cancel_download_and_revert_to_gemini() {
    use crate::overlay::realtime_webview::state::{
        NEW_TRANSCRIPTION_MODEL, REALTIME_HWND, REALTIME_STATE, REALTIME_WEBVIEWS,
        TRANSCRIPTION_MODEL_CHANGE,
    };
    use std::sync::atomic::Ordering;

    // 1. Set stop signal to cancel download
    crate::overlay::realtime_webview::state::REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);

    // 2. Clear the download state
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }

    let default_model_id = crate::model_config::DEFAULT_REALTIME_TRANSCRIPTION_MODEL.to_string();

    // 3. Revert transcription model in config
    {
        let mut app = crate::APP.lock().unwrap();
        app.config.realtime_transcription_model = default_model_id.clone();
        crate::config::save_config(&app.config);
    }

    // 4. Signal model change to restart with the default model
    if let Ok(mut model) = NEW_TRANSCRIPTION_MODEL.lock() {
        *model = default_model_id.clone();
    }
    TRANSCRIPTION_MODEL_CHANGE.store(true, Ordering::SeqCst);

    // 5. Update WebView UI to show Gemini icon as active and hide download modal
    unsafe {
        let hwnd = std::ptr::addr_of!(REALTIME_HWND).read();
        if !hwnd.is_invalid() {
            let hwnd_key = hwnd.0 as isize;
            // Hide download modal and update transcription model selection
            let script = r#"
                if(window.hideDownloadModal) window.hideDownloadModal();
                document.querySelectorAll('.trans-model-icon').forEach(icon => {
                    icon.classList.toggle('active', icon.getAttribute('data-value') === 'gemini-3.5-translate');
                });
            "#;
            REALTIME_WEBVIEWS.with(|wvs| {
                if let Some(webview) = wvs.borrow().get(&hwnd_key) {
                    let _ = webview.evaluate_script(script);
                }
            });
        }
    }

    println!("Parakeet download cancelled, reverting to default realtime model");
}
