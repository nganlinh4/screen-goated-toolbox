// Staging buffer for chunked IPC transfer of baked export data.
//
// The frontend sends baked camera/cursor/overlay data in small chunks via
// `stage_export_data` IPC calls, then triggers export with a lightweight
// config-only `start_export_server` call. This avoids V8's JSON.stringify
// string length limit for large recordings.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use super::config::{
    AnimatedCursorSlotData, BakedCameraFrame, BakedCursorFrame, BakedWebcamFrame, OverlayFrame,
};
use super::overlay_frames::OverlayAtlasMetadata;

#[derive(Clone)]
pub struct CursorSlotOverride {
    pub slot_id: u32,
    pub rgba: Vec<u8>,
}

pub struct StagedExportData {
    pub camera_frames: Vec<BakedCameraFrame>,
    pub cursor_frames: Vec<BakedCursorFrame>,
    pub webcam_frames: Vec<BakedWebcamFrame>,
    pub cursor_slot_overrides: Vec<CursorSlotOverride>,
    /// Decoded RGBA pixels from the sprite atlas PNG (width × height × 4).
    pub atlas_rgba: Option<Vec<u8>>,
    pub atlas_w: u32,
    pub atlas_h: u32,
    /// Pre-computed overlay quads per output frame (replaces CPU overlay compositing).
    pub overlay_frames: Vec<OverlayFrame>,
    /// Atlas metadata for Rust-side frame quad generation (replaces JS frame loop).
    pub overlay_metadata: Option<OverlayAtlasMetadata>,
}

impl StagedExportData {
    fn new() -> Self {
        Self {
            camera_frames: Vec::new(),
            cursor_frames: Vec::new(),
            webcam_frames: Vec::new(),
            cursor_slot_overrides: Vec::new(),
            atlas_rgba: None,
            atlas_w: 1,
            atlas_h: 1,
            overlay_frames: Vec::new(),
            overlay_metadata: None,
        }
    }
}

static STAGED: Mutex<Option<StagedExportData>> = Mutex::new(None);
static STAGED_SESSIONS: LazyLock<Mutex<HashMap<String, HashMap<String, StagedExportData>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Persistent store for pre-rendered animated cursor frames.
/// Unlike STAGED, this is NOT cleared by `clear_staged()` — the frontend
/// pre-computes these in the background so export has zero additional work.
static ANIMATED_CURSORS: Mutex<Vec<AnimatedCursorSlotData>> = Mutex::new(Vec::new());

/// Clear all staged data (called before a new export session).
/// Does NOT clear animated cursor slots — those are pre-computed and persistent.
pub fn clear_staged() {
    let mut guard = STAGED.lock().unwrap();
    *guard = Some(StagedExportData::new());
}

pub fn clear_session(session_id: &str) {
    STAGED_SESSIONS.lock().unwrap().remove(session_id);
}

pub fn append_camera_frames(frames: Vec<BakedCameraFrame>) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.camera_frames.extend(frames);
}

pub fn append_camera_frames_for(session_id: &str, job_id: &str, frames: Vec<BakedCameraFrame>) {
    let mut guard = STAGED_SESSIONS.lock().unwrap();
    let staged = guard
        .entry(session_id.to_string())
        .or_default()
        .entry(job_id.to_string())
        .or_insert_with(StagedExportData::new);
    staged.camera_frames.extend(frames);
}

pub fn append_cursor_frames(frames: Vec<BakedCursorFrame>) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.cursor_frames.extend(frames);
}

pub fn append_webcam_frames(frames: Vec<BakedWebcamFrame>) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.webcam_frames.extend(frames);
}

pub fn append_webcam_frames_for(session_id: &str, job_id: &str, frames: Vec<BakedWebcamFrame>) {
    let mut guard = STAGED_SESSIONS.lock().unwrap();
    let staged = guard
        .entry(session_id.to_string())
        .or_default()
        .entry(job_id.to_string())
        .or_insert_with(StagedExportData::new);
    staged.webcam_frames.extend(frames);
}

