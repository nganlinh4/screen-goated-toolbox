// --- SCREEN RECORD IPC ---
// IPC command handling for screen recording WebView.
// Routes commands to specialized submodules.

mod audio_waveform;
mod cursor_svg;
mod gemini_translate_narration;
mod hotkeys;
mod job_registry;
pub mod media_server;
mod narration;
mod recording;
mod s2s_narration;
mod stage_export;
mod subtitle_export;
mod subtitles;
mod wav_decode;
mod window_monitor;

use super::bg_download;
use super::engine::get_monitors;
use super::mf_decode;
use super::native_export;
use super::raw_video;
use super::{MEDIA_SERVER_TOKEN, SERVER_PORT, SR_HWND};
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
        "get_bg_download_states" => {
            let ids = args["ids"]
                .as_array()
                .ok_or("Missing ids")?
                .iter()
                .filter_map(|value| value.as_str())
                .collect::<Vec<_>>();
            let mut states = serde_json::Map::new();
            for id in ids {
                let info = bg_download::download_info(id);
                let status = bg_download::get_download_status(id);
                states.insert(
                    id.to_string(),
                    serde_json::json!({
                        "downloaded": info.is_some(),
                        "ext": info.as_ref().map(|(ext, _)| ext.clone()),
                        "version": info.as_ref().map(|(_, version)| *version),
                        "progress": status,
                    }),
                );
            }
            Ok(serde_json::Value::Object(states))
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
        "stage_export_data" => stage_export::handle_stage_export_data(&args),
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
        "start_audio_download" => native_export::start_audio_download(args),
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
            // Deliver the gate token over this SECURE custom-IPC bridge only
            // (it is not HTTP-reachable). The client attaches it as the
            // `X-SGT-Token` header on POST/fetch and as `&token=` on GET URLs.
            let token = MEDIA_SERVER_TOKEN.get().cloned().unwrap_or_default();
            Ok(serde_json::json!({ "port": port, "token": token }))
        }
        "import_video_path" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let trace_id = args["traceId"]
                .as_str()
                .unwrap_or("video-import-native-path");
            let (path, has_audio) = media_server::import_video_path_to_managed_media_file(
                std::path::Path::new(path),
                trace_id,
            )?;
            Ok(serde_json::json!({ "path": path, "hasAudio": has_audio }))
        }
        "import_audio_path" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let trace_id = args["traceId"]
                .as_str()
                .unwrap_or("audio-import-native-path");
            let (path, duration) = media_server::import_audio_path_to_managed_media_file(
                std::path::Path::new(path),
                trace_id,
            )?;
            Ok(serde_json::json!({ "path": path, "duration": duration }))
        }
        "create_audio_placeholder_video" => {
            let duration = args["duration"].as_f64().ok_or("Missing duration")?;
            let trace_id = args["traceId"]
                .as_str()
                .unwrap_or("audio-placeholder-video");
            let path = media_server::create_audio_placeholder_video(duration, trace_id)?;
            Ok(serde_json::json!({ "path": path }))
        }
        "take_pending_video_drop_actions" => Ok(serde_json::to_value(
            super::take_pending_video_drop_actions(),
        )
        .unwrap_or_else(|_| serde_json::json!([]))),
        "take_pending_audio_drop_actions" => Ok(serde_json::to_value(
            super::take_pending_audio_drop_actions(),
        )
        .unwrap_or_else(|_| serde_json::json!([]))),
        "take_pending_subtitle_drop_actions" => Ok(serde_json::to_value(
            super::take_pending_subtitle_drop_actions(),
        )
        .unwrap_or_else(|_| serde_json::json!([]))),
        "read_subtitle_file_path" => handle_read_subtitle_file_path(&args),
        "generate_thumbnails" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let count = args["count"].as_u64().unwrap_or(20) as u32;
            let start = args["start"].as_f64().unwrap_or(0.0);
            let end = args["end"].as_f64().unwrap_or(start);
            let result = super::mf_decode::generate_thumbnails(path, count, start, end)?;
            Ok(serde_json::json!(result))
        }
        "generate_timeline_thumbnails" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let times = args["times"]
                .as_array()
                .ok_or("Missing times")?
                .iter()
                .filter_map(|value| value.as_f64())
                .collect::<Vec<_>>();
            let width = args["width"].as_u64().unwrap_or(240) as u32;
            let height = args["height"].as_u64().unwrap_or(135) as u32;
            let quality = ((args["quality"].as_f64().unwrap_or(0.72) * 100.0).round() as i64)
                .clamp(1, 100) as u8;
            let result = super::mf_decode::generate_thumbnails_at_times(
                path, &times, width, height, quality,
            )?;
            Ok(serde_json::json!(result))
        }
        "get_audio_waveform" => audio_waveform::handle_get_audio_waveform(&args),
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
        "pick_audio_file" => {
            let selected = crate::overlay::tts_playground::pick_step_audio_reference_audio()
                .map_err(|e| e.to_string())?;
            Ok(match selected {
                Some(path) => serde_json::json!(path.display().to_string()),
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
        "save_subtitle_srt" => subtitle_export::handle_save_subtitle_srt(&args),
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
                let is_dark = app.config.theme_mode.is_dark();
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
                let is_dark = app.config.theme_mode.is_dark();
                let lang = app.config.ui_language.clone();
                (is_dark, lang)
            };
            super::audio_source_selection::show_audio_app_selector(is_dark, lang);
            Ok(serde_json::Value::Null)
        }
        "start_recording" => recording::handle_start_recording(&args),
        "stop_recording" => recording::handle_stop_recording(),
        "start_subtitle_generation" => subtitles::handle_start_subtitle_generation(&args),
        "get_subtitle_generation_capabilities" => {
            subtitles::handle_get_subtitle_generation_capabilities(&args)
        }
        "prepare_qwen_local_subtitles" => subtitles::handle_prepare_qwen_local_subtitles(&args),
        "prepare_parakeet_tdt_subtitles" => subtitles::handle_prepare_parakeet_tdt_subtitles(&args),
        "get_subtitle_generation_status" => subtitles::handle_get_subtitle_generation_status(&args),
        "cancel_subtitle_generation" => subtitles::handle_cancel_subtitle_generation(&args),
        "start_subtitle_translation" => subtitles::handle_start_subtitle_translation(&args),
        "get_subtitle_translation_capabilities" => {
            subtitles::handle_get_subtitle_translation_capabilities(&args)
        }
        "get_subtitle_translation_status" => {
            subtitles::handle_get_subtitle_translation_status(&args)
        }
        "cancel_subtitle_translation" => subtitles::handle_cancel_subtitle_translation(&args),
        "start_subtitle_narration" => narration::handle_start_subtitle_narration(&args),
        "get_subtitle_narration_status" => narration::handle_get_subtitle_narration_status(&args),
        "cancel_subtitle_narration" => narration::handle_cancel_subtitle_narration(&args),
        "start_s2s_narration" => s2s_narration::handle_start_s2s_narration(&args),
        "get_s2s_narration_status" => s2s_narration::handle_get_s2s_narration_status(&args),
        "cancel_s2s_narration" => s2s_narration::handle_cancel_s2s_narration(&args),
        "start_gemini_translate_narration" => {
            gemini_translate_narration::handle_start_gemini_translate_narration(&args)
        }
        "get_gemini_translate_narration_status" => {
            gemini_translate_narration::handle_get_gemini_translate_narration_status(&args)
        }
        "cancel_gemini_translate_narration" => {
            gemini_translate_narration::handle_cancel_gemini_translate_narration(&args)
        }
        "get_narration_tts_metadata" => narration::handle_get_narration_tts_metadata(&args),
        "detect_narration_language" => narration::handle_detect_narration_language(&args),
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
                    let _ = PostMessageW(Some(hwnd.0), WM_CLOSE, WPARAM(0), LPARAM(0));
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

fn handle_read_subtitle_file_path(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let path = args["path"].as_str().ok_or("Missing path")?;
    let path = std::path::Path::new(path);
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension != "srt" && extension != "vtt" {
        return Err("Only .srt and .vtt subtitle files can be imported".to_string());
    }
    const MAX_SUBTITLE_BYTES: u64 = 10 * 1024 * 1024;
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Subtitle file is unavailable: {error}"))?;
    if metadata.len() > MAX_SUBTITLE_BYTES {
        return Err("Subtitle file is too large".to_string());
    }
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("Failed to read subtitle file: {error}"))?;
    let fallback_name = if extension == "vtt" {
        "subtitles.vtt"
    } else {
        "subtitles.srt"
    };
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(fallback_name);
    Ok(serde_json::json!({
        "fileName": file_name,
        "content": content,
        "format": extension,
    }))
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
