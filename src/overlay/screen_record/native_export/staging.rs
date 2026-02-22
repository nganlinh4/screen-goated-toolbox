// Staging buffer for chunked IPC transfer of baked export data.
//
// The frontend sends baked camera/cursor/overlay data in small chunks via
// `stage_export_data` IPC calls, then triggers export with a lightweight
// config-only `start_export_server` call. This avoids V8's JSON.stringify
// string length limit for large recordings.

use std::sync::Mutex;

use super::config::{BakedCameraFrame, BakedCursorFrame, OverlayFrame};

pub struct StagedExportData {
    pub camera_frames: Vec<BakedCameraFrame>,
    pub cursor_frames: Vec<BakedCursorFrame>,
    /// Decoded RGBA pixels from the sprite atlas PNG (width × height × 4).
    pub atlas_rgba: Option<Vec<u8>>,
    pub atlas_w: u32,
    pub atlas_h: u32,
    /// Pre-computed overlay quads per output frame (replaces CPU overlay compositing).
    pub overlay_frames: Vec<OverlayFrame>,
}

impl StagedExportData {
    fn new() -> Self {
        Self {
            camera_frames: Vec::new(),
            cursor_frames: Vec::new(),
            atlas_rgba: None,
            atlas_w: 1,
            atlas_h: 1,
            overlay_frames: Vec::new(),
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

/// Set the sprite atlas (decoded RGBA pixels). Called once per export session.
pub fn set_atlas(rgba: Vec<u8>, w: u32, h: u32) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.atlas_rgba = Some(rgba);
    staged.atlas_w = w;
    staged.atlas_h = h;
}

pub fn append_overlay_frames(frames: Vec<OverlayFrame>) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.overlay_frames.extend(frames);
}

/// Take all staged data, leaving None behind. Returns the accumulated data
/// or an empty set if nothing was staged.
pub fn take_staged() -> StagedExportData {
    let mut guard = STAGED.lock().unwrap();
    guard.take().unwrap_or_else(StagedExportData::new)
}
