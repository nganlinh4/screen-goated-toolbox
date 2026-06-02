use base64::Engine as _;

use crate::overlay::screen_record::native_export;

pub(super) fn handle_stage_export_data(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let data_type = args["dataType"].as_str().ok_or("missing dataType")?;
    let session_id = args["sessionId"].as_str();
    let job_id = args["jobId"].as_str();
    match data_type {
        "camera" => {
            let frames: Vec<native_export::config::BakedCameraFrame> =
                serde_json::from_value(args["data"].clone())
                    .map_err(|e| format!("bad camera chunk: {e}"))?;
            if let (Some(session_id), Some(job_id)) = (session_id, job_id) {
                native_export::staging::append_camera_frames_for(session_id, job_id, frames);
            } else {
                native_export::staging::append_camera_frames(frames);
            }
        }
        "cursor" => {
            let frames: Vec<native_export::config::BakedCursorFrame> =
                serde_json::from_value(args["data"].clone())
                    .map_err(|e| format!("bad cursor chunk: {e}"))?;
            if let (Some(session_id), Some(job_id)) = (session_id, job_id) {
                native_export::staging::append_cursor_frames_for(session_id, job_id, frames);
            } else {
                native_export::staging::append_cursor_frames(frames);
            }
        }
        "webcam" => {
            let frames: Vec<native_export::config::BakedWebcamFrame> =
                serde_json::from_value(args["data"].clone())
                    .map_err(|e| format!("bad webcam chunk: {e}"))?;
            if let (Some(session_id), Some(job_id)) = (session_id, job_id) {
                native_export::staging::append_webcam_frames_for(session_id, job_id, frames);
            } else {
                native_export::staging::append_webcam_frames(frames);
            }
        }
        "atlas" => {
            let b64 = args["base64"].as_str().ok_or("missing base64")?;
            let w = args["width"].as_u64().unwrap_or(1) as u32;
            let h = args["height"].as_u64().unwrap_or(1) as u32;
            let raw = base64::engine::general_purpose::STANDARD
                .decode(b64.trim_start_matches("data:image/png;base64,"))
                .map_err(|e| e.to_string())?;
            let img = image::load_from_memory(&raw)
                .map_err(|e| e.to_string())?
                .to_rgba8();
            if let (Some(session_id), Some(job_id)) = (session_id, job_id) {
                native_export::staging::set_atlas_for(session_id, job_id, img.into_raw(), w, h);
            } else {
                native_export::staging::set_atlas(img.into_raw(), w, h);
            }
        }
        "overlay_frames_chunk" => {
            let frames: Vec<native_export::config::OverlayFrame> =
                serde_json::from_value(args["data"].clone())
                    .map_err(|e| format!("bad overlay chunk: {e}"))?;
            if let (Some(session_id), Some(job_id)) = (session_id, job_id) {
                native_export::staging::append_overlay_frames_for(session_id, job_id, frames);
            } else {
                native_export::staging::append_overlay_frames(frames);
            }
        }
        "overlay_atlas_metadata" => {
            let meta: native_export::overlay_frames::OverlayAtlasMetadata =
                serde_json::from_value(args["data"].clone())
                    .map_err(|e| format!("bad overlay metadata: {e}"))?;
            if let (Some(session_id), Some(job_id)) = (session_id, job_id) {
                native_export::staging::set_overlay_metadata_for(session_id, job_id, meta);
            } else {
                native_export::staging::set_overlay_metadata(meta);
            }
        }
        "cursor_slots_png" => handle_cursor_slots_png(args, session_id, job_id)?,
        _ => return Err(format!("unknown stage dataType: {data_type}")),
    }
    Ok(serde_json::Value::Null)
}

fn handle_cursor_slots_png(
    args: &serde_json::Value,
    session_id: Option<&str>,
    job_id: Option<&str>,
) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SlotPng {
        slot_id: u32,
        png_base64: String,
    }

    let entries: Vec<SlotPng> = serde_json::from_value(args["data"].clone())
        .map_err(|e| format!("bad cursor_slots_png payload: {e}"))?;
    const CURSOR_TILE_SIZE: u32 = 512;
    let mut overrides = Vec::with_capacity(entries.len());

    for entry in entries {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(
                entry
                    .png_base64
                    .trim_start_matches("data:image/png;base64,"),
            )
            .map_err(|e| format!("cursor slot {} b64: {e}", entry.slot_id))?;
        let img = image::load_from_memory(&raw)
            .map_err(|e| format!("cursor slot {} png: {e}", entry.slot_id))?
            .to_rgba8();
        if img.width() != CURSOR_TILE_SIZE || img.height() != CURSOR_TILE_SIZE {
            return Err(format!(
                "cursor slot {} tile must be {}x{}, got {}x{}",
                entry.slot_id,
                CURSOR_TILE_SIZE,
                CURSOR_TILE_SIZE,
                img.width(),
                img.height()
            ));
        }
        overrides.push(native_export::staging::CursorSlotOverride {
            slot_id: entry.slot_id,
            rgba: img.into_raw(),
        });
    }
    if let (Some(session_id), Some(job_id)) = (session_id, job_id) {
        native_export::staging::set_cursor_slot_overrides_for(session_id, job_id, overrides);
    } else {
        native_export::staging::set_cursor_slot_overrides(overrides);
    }
    Ok(())
}
