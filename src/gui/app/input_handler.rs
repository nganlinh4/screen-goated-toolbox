// Input Handler - Drag-and-Drop and Paste handling for the main app UI
//
// When files/images are dropped or pasted (Ctrl+V), this module:
// 1. Detects the content type (image, text, or audio)
// 2. Shows the appropriate preset wheel
// 3. Triggers the processing pipeline with the selected preset

mod file_load;
mod media_load;
mod paste;

use crate::APP;
use crate::overlay::preset_wheel::{
    show_custom_wheel, show_preset_wheel_id, show_preset_wheel_with_extra,
};
use crate::overlay::process::pipeline::{
    start_processing_pipeline, start_processing_pipeline_parallel, start_text_processing,
};
use crate::task_runtime::{TaskClass, spawn_detached};
use eframe::egui;
use image::{ImageBuffer, Rgba};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

pub(crate) use media_load::load_audio_file;
pub use paste::handle_paste;

type CapturedImagePayload = (ImageBuffer<Rgba<u8>, Vec<u8>>, Vec<u8>);
static DROP_SELECTOR_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Image file extensions we support
const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "ico", "tiff", "tif",
];

/// Video file extensions routed to SGT Record.
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "mkv", "webm", "avi", "m4v", "wmv", "flv", "mpeg", "mpg", "3gp", "ts", "mts",
    "m2ts",
];

/// Subtitle file extensions routed to SGT Record.
const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "vtt"];

/// Check if a file extension is an image type
fn is_image_extension(ext: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Check if a file extension is an audio type
fn is_audio_extension(ext: &str) -> bool {
    media_load::is_supported_audio_extension(ext)
}

/// Check if a file extension is a video type
fn is_video_extension(ext: &str) -> bool {
    VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Check if a file extension is a subtitle type
fn is_subtitle_extension(ext: &str) -> bool {
    SUBTITLE_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

fn notify_drop_error(error: file_load::DropError) {
    let text = {
        let app = APP.lock().unwrap();
        let shell = crate::gui::locale::LocaleText::get(&app.config.ui_language).shell;
        match error {
            file_load::DropError::TooLarge => shell.drop_too_large,
            file_load::DropError::Unsupported => shell.drop_unsupported,
            file_load::DropError::ReadFailed => shell.drop_read_failed,
            file_load::DropError::Busy => shell.drop_busy,
            file_load::DropError::ProcessingFailed => shell.drop_process_failed,
        }
    };
    crate::overlay::auto_copy_badge::show_error_notification(text);
}

fn spawn_drop_load(class: TaskClass, name: &'static str, work: impl FnOnce() + Send + 'static) {
    if crate::task_runtime::spawn(class, name, move |_| work()).is_err() {
        notify_drop_error(file_load::DropError::ProcessingFailed);
    }
}

fn reserve_drop() -> Option<file_load::SelectionGuard<'static>> {
    let guard = file_load::SelectionGuard::try_acquire(&DROP_SELECTOR_ACTIVE);
    if guard.is_none() {
        notify_drop_error(file_load::DropError::Busy);
    }
    guard
}

fn spawn_drop_selector(
    guard: file_load::SelectionGuard<'static>,
    work: impl FnOnce() + Send + 'static,
) {
    if std::thread::Builder::new()
        .name("dropped-file-selector".into())
        .spawn(move || {
            let _guard = guard;
            work();
        })
        .is_err()
    {
        notify_drop_error(file_load::DropError::ProcessingFailed);
    }
}

fn select_preset(
    filter: &str,
    cursor_pos: POINT,
) -> Option<(crate::config::Config, crate::config::Preset)> {
    let id = show_preset_wheel_id(filter, None, cursor_pos)?;
    let selected = {
        let mut app = APP.lock().unwrap();
        select_current_preset(&mut app.config, &id).map(|preset| (app.config.clone(), preset))
    };
    if selected.is_none() {
        notify_drop_error(file_load::DropError::ProcessingFailed);
    }
    selected
}

fn select_current_preset(
    config: &mut crate::config::Config,
    id: &str,
) -> Option<crate::config::Preset> {
    let index = config.presets.iter().position(|preset| preset.id == id)?;
    config.active_preset_idx = index;
    Some(config.presets[index].clone())
}

/// Get cursor position for wheel placement
fn get_cursor_pos() -> POINT {
    let mut pos = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pos);
    }
    pos
}

