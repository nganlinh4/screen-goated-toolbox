// --- SCREEN RECORD IPC ---
// IPC command handling for screen recording WebView.
// Routes commands to specialized submodules.

mod audio_waveform;
mod audio_waveform_cache;
mod cursor_anim_cache;
mod cursor_svg;
mod frame_export;
mod gemini_translate_narration;
pub(crate) use gemini_translate_narration::run_gt_narration_test_cli;
mod hotkeys;
mod job_registry;
mod managed_files;
pub mod media_server;
mod narration;
mod path_validation;
mod recording;
pub(super) mod recording_state;
mod s2s_narration;
mod stage_export;
mod subtitle_export;
mod subtitle_import;
mod subtitles;
mod wav_decode;
mod window_monitor;

use super::bg_download;
use super::engine::get_monitors;
use super::mf_decode;
use super::native_export;
use super::raw_video;
use super::{MEDIA_SERVER_TOKEN, SERVER_PORT, SR_HWND};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// Re-exports used by the parent module.
pub use media_server::start_global_media_server;

pub(super) fn cancel_all_jobs() {
    subtitles::cancel_all_jobs();
    narration::cancel_all_jobs();
    s2s_narration::cancel_all_jobs();
    gemini_translate_narration::cancel_all_jobs();
}
pub(super) fn start_recording(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    recording::handle_start_recording(args)
}

pub(super) fn stop_recording() -> Result<serde_json::Value, String> {
    recording::handle_stop_recording()
}

pub fn handle_ipc_command(
    cmd: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match cmd.as_str() {
        "diagnostic_arm_capture_setup_failure"
        | "diagnostic_gpu_device_loss"
        | "diagnostic_snapshot" => super::diagnostics::command(&cmd),
        "shutdown_ack" => super::shutdown::acknowledge(&args),
        "get_pending_recording" => super::shutdown::pending_recording(),
        "recording_saved" => super::shutdown::recording_saved(&args),
        "get_failed_recording" => super::shutdown::failed_recording(),
        "release_failed_recording" => super::shutdown::release_failed_recording(&args),
        "check_bg_downloaded" => {
            let id = args["id"].as_str().unwrap_or("");
            bg_download::validate_catalog_background_id(id)?;
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
            bg_download::start_download(id, url)?;
            Ok(serde_json::Value::Null)
        }
        "get_bg_download_progress" => {
            let id = args["id"].as_str().unwrap_or("");
            bg_download::validate_catalog_background_id(id)?;
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
            if ids.len() > 256 {
                return Err("Too many background states requested".to_string());
            }
            let mut states = serde_json::Map::new();
            for id in ids {
                bg_download::validate_catalog_background_id(id)?;
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
            bg_download::delete_downloaded(id)?;
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
        "reconcile_uploaded_backgrounds" => {
            let retained_values = args["retainedUrls"]
                .as_array()
                .ok_or("Missing retained background URLs")?;
            if retained_values.len() > 4_096 {
                return Err("Too many retained background URLs".to_string());
            }
            let retained = retained_values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>();
            let removed = bg_download::reconcile_uploaded_files(&retained)?;
            Ok(serde_json::json!({ "removed": removed }))
        }
        "log_message" => {
            let msg = args["message"].as_str().unwrap_or("");
            eprintln!("{msg}");
            Ok(serde_json::Value::Null)
        }
        "report_startup_milestone" => {
            let milestone = args["milestone"]
                .as_str()
                .ok_or("Missing startup milestone")?;
            if !matches!(
                milestone,
                "frontend-module-evaluated"
                    | "react-committed"
                    | "first-visible-frame"
                    | "projects-hydrated"
            ) {
                return Err("Unsupported startup milestone".to_string());
            }
            super::startup_trace::log(milestone);
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
        "load_cursor_anim_cache" => cursor_anim_cache::handle_load(&args),
        "save_cursor_anim_cache" => cursor_anim_cache::handle_save(&args),
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
        "read_subtitle_file_path" => subtitle_import::handle_read_subtitle_file_path(&args),
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
            let explorer =
                crate::system_command::explorer_executable().map_err(|e| e.to_string())?;
            std::process::Command::new(explorer)
                .args(["/select,", &path.replace("/", "\\")])
                .spawn()
                .map_err(|e| e.to_string())?;
            Ok(serde_json::Value::Null)
        }
        "rename_file" => {
            let path_str = args["path"].as_str().ok_or("Missing path")?;
            let new_name = args["newName"].as_str().ok_or("Missing newName")?;
            let new_path =
                managed_files::rename_media_file(std::path::Path::new(path_str), new_name)?;
            Ok(serde_json::json!(new_path.to_string_lossy().to_string()))
        }
        "delete_file" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            managed_files::delete_recording_file(path)?;
            Ok(serde_json::Value::Null)
        }
        "set_project_limit" => {
            let limit = args["limit"].as_u64().ok_or("Missing project limit")? as usize;
            if !(10..=100).contains(&limit) {
                return Err("Project limit must be between 10 and 100".to_string());
            }
            super::worker_settings::request(sgt_recorder_protocol::SettingsChange::ProjectLimit(
                limit,
            ))?;
            Ok(serde_json::Value::Null)
        }
        "set_recent_upload_limit" => {
            let limit = args["limit"].as_u64().ok_or("Missing upload limit")? as usize;
            if !(4..=24).contains(&limit) {
                return Err("Recent upload limit must be between 4 and 24".to_string());
            }
            super::worker_settings::request(sgt_recorder_protocol::SettingsChange::UploadLimit(
                limit,
            ))?;
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
        "save_current_frame" => frame_export::handle_save_current_frame(&args),
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
                    if let Some(data_url) =
                        window_monitor::capture_window_thumbnail_with_timeout(hwnd)
                    {
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
        "start_recording" => super::shutdown::start_recording(&args),
        "stop_recording" => super::shutdown::stop_recording(),
        "start_subtitle_generation" => subtitles::handle_start_subtitle_generation(&args),
        "get_subtitle_generation_capabilities" => {
            subtitles::handle_get_subtitle_generation_capabilities(&args)
        }
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
        "allocate_narration_job_id" => job_registry::allocate_narration_job_id(&args),
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
            let override_id = args["subtitleOverrideId"]
                .as_u64()
                .unwrap_or(0)
                .min(i32::MAX as u64) as usize;
            unsafe {
                let hwnd = std::ptr::addr_of!(SR_HWND).read();
                if hwnd.is_invalid() {
                    return Err("Recorder window is unavailable".to_string());
                }
                PostMessageW(Some(hwnd.0), WM_CLOSE, WPARAM(override_id), LPARAM(0))
                    .map_err(|error| format!("Cannot close recorder: {error}"))?;
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

#[cfg(test)]
mod retired_command_tests {
    #[test]
    fn the_retired_window_list_command_is_unknown() {
        assert_eq!(
            super::handle_ipc_command("get_windows".to_string(), serde_json::json!({})),
            Err("Unknown command: get_windows".to_string())
        );
    }
}
