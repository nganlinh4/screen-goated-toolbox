// Bounded staging buffers for chunked export IPC transfer.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use super::config::{
    AnimatedCursorSlotData, BakedCameraFrame, BakedCursorFrame, BakedWebcamFrame, OverlayFrame,
};
use super::overlay_frames::OverlayAtlasMetadata;

mod accounting;
use accounting::ByteUpdate;

const MAX_STAGED_SESSIONS: usize = 8;
const MAX_JOBS_PER_SESSION: usize = 512;
const MAX_ITEMS_PER_STREAM: usize = 2_000_000;
const MAX_CHUNK_ITEMS: usize = 10_000;
const MAX_ATLAS_PIXELS: usize = 67_108_864;
const MAX_CURSOR_OVERRIDES: usize = 64;
const CURSOR_OVERRIDE_BYTES: usize = 512 * 512 * 4;
const MAX_ANIMATED_CURSOR_SLOTS: usize = 256;
const MAX_ANIMATED_CURSOR_FRAMES: usize = 240;
const MAX_ANIMATED_CURSOR_BYTES: usize = 512 * 1024 * 1024;

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
    pub atlas_rgba: Option<Vec<u8>>,
    pub atlas_w: u32,
    pub atlas_h: u32,
    pub overlay_frames: Vec<OverlayFrame>,
    pub overlay_metadata: Option<OverlayAtlasMetadata>,
    accounted_bytes: usize,
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
            accounted_bytes: 0,
        }
    }
}

static STAGED: Mutex<Option<StagedExportData>> = Mutex::new(None);
static STAGED_SESSIONS: LazyLock<Mutex<HashMap<String, HashMap<String, StagedExportData>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static ANIMATED_CURSORS: Mutex<Vec<AnimatedCursorSlotData>> = Mutex::new(Vec::new());

fn append_bounded<T>(target: &mut Vec<T>, chunk: Vec<T>, label: &str) -> Result<(), String> {
    if chunk.len() > MAX_CHUNK_ITEMS {
        return Err(format!(
            "{label} chunk exceeds the {MAX_CHUNK_ITEMS}-item limit"
        ));
    }
    if target.len().saturating_add(chunk.len()) > MAX_ITEMS_PER_STREAM {
        return Err(format!(
            "{label} exceeds the {MAX_ITEMS_PER_STREAM}-item staging limit"
        ));
    }
    target.extend(chunk);
    Ok(())
}

fn update_staged(
    byte_update: impl FnOnce(&StagedExportData) -> Result<ByteUpdate, String>,
    update: impl FnOnce(&mut StagedExportData) -> Result<(), String>,
) -> Result<(), String> {
    let mut guard = STAGED.lock().unwrap();
    let staged = guard.get_or_insert_with(StagedExportData::new);
    let scoped_bytes = STAGED_SESSIONS
        .lock()
        .unwrap()
        .values()
        .flat_map(HashMap::values)
        .map(|entry| entry.accounted_bytes)
        .try_fold(0usize, usize::checked_add)
        .ok_or_else(|| "Total staged byte accounting overflowed".to_string())?;
    let current_total = staged
        .accounted_bytes
        .checked_add(scoped_bytes)
        .ok_or_else(|| "Total staged byte accounting overflowed".to_string())?;
    let projected =
        accounting::project_legacy(staged.accounted_bytes, current_total, byte_update(staged)?)?;
    update(staged)?;
    staged.accounted_bytes = projected;
    Ok(())
}

fn update_staged_for(
    session_id: &str,
    job_id: &str,
    byte_update: impl FnOnce(&StagedExportData) -> Result<ByteUpdate, String>,
    update: impl FnOnce(&mut StagedExportData) -> Result<(), String>,
) -> Result<(), String> {
    super::validation::validate_identifier(session_id, "session id")?;
    super::validation::validate_identifier(job_id, "job id")?;
    let legacy_bytes = STAGED
        .lock()
        .unwrap()
        .as_ref()
        .map_or(0, |staged| staged.accounted_bytes);
    let mut sessions = STAGED_SESSIONS.lock().unwrap();
    if !sessions.contains_key(session_id) && sessions.len() >= MAX_STAGED_SESSIONS {
        return Err(format!(
            "Too many staged sessions; clear one of the {MAX_STAGED_SESSIONS} existing sessions"
        ));
    }
    let existing_jobs = sessions.get(session_id);
    if existing_jobs
        .is_some_and(|jobs| !jobs.contains_key(job_id) && jobs.len() >= MAX_JOBS_PER_SESSION)
    {
        return Err(format!(
            "Session exceeds the {MAX_JOBS_PER_SESSION}-job staging limit"
        ));
    }
    let empty = StagedExportData::new();
    let existing_job = existing_jobs.and_then(|jobs| jobs.get(job_id));
    let current_job = existing_job.map_or(0, |staged| staged.accounted_bytes);
    let current_session = existing_jobs
        .into_iter()
        .flat_map(HashMap::values)
        .map(|staged| staged.accounted_bytes)
        .try_fold(0usize, usize::checked_add)
        .ok_or_else(|| "Staged session byte accounting overflowed".to_string())?;
    let scoped_bytes = sessions
        .values()
        .flat_map(HashMap::values)
        .map(|staged| staged.accounted_bytes)
        .try_fold(0usize, usize::checked_add)
        .ok_or_else(|| "Total staged byte accounting overflowed".to_string())?;
    let current_total = legacy_bytes
        .checked_add(scoped_bytes)
        .ok_or_else(|| "Total staged byte accounting overflowed".to_string())?;
    let projected = accounting::project_scoped(
        current_job,
        current_session,
        current_total,
        byte_update(existing_job.unwrap_or(&empty))?,
    )?;
    let staged = sessions
        .entry(session_id.to_string())
        .or_default()
        .entry(job_id.to_string())
        .or_insert_with(StagedExportData::new);
    update(staged)?;
    staged.accounted_bytes = projected;
    Ok(())
}