/// Get screen rect centered around cursor for result window placement
fn get_screen_rect_at_cursor() -> RECT {
    let pos = get_cursor_pos();
    RECT {
        left: pos.x - 200,
        top: pos.y - 100,
        right: pos.x + 200,
        bottom: pos.y + 100,
    }
}

/// Process dropped/pasted image content
fn process_image_content(img: ImageBuffer<Rgba<u8>, Vec<u8>>) {
    process_image_content_at(img, get_cursor_pos());
}

fn process_image_content_at(img: ImageBuffer<Rgba<u8>, Vec<u8>>, cursor_pos: POINT) {
    let destination = crate::overlay::process::chain::AutoPasteDestination::capture_current();
    // Show image preset wheel (no filter_mode = all image presets)
    if let Some((config, preset)) = select_preset("image", cursor_pos) {
        let rect = get_screen_rect_at_cursor();

        // Spawn processing in background thread
        spawn_detached(TaskClass::Io, "dropped-image-pipeline", move || {
            start_processing_pipeline(img, rect, config, preset, destination);
        });
    }
}

/// Process dropped/pasted text content
fn process_text_content(text: String) {
    process_text_content_at(text, get_cursor_pos());
}

fn process_text_content_at(text: String, cursor_pos: POINT) {
    let destination = crate::overlay::process::chain::AutoPasteDestination::capture_current();
    // Show text preset wheel without mode filter (shows both select and type presets)
    if let Some((config, preset)) = select_preset("text", cursor_pos) {
        let rect = get_screen_rect_at_cursor();
        let ui_lang = config.ui_language.clone();
        let localized_name =
            crate::gui::settings_ui::get_localized_preset_name(&preset.id, &ui_lang);
        let cancel_hotkey = preset
            .hotkeys
            .first()
            .map(|h| h.display_name())
            .unwrap_or_default();

        // Spawn processing in background thread
        spawn_detached(TaskClass::Io, "dropped-text-pipeline", move || {
            start_text_processing(
                text,
                rect,
                config,
                preset,
                localized_name,
                cancel_hotkey,
                destination,
            );
        });
    }
}

/// Process image content in parallel (show wheel immediately, wait for load)
fn process_image_parallel(rx: mpsc::Receiver<Option<CapturedImagePayload>>, cursor_pos: POINT) {
    let destination = crate::overlay::process::chain::AutoPasteDestination::capture_current();
    if let Some((config, preset)) = select_preset("image", cursor_pos) {
        let rect = get_screen_rect_at_cursor();

        // Use parallel pipeline to show window immediately while waiting for data
        start_processing_pipeline_parallel(rx, rect, config, preset, destination);
    }
}

fn process_audio_parallel_with_preset(rx: mpsc::Receiver<Option<Vec<u8>>>, preset_idx: usize) {
    let destination = crate::overlay::process::chain::AutoPasteDestination::capture_current();
    let preset = {
        let mut app = APP.lock().unwrap();
        app.config.active_preset_idx = preset_idx;
        app.config.presets[preset_idx].clone()
    };

    spawn_detached(TaskClass::Interactive, "dropped-audio-receive", move || {
        if let Ok(Some(wav_data)) = rx.recv() {
            crate::api::audio::process_audio_file_request(preset, wav_data, destination);
        }
    });
}

