// --- SCREEN RECORD IPC ---
// IPC command handling for screen recording WebView.
// Routes commands to specialized submodules.

mod cursor_svg;
mod hotkeys;
pub mod media_server;
mod recording;
mod window_monitor;

use super::bg_download;
use super::engine::get_monitors;
use super::mf_decode;
use super::native_export;
use super::raw_video;
use super::{SERVER_PORT, SR_HWND};
use base64::Engine as _;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// Re-exports used by the parent module.
pub use media_server::start_global_media_server;
pub(crate) use window_monitor::capture_window_thumbnail;

pub fn handle_ipc_command(
    cmd: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match cmd.as_str() {
        "check_bg_downloaded" => {
            let id = args["id"].as_str().unwrap_or("");
            let info = bg_download::download_info(id);
            Ok(serde_json::json!({
                "downloaded": info.is_some(),
                "ext": info.as_ref().map(|(ext, _)| ext.clone()),
                "version": info.as_ref().map(|(_, version)| *version)
            }))
        }
        "start_bg_download" => {
            let id = args["id"].as_str().unwrap_or("").to_string();
            let url = args["url"].as_str().unwrap_or("").to_string();
            bg_download::start_download(id, url);
            Ok(serde_json::Value::Null)
        }
        "get_bg_download_progress" => {
            let id = args["id"].as_str().unwrap_or("");
            let status = bg_download::get_download_status(id);
            Ok(serde_json::to_value(&status).unwrap())
        }
        "delete_bg_download" => {
            let id = args["id"].as_str().unwrap_or("");
            bg_download::delete_downloaded(id);
            Ok(serde_json::Value::Null)
        }
        "read_bg_as_data_url" => {
            let id = args["id"].as_str().unwrap_or("");
            match bg_download::read_as_data_url(id) {
                Ok(data_url) => Ok(serde_json::json!(data_url)),
                Err(e) => Err(e),
            }
        }
        "save_uploaded_bg_data_url" => {
            let data_url = args["dataUrl"].as_str().ok_or("Missing dataUrl")?;
            let url = bg_download::save_uploaded_data_url(data_url)?;
            Ok(serde_json::json!(url))
        }
        "prewarm_custom_background" => {
            let url = args["url"].as_str().ok_or("Missing url")?;
            native_export::prewarm_custom_background(url)?;
            Ok(serde_json::Value::Null)
        }
        "log_message" => {
            let msg = args["message"].as_str().unwrap_or("");
            eprintln!("{msg}");
            Ok(serde_json::Value::Null)
        }
        "clear_export_staging" => {
            if let Some(session_id) = args["sessionId"].as_str() {
                native_export::staging::clear_session(session_id);
            } else {
                native_export::staging::clear_staged();
            }
            Ok(serde_json::Value::Null)
        }
        "stage_export_data" => handle_stage_export_data(&args),
        // Check disk cache for pre-rendered animated cursor frames.
        // Returns cached preview PNGs (base64) + populates export store, or null.
        "load_cursor_anim_cache" => {
            let slot_id = args["slotId"].as_u64().ok_or("missing slotId")? as u32;
            let svg_hash = args["svgHash"].as_str().ok_or("missing svgHash")?;
            match native_export::anim_cache::load_cache(slot_id, svg_hash) {
                Some(result) => {
                    let preview_b64: Vec<String> = result
                        .preview_pngs
                        .iter()
                        .map(|png| base64::engine::general_purpose::STANDARD.encode(png))
                        .collect();
                    Ok(serde_json::json!({
                        "cached": true,
                        "loopDuration": result.loop_duration,
                        "naturalWidth": result.natural_width,
                        "naturalHeight": result.natural_height,
                        "previewFrames": preview_b64,
                    }))
                }
                None => Ok(serde_json::json!({ "cached": false })),
            }
        }
        // Save pre-rendered animated cursor frames to disk cache.
        // Also decodes export PNGs to RGBA and populates the persistent export store.
        "save_cursor_anim_cache" => handle_save_cursor_anim_cache(&args),
        "start_export_server" => {
            let result = native_export::start_native_export(args);
            native_export::persist_export_result(&result);
            result
        }
        "start_composition_export_server" => {
            let result = native_export::start_composition_export(args);
            native_export::persist_export_result(&result);
            result
        }
        "get_export_capabilities" => Ok(native_export::get_export_capabilities()),
        "cancel_export" => {
            println!("[Cancel] IPC cancel_export received");
            native_export::cancel_export();
            println!("[Cancel] cancel_export() returned");
            Ok(serde_json::Value::Null)
        }
        "get_default_export_dir" => Ok(serde_json::json!(native_export::get_default_export_dir())),
        "get_media_server_port" => {
            let mut port = SERVER_PORT.load(std::sync::atomic::Ordering::SeqCst);
            if port == 0 {
                port = start_global_media_server().unwrap_or(0);
            }
            Ok(serde_json::json!(port))
        }
        "generate_thumbnails" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let count = args["count"].as_u64().unwrap_or(20) as u32;
            let start = args["start"].as_f64().unwrap_or(0.0);
            let end = args["end"].as_f64().unwrap_or(start);
            let result = super::mf_decode::generate_thumbnails(path, count, start, end)?;
            Ok(serde_json::json!(result))
        }
        "probe_video_metadata" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let result = mf_decode::probe_video_metadata(path)?;
            Ok(serde_json::to_value(result).unwrap())
        }
        "show_in_folder" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            std::process::Command::new("explorer")
                .args(["/select,", &path.replace("/", "\\")])
                .spawn()
                .map_err(|e| e.to_string())?;
            Ok(serde_json::Value::Null)
        }
        "rename_file" => {
            let path_str = args["path"].as_str().ok_or("Missing path")?;
            let new_name = args["newName"].as_str().ok_or("Missing newName")?;
            let path = std::path::PathBuf::from(path_str);
            let new_path = path.with_file_name(new_name);
            std::fs::rename(&path, &new_path).map_err(|e| e.to_string())?;
            Ok(serde_json::json!(new_path.to_string_lossy().to_string()))
        }
        "delete_file" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let _ = std::fs::remove_file(path);
            Ok(serde_json::Value::Null)
        }
        "pick_export_folder" => {
            let initial_dir = args["initialDir"].as_str().map(|s| s.to_string());
            let selected = native_export::pick_export_folder(initial_dir)?;
            Ok(match selected {
                Some(path) => serde_json::json!(path),
                None => serde_json::Value::Null,
            })
        }
        "save_raw_video_copy" => {
            let source_path = args["sourcePath"].as_str().ok_or("Missing sourcePath")?;
            let target_dir = args["targetDir"].as_str().ok_or("Missing targetDir")?;
            let saved_path = raw_video::save_raw_video_copy(source_path, target_dir)?;
            Ok(serde_json::json!({ "savedPath": saved_path }))
        }
        "save_composition_snapshot_copy" => {
            let source_path = args["sourcePath"].as_str().ok_or("Missing sourcePath")?;
            let saved_path = raw_video::save_composition_snapshot_copy(source_path)?;
            Ok(serde_json::json!({ "savedPath": saved_path }))
        }
        "move_saved_raw_video" => {
            let current_path = args["currentPath"].as_str().ok_or("Missing currentPath")?;
            let target_dir = args["targetDir"].as_str().ok_or("Missing targetDir")?;
            let saved_path = raw_video::move_saved_raw_video(current_path, target_dir)?;
            Ok(serde_json::json!({ "savedPath": saved_path }))
        }
        "copy_video_file_to_clipboard" => {
            let file_path = args["filePath"].as_str().ok_or("Missing filePath")?;
            raw_video::copy_video_file_to_clipboard(file_path)?;
            Ok(serde_json::Value::Null)
        }
        "apply_cursor_svg_adjustment" => {
            let src = args["src"].as_str().ok_or("Missing src")?;
            let scale = args["scale"].as_f64().ok_or("Missing scale")? as f32;
            let offset_x = args["offsetX"].as_f64().ok_or("Missing offsetX")? as f32;
            let offset_y = args["offsetY"].as_f64().ok_or("Missing offsetY")? as f32;

            let files = cursor_svg::apply_cursor_svg_adjustment(src, scale, offset_x, offset_y)?;
            Ok(serde_json::json!({
                "ok": true,
                "filesUpdated": files,
            }))
        }
        "get_monitors" => {
            let mut monitors = get_monitors();
            for m in &mut monitors {
                m.thumbnail = window_monitor::capture_monitor_thumbnail(
                    m.x,
                    m.y,
                    m.width as i32,
                    m.height as i32,
                );
            }
            Ok(serde_json::to_value(monitors).unwrap())
        }
        "get_windows" => Ok(serde_json::Value::Array(
            window_monitor::gather_window_infos()?,
        )),
        "show_window_selector" => {
            // Fast: gather metadata only (no thumbnails) so the overlay opens instantly.
            let window_infos = window_monitor::gather_window_metadata()?;
            let (is_dark, lang) = {
                let app = crate::APP.lock().unwrap();
                let is_dark = match app.config.theme_mode {
                    crate::config::ThemeMode::Dark => true,
                    crate::config::ThemeMode::Light => false,
                    crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
                };
                let lang = app.config.ui_language.clone();
                (is_dark, lang)
            };
            super::window_selection::show_window_selector(window_infos.clone(), is_dark, lang);

            // Lazy-load thumbnails in background — push them one-by-one after UI appears.
            std::thread::spawn(move || {
                // Wait for the WebView to finish its entrance animation before flooding it.
                std::thread::sleep(std::time::Duration::from_millis(280));
                for win in &window_infos {
                    if super::window_selection::selector_is_closed() {
                        break;
                    }
                    let hwnd_val = win["id"]
                        .as_str()
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    if hwnd_val == 0 {
                        continue;
                    }
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    if let Some(data_url) = capture_window_thumbnail(hwnd) {
                        super::window_selection::post_thumbnail_update(hwnd_val, data_url);
                    }
                }
            });

            Ok(serde_json::Value::Null)
        }
        "show_recording_audio_app_selector" => {
            let (is_dark, lang) = {
                let app = crate::APP.lock().unwrap();
                let is_dark = match app.config.theme_mode {
                    crate::config::ThemeMode::Dark => true,
                    crate::config::ThemeMode::Light => false,
                    crate::config::ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
                };
                let lang = app.config.ui_language.clone();
                (is_dark, lang)
            };
            super::audio_source_selection::show_audio_app_selector(is_dark, lang);
            Ok(serde_json::Value::Null)
        }
        "start_recording" => recording::handle_start_recording(&args),
        "stop_recording" => recording::handle_stop_recording(),
        "get_hotkeys" => hotkeys::handle_get_hotkeys(),
        "remove_hotkey" => hotkeys::handle_remove_hotkey(&args),
        "set_hotkey" => hotkeys::handle_set_hotkey(&args),
        "unregister_hotkeys" => hotkeys::handle_unregister_hotkeys(),
        "register_hotkeys" => hotkeys::handle_register_hotkeys(),
        "restore_window" => {
            unsafe {
                let hwnd = std::ptr::addr_of!(SR_HWND).read();
                if !hwnd.is_invalid() {
                    if IsIconic(hwnd.0).as_bool() {
                        let _ = ShowWindow(hwnd.0, SW_RESTORE);
                    } else {
                        let _ = ShowWindow(hwnd.0, SW_SHOW);
                    }
                    let _ = SetForegroundWindow(hwnd.0);
                }
            }
            Ok(serde_json::Value::Null)
        }
        "minimize_window" => {
            unsafe {
                let hwnd = std::ptr::addr_of!(SR_HWND).read();
                if !hwnd.is_invalid() {
                    let _ = ShowWindow(hwnd.0, SW_MINIMIZE);
                }
            }
            Ok(serde_json::Value::Null)
        }
        "toggle_maximize" => {
            unsafe {
                let hwnd = std::ptr::addr_of!(SR_HWND).read();
                if !hwnd.is_invalid() {
                    if IsZoomed(hwnd.0).as_bool() {
                        let _ = ShowWindow(hwnd.0, SW_RESTORE);
                    } else {
                        let _ = ShowWindow(hwnd.0, SW_MAXIMIZE);
                    }
                }
            }
            Ok(serde_json::Value::Null)
        }
        "close_window" => {
            unsafe {
                let hwnd = std::ptr::addr_of!(SR_HWND).read();
                if !hwnd.is_invalid() {
                    let _ = ShowWindow(hwnd.0, SW_HIDE);
                }
            }
            Ok(serde_json::Value::Null)
        }
        "get_font_css" => {
            let css = crate::overlay::html_components::font_manager::get_font_css();
            Ok(serde_json::json!(css))
        }
        "is_maximized" => unsafe {
            let hwnd = std::ptr::addr_of!(SR_HWND).read();
            let maximized = if !hwnd.is_invalid() {
                IsZoomed(hwnd.0).as_bool()
            } else {
                false
            };
            Ok(serde_json::json!(maximized))
        },
        _ => Err(format!("Unknown command: {}", cmd)),
    }
}