fn validate_overrides(overrides: &[CursorSlotOverride]) -> Result<(), String> {
    if overrides.len() > MAX_CURSOR_OVERRIDES {
        return Err(format!(
            "Cursor overrides exceed the {MAX_CURSOR_OVERRIDES}-slot limit"
        ));
    }
    if overrides
        .iter()
        .any(|override_data| override_data.rgba.len() != CURSOR_OVERRIDE_BYTES)
    {
        return Err("Cursor override must contain one 512x512 RGBA tile".to_string());
    }
    Ok(())
}

fn validate_atlas(rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
    let pixels = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| "Atlas dimensions overflowed".to_string())?;
    let expected = pixels
        .checked_mul(4)
        .ok_or_else(|| "Atlas byte length overflowed".to_string())?;
    if pixels == 0 || pixels > MAX_ATLAS_PIXELS || rgba.len() != expected {
        return Err("Atlas dimensions or RGBA byte length are invalid".to_string());
    }
    Ok(())
}

fn validate_metadata(meta: &OverlayAtlasMetadata) -> Result<(), String> {
    let pixels = u64::from(meta.atlas_width) * u64::from(meta.atlas_height);
    if pixels == 0 || pixels > MAX_ATLAS_PIXELS as u64 {
        return Err("Overlay atlas metadata dimensions are invalid".to_string());
    }
    let item_count = meta
        .text_entries
        .len()
        .saturating_add(meta.keystroke_entries.len())
        .saturating_add(meta.visibility_segments.len())
        .saturating_add(meta.display_events.len())
        .saturating_add(meta.keyboard_start_times.len())
        .saturating_add(meta.keyboard_indices.len())
        .saturating_add(meta.mouse_start_times.len())
        .saturating_add(meta.mouse_indices.len())
        .saturating_add(meta.event_slots.len())
        .saturating_add(meta.event_identities.len())
        .saturating_add(meta.keyboard_slot_representative_widths.len())
        .saturating_add(meta.mouse_slot_representative_widths.len());
    if item_count > MAX_ITEMS_PER_STREAM {
        return Err("Overlay metadata contains too many items".to_string());
    }
    Ok(())
}

pub fn clear_staged() {
    *STAGED.lock().unwrap() = Some(StagedExportData::new());
}

pub fn clear_session(session_id: &str) {
    if super::validation::validate_identifier(session_id, "session id").is_ok() {
        STAGED_SESSIONS.lock().unwrap().remove(session_id);
    }
}

pub fn clear_all() {
    *STAGED.lock().unwrap() = None;
    STAGED_SESSIONS.lock().unwrap().clear();
    ANIMATED_CURSORS.lock().unwrap().clear();
}

pub fn append_camera_frames(frames: Vec<BakedCameraFrame>) -> Result<(), String> {
    let bytes = accounting::camera_frames(&frames)?;
    update_staged(
        |_| Ok(ByteUpdate::append(bytes)),
        |staged| append_bounded(&mut staged.camera_frames, frames, "camera frames"),
    )
}

pub fn append_camera_frames_for(
    session_id: &str,
    job_id: &str,
    frames: Vec<BakedCameraFrame>,
) -> Result<(), String> {
    let bytes = accounting::camera_frames(&frames)?;
    update_staged_for(
        session_id,
        job_id,
        |_| Ok(ByteUpdate::append(bytes)),
        |staged| append_bounded(&mut staged.camera_frames, frames, "camera frames"),
    )
}

