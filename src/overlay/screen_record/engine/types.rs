use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, LazyLock};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
use std::time::Instant;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows_capture::{
    SendDirectX, graphics_capture_api::InternalCaptureControl, windows_bindings as wc_windows,
};

use super::cursor::load_grab_signatures;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MousePosition {
    pub x: i32,
    pub y: i32,
    pub timestamp: f64,
    pub is_clicked: bool,
    pub cursor_type: String,
    pub capture_width: Option<u32>,
    pub capture_height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
    pub hz: u32,
    /// JPEG data URL captured at call time; filled in by the IPC handler.
    pub thumbnail: Option<String>,
}

#[derive(Clone, Copy)]
pub(crate) struct SystemCursorHandles {
    pub(crate) arrow: isize,
    pub(crate) ibeam: isize,
    pub(crate) wait: isize,
    pub(crate) appstarting: isize,
    pub(crate) cross: isize,
    pub(crate) hand: isize,
    pub(crate) size_all: isize,
    pub(crate) size_ns: isize,
    pub(crate) size_we: isize,
    pub(crate) size_nwse: isize,
    pub(crate) size_nesw: isize,
}

pub(crate) struct VramFrame {
    pub(crate) texture: SendDirectX<ID3D11Texture2D>,
    pub(crate) surface: SendDirectX<wc_windows::Graphics::DirectX::Direct3D11::IDirect3DSurface>,
    pub(crate) in_flight: Arc<AtomicUsize>,
}

// The ring buffer is shared read-only across threads. Actual mutation happens via
// the D3D11 API using the texture handles, coordinated by the capture callback and
// pump index atomics.
unsafe impl Sync for VramFrame {}

pub static MOUSE_POSITIONS: LazyLock<Mutex<VecDeque<MousePosition>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));
pub static IS_RECORDING: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// Stores the last capture-start error so `stop_recording` can report it.
pub static CAPTURE_ERROR: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
pub static SHOULD_STOP: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
pub static SHOULD_STOP_AUDIO: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
pub static ENCODING_FINISHED: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
pub static AUDIO_ENCODING_FINISHED: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
pub static MIC_AUDIO_ENCODING_FINISHED: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(true)));
pub static WEBCAM_ENCODING_FINISHED: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(true)));
pub static ENCODER_ACTIVE: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
pub static ACTIVE_CAPTURE_CONTROL: LazyLock<Mutex<Option<InternalCaptureControl>>> =
    LazyLock::new(|| Mutex::new(None));
// Last emitted cursor debug record to avoid spamming logs every frame.
pub(crate) static LAST_CURSOR_DEBUG: LazyLock<Mutex<Option<(isize, String, bool, String)>>> =
    LazyLock::new(|| Mutex::new(None));
// Learned non-system custom cursor signatures that represent grab/openhand cursors.
// Learned only when unknown cursor appears while clicked=true.
pub(crate) static CUSTOM_GRAB_SIGNATURES: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(load_grab_signatures()));
// Runtime-computed signatures for the current machine's system cursor shapes.
// This catches apps/games that clone a system cursor into a private handle.
pub(crate) static SYSTEM_CURSOR_SIGNATURES: LazyLock<HashMap<String, &'static str>> =
    LazyLock::new(super::cursor::load_system_cursor_signatures);
// Resolve system cursor handles once; avoids repeated LoadCursorW calls per sample.
pub(crate) static SYSTEM_CURSOR_HANDLES: LazyLock<SystemCursorHandles> =
    LazyLock::new(super::cursor::load_system_cursor_handles);
// Cache cursor_signature() results by HCURSOR raw pointer value.
// Windows reuses cursor handles for the lifetime of a process, so a given
// pointer always maps to the same bitmap metadata.  Cleared on recording start.
pub static CURSOR_SIGNATURE_CACHE: LazyLock<Mutex<HashMap<isize, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
// Most recent unknown cursor seen while no mouse button was held. Used to
// safely learn custom drag/grab cursors only when the shape changed under drag.
pub(crate) static LAST_UNKNOWN_RELEASED_SIGNATURE: LazyLock<Mutex<Option<(String, Instant)>>> =
    LazyLock::new(|| Mutex::new(None));
// Set SCREEN_RECORD_CURSOR_DEBUG=1 to enable verbose cursor classification logs.
pub(crate) static CURSOR_DEBUG_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("SCREEN_RECORD_CURSOR_DEBUG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
});

pub static VIDEO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static AUDIO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static MIC_AUDIO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static WEBCAM_VIDEO_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub static MIC_AUDIO_START_OFFSET_MS: AtomicU64 = AtomicU64::new(u64::MAX);
pub static WEBCAM_VIDEO_START_OFFSET_MS: AtomicU64 = AtomicU64::new(u64::MAX);
/// FPS the most recent recording was actually encoded at. Used by stop_recording
/// so the frontend can show the correct "Match Original" option in the export UI.
pub static LAST_RECORDING_FPS: std::sync::Mutex<Option<u32>> = std::sync::Mutex::new(None);
pub static mut MONITOR_X: i32 = 0;
pub static mut MONITOR_Y: i32 = 0;
/// Dynamically track target window so cursor math stays accurate if the window moves.
pub static TARGET_HWND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub static LAST_CAPTURE_FRAME_WIDTH: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
pub static LAST_CAPTURE_FRAME_HEIGHT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub(crate) const DEFAULT_GRAB_SIGNATURE: &str = "hot(13,13)|mask(32x32)|color(32x32)|mono(0)";
pub(crate) const DEFAULT_TARGET_FPS: u32 = 60;
pub(crate) const ENCODER_MAX_PENDING_FRAMES: usize = 12;
pub(crate) const CAPTURE_STATS_WINDOW_SECS: f64 = 1.0;
pub(crate) const CURSOR_SAMPLE_MIN_FPS: u32 = 30;
pub(crate) const CURSOR_SAMPLE_MAX_FPS: u32 = 120;
pub(crate) const CURSOR_GRAB_LEARN_WINDOW_MS: u64 = 1_000;
pub(crate) const NO_READY_VRAM_FRAME: usize = usize::MAX;
pub(crate) const MIN_VALID_WINDOW_FRAME_DIM: u32 = 300;
pub(crate) const WINDOW_CAPTURE_QUEUE_TARGET_MS: usize = 350;
pub(crate) const WINDOW_CAPTURE_MAX_PENDING_FRAMES: usize = 48;
pub(crate) const WINDOW_CAPTURE_VRAM_POOL_MIN_FRAMES: usize = 6;
pub(crate) const WINDOW_CAPTURE_VRAM_POOL_MAX_FRAMES: usize = 12;