pub fn append_cursor_frames_for(session_id: &str, job_id: &str, frames: Vec<BakedCursorFrame>) {
    let mut guard = STAGED_SESSIONS.lock().unwrap();
    let staged = guard
        .entry(session_id.to_string())
        .or_default()
        .entry(job_id.to_string())
        .or_insert_with(StagedExportData::new);
    staged.cursor_frames.extend(frames);
}

/// Set cursor slot overrides (browser-rasterized tiles from frontend).
/// Replaces previous overrides for this export session.
pub fn set_cursor_slot_overrides(overrides: Vec<CursorSlotOverride>) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.cursor_slot_overrides = overrides;
}

pub fn set_cursor_slot_overrides_for(
    session_id: &str,
    job_id: &str,
    overrides: Vec<CursorSlotOverride>,
) {
    let mut guard = STAGED_SESSIONS.lock().unwrap();
    let staged = guard
        .entry(session_id.to_string())
        .or_default()
        .entry(job_id.to_string())
        .or_insert_with(StagedExportData::new);
    staged.cursor_slot_overrides = overrides;
}

/// Set the sprite atlas (decoded RGBA pixels). Called once per export session.
pub fn set_atlas(rgba: Vec<u8>, w: u32, h: u32) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.atlas_rgba = Some(rgba);
    staged.atlas_w = w;
    staged.atlas_h = h;
}

pub fn set_atlas_for(session_id: &str, job_id: &str, rgba: Vec<u8>, w: u32, h: u32) {
    let mut guard = STAGED_SESSIONS.lock().unwrap();
    let staged = guard
        .entry(session_id.to_string())
        .or_default()
        .entry(job_id.to_string())
        .or_insert_with(StagedExportData::new);
    staged.atlas_rgba = Some(rgba);
    staged.atlas_w = w;
    staged.atlas_h = h;
}

pub fn set_overlay_metadata(meta: OverlayAtlasMetadata) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.overlay_metadata = Some(meta);
}

pub fn set_overlay_metadata_for(session_id: &str, job_id: &str, meta: OverlayAtlasMetadata) {
    let mut guard = STAGED_SESSIONS.lock().unwrap();
    let staged = guard
        .entry(session_id.to_string())
        .or_default()
        .entry(job_id.to_string())
        .or_insert_with(StagedExportData::new);
    staged.overlay_metadata = Some(meta);
}

pub fn append_overlay_frames(frames: Vec<OverlayFrame>) {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    staged.overlay_frames.extend(frames);
}

pub fn append_overlay_frames_for(session_id: &str, job_id: &str, frames: Vec<OverlayFrame>) {
    let mut guard = STAGED_SESSIONS.lock().unwrap();
    let staged = guard
        .entry(session_id.to_string())
        .or_default()
        .entry(job_id.to_string())
        .or_insert_with(StagedExportData::new);
    staged.overlay_frames.extend(frames);
}

/// Store a pre-rendered animated cursor slot persistently.
/// Called from the background pre-staging IPC — survives `clear_staged()`.
pub fn set_animated_cursor_slot(data: AnimatedCursorSlotData) {
    let mut guard = ANIMATED_CURSORS.lock().unwrap();
    if let Some(existing) = guard.iter_mut().find(|s| s.slot_id == data.slot_id) {
        *existing = data;
    } else {
        guard.push(data);
    }
}

/// Clone all pre-rendered animated cursor slots for the export pipeline.
pub fn get_animated_cursor_slots() -> Vec<AnimatedCursorSlotData> {
    ANIMATED_CURSORS.lock().unwrap().clone()
}

/// Take all staged data, leaving None behind. Returns the accumulated data
/// or an empty set if nothing was staged.
pub fn take_staged() -> StagedExportData {
    let mut guard = STAGED.lock().unwrap();
    guard.take().unwrap_or_else(StagedExportData::new)
}

pub fn take_staged_for(session_id: &str, job_id: &str) -> StagedExportData {
    let mut guard = STAGED_SESSIONS.lock().unwrap();
    let Some(session_jobs) = guard.get_mut(session_id) else {
        return StagedExportData::new();
    };
    let staged = session_jobs
        .remove(job_id)
        .unwrap_or_else(StagedExportData::new);
    if session_jobs.is_empty() {
        guard.remove(session_id);
    }
    staged
}
