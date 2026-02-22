// Staging buffer for chunked IPC transfer of baked export data.
//
// The frontend sends baked camera/cursor/text/keystroke data in small chunks
// via `stage_export_data` IPC calls, then triggers export with a lightweight
// config-only `start_export_server` call. This avoids V8's JSON.stringify
// string length limit for large recordings.

use std::sync::Mutex;

use super::config::{BakedCameraFrame, BakedCursorFrame, BakedKeystrokeOverlay, BakedTextOverlay};

pub struct StagedExportData {
    pub camera_frames: Vec<BakedCameraFrame>,
    pub cursor_frames: Vec<BakedCursorFrame>,
    pub text_overlays: Vec<BakedTextOverlay>,
    pub keystroke_overlays: Vec<BakedKeystrokeOverlay>,
}

impl StagedExportData {
    fn new() -> Self {
        Self {
            camera_frames: Vec::new(),
            cursor_frames: Vec::new(),
            text_overlays: Vec::new(),
            keystroke_overlays: Vec::new(),
        }
    }
}

static STAGED: Mutex<Option<StagedExportData>> = Mutex::new(None);

/// Clear all staged data (called before a new export session).
pub fn clear_staged() {
    let mut guard = STAGED.lock().unwrap();
    *guard = Some(StagedExportData::new());
}

pub fn append_camera_frames(frames: Vec<BakedCameraFrame>) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.camera_frames.extend(frames);
}

pub fn append_cursor_frames(frames: Vec<BakedCursorFrame>) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.cursor_frames.extend(frames);
}

pub fn append_text_overlay(overlay: BakedTextOverlay) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.text_overlays.push(overlay);
}

pub fn append_keystroke_overlay(overlay: BakedKeystrokeOverlay) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.keystroke_overlays.push(overlay);
}

/// Take all staged data, leaving None behind. Returns the accumulated data
/// or an empty set if nothing was staged.
pub fn take_staged() -> StagedExportData {
    let mut guard = STAGED.lock().unwrap();
    guard.take().unwrap_or_else(StagedExportData::new)
}
