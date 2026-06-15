//! Shared state for realtime transcription overlay

use crate::api::realtime_audio::{RealtimeState, SharedRealtimeState};
pub use crate::win_types::HwndWrapper;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, Once, atomic::AtomicBool};
use windows::Win32::Foundation::*;
pub const WM_APP_REALTIME_START: u32 = 0x0400 + 500; // WM_USER + 500
pub const WM_APP_REALTIME_HIDE: u32 = 0x0400 + 501; // WM_USER + 501

// Gap between realtime and translation overlays
pub const GAP: i32 = 20;

pub static REALTIME_STOP_SIGNAL: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// True while a realtime session is winding down and must not be restarted yet.
pub static REALTIME_SESSION_STOPPING: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// Monotonic realtime session generation. Incrementing invalidates stale backend loops.
pub static REALTIME_SESSION_ID: LazyLock<Arc<std::sync::atomic::AtomicU64>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU64::new(1)));
pub static REALTIME_STATE: LazyLock<SharedRealtimeState> =
    LazyLock::new(|| Arc::new(Mutex::new(RealtimeState::new())));
/// Signal to change audio source (true = restart with new source)
pub static AUDIO_SOURCE_CHANGE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// The new audio source to use ("mic" or "device")
pub static NEW_AUDIO_SOURCE: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
/// Signal to change target language
pub static LANGUAGE_CHANGE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// The new target language to use
pub static NEW_TARGET_LANGUAGE: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
/// Signal to change translation model
pub static TRANSLATION_MODEL_CHANGE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// The new translation model to use ("text-llm" or "google-gtx")
pub static NEW_TRANSLATION_MODEL: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
/// Signal to change transcription model
pub static TRANSCRIPTION_MODEL_CHANGE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// The new transcription model to use ("gemini", "parakeet", or "qwen3-asr-local")
pub static NEW_TRANSCRIPTION_MODEL: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::new()));
/// Visibility state for windows
pub static MIC_VISIBLE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(true)));
pub static TRANS_VISIBLE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(true)));

// --- Per-App Audio Capture State ---
/// Selected app's Process ID for per-app audio capture (0 = not selected / use mic)
pub static SELECTED_APP_PID: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(0)));
/// Selected app's name for display in UI
pub static SELECTED_APP_NAME: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));

// --- Realtime TTS State ---
/// Enable/disable realtime TTS for committed translations
pub static REALTIME_TTS_ENABLED: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// TTS playback speed (100 = 1.0x, 50 = 0.5x, 150 = 1.5x, etc.)
pub static REALTIME_TTS_SPEED: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(100)));
/// Auto-speed mode: automatically adjust speed based on queue length
pub static REALTIME_TTS_AUTO_SPEED: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(true)));
/// Queue of committed translation text segments to speak
pub static COMMITTED_TRANSLATION_QUEUE: LazyLock<Mutex<std::collections::VecDeque<String>>> =
    LazyLock::new(|| Mutex::new(std::collections::VecDeque::new()));
/// Ordered direct S2S audio segments waiting for playback.
pub static REALTIME_S2S_AUDIO_BACKLOG: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(0)));
/// Approximate queued source-audio duration waiting for S2S playback.
pub static REALTIME_S2S_AUDIO_BACKLOG_MS: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(0)));
/// Latest measured queue-to-playback delay for S2S audio.
pub static REALTIME_S2S_AUDIO_DELAY_MS: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(0)));
/// Translated S2S audio already ready after the currently playing ordered segment.
pub static REALTIME_S2S_READY_BACKLOG_MS: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(0)));

/// Track how much of the committed text has been sent to TTS
pub static LAST_SPOKEN_LENGTH: LazyLock<Arc<std::sync::atomic::AtomicUsize>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicUsize::new(0)));
/// Current effective TTS speed (including auto-speed boost) for UI display
pub static CURRENT_TTS_SPEED: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(100)));
/// Signal to close TTS modal (shared between app selection and main window)
pub static CLOSE_TTS_MODAL_REQUEST: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// TTS output volume (0–100, default 100)
pub static CURRENT_TTS_VOLUME: LazyLock<Arc<std::sync::atomic::AtomicU32>> =
    LazyLock::new(|| Arc::new(std::sync::atomic::AtomicU32::new(100)));

pub static mut REALTIME_HWND: HWND = HWND(std::ptr::null_mut());
pub static mut TRANSLATION_HWND: HWND = HWND(std::ptr::null_mut());
pub static mut IS_ACTIVE: bool = false;
pub static mut IS_WARMED_UP: bool = false;
pub static mut IS_INITIALIZING: bool = false;

pub static REGISTER_REALTIME_CLASS: Once = Once::new();
pub static REGISTER_TRANSLATION_CLASS: Once = Once::new();

// Thread-local storage for WebViews
thread_local! {
    pub static REALTIME_WEBVIEWS: std::cell::RefCell<HashMap<isize, wry::WebView>> = std::cell::RefCell::new(HashMap::new());
    // Shared WebContext for this thread using common data directory
    pub static REALTIME_WEB_CONTEXT: std::cell::RefCell<Option<wry::WebContext>> = const { std::cell::RefCell::new(None) };
}