fn open_video_in_screen_record(path: &Path, action: &str) {
    let path = path.to_string_lossy().to_string();
    let action = action.to_string();
    crate::overlay::screen_record::queue_video_drop_action(path, action);
    crate::overlay::screen_record::show_screen_record();
    spawn_detached(TaskClass::Io, "video-drop-dispatch", move || {
        let script = "window.dispatchEvent(new CustomEvent('sgt-video-drop-pending'));".to_string();
        for _ in 0..80 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if crate::overlay::screen_record::post_script(script.clone()) {
                return;
            }
        }
        crate::log_info!("[VideoDrop] Failed to dispatch video action to SGT Record");
    });
}

fn process_video_path(path: &Path) {
    const ACTION_WORK_RECORD: usize = 0;
    const ACTION_GENERATE_SUBTITLES: usize = 1;

    let cursor_pos = get_cursor_pos();
    let (work_label, subtitle_label) = {
        let app = APP.lock().unwrap();
        let locale = crate::gui::locale::LocaleText::get(&app.config.ui_language);
        (
            locale.shell.video_drop_work_record.to_string(),
            locale.shell.video_drop_generate_subtitles.to_string(),
        )
    };

    let selected = show_custom_wheel(
        vec![
            (ACTION_WORK_RECORD, work_label),
            (ACTION_GENERATE_SUBTITLES, subtitle_label),
        ],
        cursor_pos,
    );

    match selected {
        Some(ACTION_WORK_RECORD) => open_video_in_screen_record(path, "work-record"),
        Some(ACTION_GENERATE_SUBTITLES) => open_video_in_screen_record(path, "generate-subtitles"),
        _ => {}
    }
}

fn open_audio_in_screen_record(path: &Path) {
    open_audio_paths_in_screen_record(&[path.to_path_buf()]);
}

fn open_audio_paths_in_screen_record(paths: &[std::path::PathBuf]) {
    for path in paths {
        crate::overlay::screen_record::queue_audio_drop_action(path.to_string_lossy().to_string());
    }
    crate::overlay::screen_record::show_screen_record();
    spawn_detached(TaskClass::Io, "audio-drop-dispatch", move || {
        let script = "window.dispatchEvent(new CustomEvent('sgt-audio-drop-pending'));".to_string();
        for _ in 0..80 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if crate::overlay::screen_record::post_script(script.clone()) {
                return;
            }
        }
        crate::log_info!("[AudioDrop] Failed to dispatch audio action to SGT Record");
    });
}

fn open_subtitle_in_screen_record(path: &Path) {
    crate::overlay::screen_record::queue_subtitle_drop_action(path.to_string_lossy().to_string());
    crate::overlay::screen_record::show_screen_record();
    spawn_detached(TaskClass::Io, "subtitle-drop-dispatch", move || {
        let script =
            "window.dispatchEvent(new CustomEvent('sgt-subtitle-drop-pending'));".to_string();
        for _ in 0..80 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if crate::overlay::screen_record::post_script(script.clone()) {
                return;
            }
        }
        crate::log_info!("[SubtitleDrop] Failed to dispatch subtitle action to SGT Record");
    });
}

fn process_audio_paths(paths: &[std::path::PathBuf]) {
    if paths.len() == 1 {
        process_audio_path(&paths[0]);
        return;
    }

    const ACTION_ADD_TO_RECORD: usize = 1_000_000;
    let cursor_pos = get_cursor_pos();
    let record_label = {
        let app = APP.lock().unwrap();
        let locale = crate::gui::locale::LocaleText::get(&app.config.ui_language);
        locale.shell.audio_drop_add_to_record.to_string()
    };
    let selected = show_preset_wheel_with_extra(
        "audio",
        None,
        cursor_pos,
        vec![(ACTION_ADD_TO_RECORD, record_label)],
    );
    if selected == Some(ACTION_ADD_TO_RECORD) {
        open_audio_paths_in_screen_record(paths);
    }
}