pub fn append_cursor_frames(frames: Vec<BakedCursorFrame>) -> Result<(), String> {
    let bytes = accounting::cursor_frames(&frames)?;
    update_staged(
        |_| Ok(ByteUpdate::append(bytes)),
        |staged| append_bounded(&mut staged.cursor_frames, frames, "cursor frames"),
    )
}

pub fn append_cursor_frames_for(
    session_id: &str,
    job_id: &str,
    frames: Vec<BakedCursorFrame>,
) -> Result<(), String> {
    let bytes = accounting::cursor_frames(&frames)?;
    update_staged_for(
        session_id,
        job_id,
        |_| Ok(ByteUpdate::append(bytes)),
        |staged| append_bounded(&mut staged.cursor_frames, frames, "cursor frames"),
    )
}

pub fn append_webcam_frames(frames: Vec<BakedWebcamFrame>) -> Result<(), String> {
    let bytes = accounting::webcam_frames(&frames)?;
    update_staged(
        |_| Ok(ByteUpdate::append(bytes)),
        |staged| append_bounded(&mut staged.webcam_frames, frames, "webcam frames"),
    )
}

pub fn append_webcam_frames_for(
    session_id: &str,
    job_id: &str,
    frames: Vec<BakedWebcamFrame>,
) -> Result<(), String> {
    let bytes = accounting::webcam_frames(&frames)?;
    update_staged_for(
        session_id,
        job_id,
        |_| Ok(ByteUpdate::append(bytes)),
        |staged| append_bounded(&mut staged.webcam_frames, frames, "webcam frames"),
    )
}

pub fn set_cursor_slot_overrides(overrides: Vec<CursorSlotOverride>) -> Result<(), String> {
    validate_overrides(&overrides)?;
    let bytes = accounting::cursor_overrides(&overrides)?;
    update_staged(
        |staged| {
            Ok(ByteUpdate::replace(
                accounting::cursor_overrides(&staged.cursor_slot_overrides)?,
                bytes,
            ))
        },
        |staged| {
            staged.cursor_slot_overrides = overrides;
            Ok(())
        },
    )
}

pub fn set_cursor_slot_overrides_for(
    session_id: &str,
    job_id: &str,
    overrides: Vec<CursorSlotOverride>,
) -> Result<(), String> {
    validate_overrides(&overrides)?;
    let bytes = accounting::cursor_overrides(&overrides)?;
    update_staged_for(
        session_id,
        job_id,
        |staged| {
            Ok(ByteUpdate::replace(
                accounting::cursor_overrides(&staged.cursor_slot_overrides)?,
                bytes,
            ))
        },
        |staged| {
            staged.cursor_slot_overrides = overrides;
            Ok(())
        },
    )
}

pub fn set_atlas(rgba: Vec<u8>, width: u32, height: u32) -> Result<(), String> {
    validate_atlas(&rgba, width, height)?;
    let bytes = rgba.len();
    update_staged(
        |staged| {
            Ok(ByteUpdate::replace(
                staged.atlas_rgba.as_ref().map_or(0, Vec::len),
                bytes,
            ))
        },
        |staged| {
            staged.atlas_rgba = Some(rgba);
            staged.atlas_w = width;
            staged.atlas_h = height;
            Ok(())
        },
    )
}