fn handle_stage_export_data(args: &serde_json::Value) -> Result<serde_json::Value, String> {
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
        "cursor_slots_png" => {
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
                native_export::staging::set_cursor_slot_overrides_for(
                    session_id, job_id, overrides,
                );
            } else {
                native_export::staging::set_cursor_slot_overrides(overrides);
            }
        }
        _ => return Err(format!("unknown stage dataType: {data_type}")),
    }
    Ok(serde_json::Value::Null)
}

fn handle_save_cursor_anim_cache(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let slot_id = args["slotId"].as_u64().ok_or("missing slotId")? as u32;
    let svg_hash = args["svgHash"].as_str().ok_or("missing svgHash")?;
    let loop_duration = args["loopDuration"]
        .as_f64()
        .ok_or("missing loopDuration")?;
    let natural_width = args["naturalWidth"]
        .as_u64()
        .ok_or("missing naturalWidth")? as u32;
    let natural_height = args["naturalHeight"]
        .as_u64()
        .ok_or("missing naturalHeight")? as u32;

    let decode_png_array = |key: &str| -> Result<Vec<Vec<u8>>, String> {
        let arr = args[key].as_array().ok_or(format!("missing {key}"))?;
        let mut out = Vec::with_capacity(arr.len());
        for (i, v) in arr.iter().enumerate() {
            let b64 = v.as_str().ok_or(format!("{key}[{i}] not string"))?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| format!("{key}[{i}] b64: {e}"))?;
            out.push(bytes);
        }
        Ok(out)
    };

    let export_pngs = decode_png_array("exportPngs")?;
    let preview_pngs = decode_png_array("previewFrames")?;

    native_export::anim_cache::save_cache(
        slot_id,
        svg_hash,
        loop_duration,
        natural_width,
        natural_height,
        &export_pngs,
        &preview_pngs,
    )?;
    Ok(serde_json::Value::Null)
}