fn process_audio_path(path: &Path) {
    const ACTION_ADD_TO_RECORD: usize = 1_000_000;

    let cursor_pos = get_cursor_pos();
    let record_label = {
        let app = APP.lock().unwrap();
        let locale = crate::gui::locale::LocaleText::get(&app.config.ui_language);
        locale.shell.audio_drop_add_to_record.to_string()
    };

    let selected = show_preset_wheel_with_extra(
        "audio",
        None,
        cursor_pos,
        vec![(ACTION_ADD_TO_RECORD, record_label)],
    );

    match selected {
        Some(ACTION_ADD_TO_RECORD) => open_audio_in_screen_record(path),
        Some(preset_idx) => {
            let path_clone = path.to_path_buf();
            let (tx, rx) = mpsc::channel();
            spawn_detached(TaskClass::Cpu, "dropped-audio-load", move || {
                let _ = tx.send(load_audio_file(&path_clone));
            });
            process_audio_parallel_with_preset(rx, preset_idx);
        }
        _ => {}
    }
}

/// Process a single file path (public for context menu usage)
pub fn process_file_path(path: &Path) {
    crate::log_info!("Processing file path: {:?}", path);
    let path_clone = path.to_path_buf();

    // Determine type by extension for immediate feedback
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    crate::log_info!("Detected extension: '{}'", ext);

    if is_video_extension(ext) {
        crate::log_info!("Type detected: VIDEO");
        // Every wheel runs its own message loop, so it never runs on the UI thread.
        let Some(guard) = reserve_drop() else {
            return;
        };
        spawn_drop_selector(guard, move || process_video_path(&path_clone));
    } else if is_subtitle_extension(ext) {
        crate::log_info!("Type detected: SUBTITLE");
        open_subtitle_in_screen_record(path);
    } else if is_image_extension(ext) {
        crate::log_info!("Type detected: IMAGE");
        let Some(guard) = reserve_drop() else {
            return;
        };
        let cursor_pos = get_cursor_pos();
        spawn_drop_load(TaskClass::Cpu, "dropped-image-load", move || {
            let image = file_load::read_file(&path_clone, crate::image_decode::MAX_ENCODED_BYTES)
                .and_then(|bytes| {
                    crate::image_decode::load_for_pipeline(bytes)
                        .map_err(|_| file_load::DropError::Unsupported)
                });
            match image {
                Ok(payload) => {
                    let (tx, rx) = mpsc::channel();
                    let _ = tx.send(Some(payload));
                    spawn_drop_selector(guard, move || process_image_parallel(rx, cursor_pos));
                }
                Err(error) => notify_drop_error(error),
            }
        });
    } else if is_audio_extension(ext) {
        crate::log_info!("Type detected: AUDIO");
        let Some(guard) = reserve_drop() else {
            return;
        };
        spawn_drop_selector(guard, move || process_audio_path(&path_clone));
    } else {
        crate::log_info!("Type detected: possible text");
        let Some(guard) = reserve_drop() else {
            return;
        };
        let cursor_pos = get_cursor_pos();
        spawn_drop_load(
            TaskClass::Io,
            "dropped-text-load",
            move || match file_load::read_file(&path_clone, file_load::MAX_TEXT_BYTES)
                .and_then(file_load::text)
            {
                Ok(text) => {
                    spawn_drop_selector(guard, move || process_text_content_at(text, cursor_pos))
                }
                Err(error) => notify_drop_error(error),
            },
        );
    }
}