pub fn set_atlas_for(
    session_id: &str,
    job_id: &str,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<(), String> {
    validate_atlas(&rgba, width, height)?;
    let bytes = rgba.len();
    update_staged_for(
        session_id,
        job_id,
        |staged| {
            Ok(ByteUpdate::replace(
                staged.atlas_rgba.as_ref().map_or(0, Vec::len),
                bytes,
            ))
        },
        |staged| {
            staged.atlas_rgba = Some(rgba);
            staged.atlas_w = width;
            staged.atlas_h = height;
            Ok(())
        },
    )
}

pub fn set_overlay_metadata(meta: OverlayAtlasMetadata) -> Result<(), String> {
    validate_metadata(&meta)?;
    let bytes = accounting::overlay_metadata(&meta)?;
    update_staged(
        |staged| {
            Ok(ByteUpdate::replace(
                staged
                    .overlay_metadata
                    .as_ref()
                    .map(accounting::overlay_metadata)
                    .transpose()?
                    .unwrap_or(0),
                bytes,
            ))
        },
        |staged| {
            staged.overlay_metadata = Some(meta);
            Ok(())
        },
    )
}

pub fn set_overlay_metadata_for(
    session_id: &str,
    job_id: &str,
    meta: OverlayAtlasMetadata,
) -> Result<(), String> {
    validate_metadata(&meta)?;
    let bytes = accounting::overlay_metadata(&meta)?;
    update_staged_for(
        session_id,
        job_id,
        |staged| {
            Ok(ByteUpdate::replace(
                staged
                    .overlay_metadata
                    .as_ref()
                    .map(accounting::overlay_metadata)
                    .transpose()?
                    .unwrap_or(0),
                bytes,
            ))
        },
        |staged| {
            staged.overlay_metadata = Some(meta);
            Ok(())
        },
    )
}

pub fn append_overlay_frames(frames: Vec<OverlayFrame>) -> Result<(), String> {
    if frames.iter().any(|frame| frame.quads.len() > 256) {
        return Err("Overlay frame exceeds the 256-quad limit".to_string());
    }
    let bytes = accounting::overlay_frames(&frames)?;
    update_staged(
        |_| Ok(ByteUpdate::append(bytes)),
        |staged| append_bounded(&mut staged.overlay_frames, frames, "overlay frames"),
    )
}

pub fn append_overlay_frames_for(
    session_id: &str,
    job_id: &str,
    frames: Vec<OverlayFrame>,
) -> Result<(), String> {
    if frames.iter().any(|frame| frame.quads.len() > 256) {
        return Err("Overlay frame exceeds the 256-quad limit".to_string());
    }
    let bytes = accounting::overlay_frames(&frames)?;
    update_staged_for(
        session_id,
        job_id,
        |_| Ok(ByteUpdate::append(bytes)),
        |staged| append_bounded(&mut staged.overlay_frames, frames, "overlay frames"),
    )
}

pub fn set_animated_cursor_slot(data: AnimatedCursorSlotData) -> Result<(), String> {
    if data.slot_id >= MAX_ANIMATED_CURSOR_SLOTS as u32
        || data.frames.len() > MAX_ANIMATED_CURSOR_FRAMES
        || !data.loop_duration.is_finite()
        || data.loop_duration <= 0.0
        || data
            .frames
            .iter()
            .any(|frame| frame.len() != CURSOR_OVERRIDE_BYTES)
    {
        return Err("Animated cursor slot data exceeds supported limits".to_string());
    }
    let mut slots = ANIMATED_CURSORS.lock().unwrap();
    let replaced_bytes = slots
        .iter()
        .find(|slot| slot.slot_id == data.slot_id)
        .map_or(0, |slot| slot.frames.iter().map(Vec::len).sum());
    let current_bytes: usize = slots
        .iter()
        .flat_map(|slot| &slot.frames)
        .map(Vec::len)
        .sum();
    let incoming_bytes: usize = data.frames.iter().map(Vec::len).sum();
    if incoming_bytes > MAX_ANIMATED_CURSOR_BYTES {
        return Err("Animated cursor cache exceeds the 512 MiB limit".to_string());
    }
    if let Some(index) = slots.iter().position(|slot| slot.slot_id == data.slot_id) {
        slots.remove(index);
    }
    let mut retained_bytes = current_bytes.saturating_sub(replaced_bytes);
    while retained_bytes.saturating_add(incoming_bytes) > MAX_ANIMATED_CURSOR_BYTES {
        if slots.is_empty() {
            return Err("Animated cursor cache cannot free enough space".to_string());
        }
        let removed = slots.remove(0);
        retained_bytes = retained_bytes.saturating_sub(removed.frames.iter().map(Vec::len).sum());
    }
    slots.push(data);
    Ok(())
}

pub fn get_animated_cursor_slots() -> Vec<AnimatedCursorSlotData> {
    ANIMATED_CURSORS.lock().unwrap().clone()
}

pub fn take_staged() -> StagedExportData {
    STAGED
        .lock()
        .unwrap()
        .take()
        .unwrap_or_else(StagedExportData::new)
}

pub fn take_staged_for(session_id: &str, job_id: &str) -> StagedExportData {
    if super::validation::validate_identifier(session_id, "session id").is_err()
        || super::validation::validate_identifier(job_id, "job id").is_err()
    {
        return StagedExportData::new();
    }
    let mut sessions = STAGED_SESSIONS.lock().unwrap();
    let Some(jobs) = sessions.get_mut(session_id) else {
        return StagedExportData::new();
    };
    let staged = jobs.remove(job_id).unwrap_or_else(StagedExportData::new);
    if jobs.is_empty() {
        sessions.remove(session_id);
    }
    staged
}

#[cfg(test)]
mod tests {
    use super::{MAX_CHUNK_ITEMS, append_camera_frames, clear_all};

    #[test]
    fn rejects_oversized_staging_chunks() {
        clear_all();
        let frames = (0..=MAX_CHUNK_ITEMS)
            .map(|_| super::BakedCameraFrame {
                time: 0.0,
                x: 0.0,
                y: 0.0,
                zoom: 1.0,
            })
            .collect();
        assert!(append_camera_frames(frames).is_err());
        clear_all();
    }
}