/// Handle dropped files from egui
pub fn handle_dropped_files(ctx: &egui::Context) -> bool {
    let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());

    if dropped_files.is_empty() {
        return false;
    }

    let audio_paths: Vec<_> = dropped_files
        .iter()
        .map(|file| file.path().to_path_buf())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(is_audio_extension)
                .unwrap_or(false)
        })
        .collect();
    if !audio_paths.is_empty() && audio_paths.len() == dropped_files.len() {
        crate::log_info!("Handling dropped audio files: {}", audio_paths.len());
        if let Some(guard) = reserve_drop() {
            spawn_drop_selector(guard, move || process_audio_paths(&audio_paths));
        }
        return true;
    }

    // Process the first dropped file
    if let Some(file) = dropped_files.first() {
        let path = file.path();
        if !path.as_os_str().is_empty() {
            crate::log_info!("Handling dropped file: {:?}", path);
            process_file_path(path);
            return true;
        }
        // If path is not available, use existing byte handling (already threaded but serial load->process)
        else if let Ok(bytes) = file.bytes() {
            let Some(guard) = reserve_drop() else {
                return true;
            };
            let bytes_clone = bytes;
            let cursor_pos = get_cursor_pos();
            spawn_drop_load(TaskClass::Cpu, "dropped-bytes-decode", move || {
                if bytes_clone.len() > crate::image_decode::MAX_ENCODED_BYTES {
                    notify_drop_error(file_load::DropError::TooLarge);
                    return;
                }
                // Try to interpret as image first
                if let Ok(img) = crate::image_decode::load_from_memory(&bytes_clone) {
                    let rgba = img.to_rgba8();
                    spawn_drop_selector(guard, move || process_image_content_at(rgba, cursor_pos));
                }
                // Try as text
                else {
                    if bytes_clone.len() > file_load::MAX_TEXT_BYTES {
                        notify_drop_error(file_load::DropError::TooLarge);
                        return;
                    }
                    match file_load::text(bytes_clone.to_vec()) {
                        Ok(text) => spawn_drop_selector(guard, move || {
                            process_text_content_at(text, cursor_pos)
                        }),
                        Err(error) => notify_drop_error(error),
                    }
                }
            });
            return true;
        }
    }

    false
}

/// Check if files are currently being dragged over the window (not yet dropped)
pub fn is_files_hovered(ctx: &egui::Context) -> bool {
    ctx.input(|i| !i.raw.hovered_files.is_empty())
}

#[cfg(test)]
mod drop_selector_tests {
    use super::*;
    use std::time::Duration;

    /// The wheel's message loop runs on its own thread: the UI thread returns
    /// at once, and the drop reservation lasts until the selection finishes.
    #[test]
    fn selector_work_runs_off_the_calling_thread_and_holds_the_reservation() {
        static ACTIVE: AtomicBool = AtomicBool::new(false);
        let guard = file_load::SelectionGuard::try_acquire(&ACTIVE).unwrap();
        let caller = std::thread::current().id();
        let (release, gate) = mpsc::channel::<()>();
        let (report, finished) = mpsc::channel();
        spawn_drop_selector(guard, move || {
            let _ = gate.recv_timeout(Duration::from_secs(5));
            let _ = report.send(std::thread::current().id());
        });
        let reserved_while_selecting = file_load::SelectionGuard::try_acquire(&ACTIVE).is_none();
        release.send(()).unwrap();
        let worker = finished.recv_timeout(Duration::from_secs(5)).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while ACTIVE.load(std::sync::atomic::Ordering::Acquire) {
            assert!(std::time::Instant::now() < deadline, "reservation leaked");
            std::thread::yield_now();
        }
        assert!(reserved_while_selecting);
        assert_ne!(worker, caller);
    }

    #[test]
    fn every_wheel_opening_drop_goes_through_the_selector_thread() {
        let source = include_str!("input_handler.rs");
        let dispatch = &source[source.find("pub fn process_file_path").unwrap()
            ..source.find("pub fn is_files_hovered").unwrap()];
        for call in [
            "process_video_path(",
            "process_audio_path(",
            "process_audio_paths(",
            "process_image_parallel(",
            "process_text_content_at(",
            "process_image_content_at(",
        ] {
            for (offset, _) in dispatch.match_indices(call) {
                let before = &dispatch[..offset];
                let selector = before.rfind("spawn_drop_selector(").unwrap_or(0);
                assert!(
                    selector > 0 && !before[selector..].contains(';'),
                    "{call} must run inside spawn_drop_selector"
                );
            }
        }
    }
}
