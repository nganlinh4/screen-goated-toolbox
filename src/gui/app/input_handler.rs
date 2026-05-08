// Input Handler - Drag-and-Drop and Paste handling for the main app UI
//
// When files/images are dropped or pasted (Ctrl+V), this module:
// 1. Detects the content type (image, text, or audio)
// 2. Shows the appropriate preset wheel
// 3. Triggers the processing pipeline with the selected preset

use crate::APP;
use crate::overlay::preset_wheel::{
    show_custom_wheel, show_preset_wheel, show_preset_wheel_with_extra,
};
use crate::overlay::process::pipeline::{
    start_processing_pipeline, start_processing_pipeline_parallel, start_text_processing,
};
use crate::overlay::utils::get_clipboard_image_bytes;
use eframe::egui;
use image::{ImageBuffer, Rgba};
use std::io::Cursor;
use std::path::Path;
use std::sync::mpsc;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

type CapturedImagePayload = (ImageBuffer<Rgba<u8>, Vec<u8>>, Vec<u8>);

/// Image file extensions we support
const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "ico", "tiff", "tif",
];

/// Audio file extensions we support (decoded via symphonia)
const AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "flac", "ogg", "m4a", "aac", "alac", "aiff", "aif", "wma", "opus",
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
    AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Check if a file extension is a video type
fn is_video_extension(ext: &str) -> bool {
    VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Check if a file extension is a subtitle type
fn is_subtitle_extension(ext: &str) -> bool {
    SUBTITLE_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Load a text file content
fn load_text_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Load an audio file and convert to WAV format using symphonia
/// Supports: WAV, MP3, FLAC, OGG, AAC, ALAC, AIFF, etc.
fn load_audio_file(path: &Path) -> Option<Vec<u8>> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    // Open the file
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    // Create a hint using the file extension
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    // Probe the media source
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;

    let mut format = probed.format;

    // Find the first audio track
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();

    // Get sample rate and channels
    let sample_rate = codec_params.sample_rate.unwrap_or(44100);
    let channels = codec_params.channels.map(|c| c.count()).unwrap_or(2) as u16;

    // Create decoder
    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .ok()?;

    // Decode all samples
    let mut all_samples: Vec<i16> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Convert to interleaved i16 samples
        let spec = *decoded.spec();
        let duration = decoded.capacity() as u64;
        let mut sample_buf = SampleBuffer::<i16>::new(duration, spec);
        sample_buf.copy_interleaved_ref(decoded);
        all_samples.extend(sample_buf.samples());
    }

    if all_samples.is_empty() {
        return None;
    }

    // Write to WAV format in memory
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut wav_cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut wav_cursor, spec).ok()?;
        for sample in &all_samples {
            writer.write_sample(*sample).ok()?;
        }
        writer.finalize().ok()?;
    }

    Some(wav_cursor.into_inner())
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
    let cursor_pos = get_cursor_pos();

    // Show image preset wheel (no filter_mode = all image presets)
    let selected = show_preset_wheel("image", None, cursor_pos);

    if let Some(preset_idx) = selected {
        let (config, preset) = {
            let mut app = APP.lock().unwrap();
            // Update active preset for auto-paste to work correctly
            app.config.active_preset_idx = preset_idx;
            (app.config.clone(), app.config.presets[preset_idx].clone())
        };

        let rect = get_screen_rect_at_cursor();

        // Spawn processing in background thread
        std::thread::spawn(move || {
            start_processing_pipeline(img, rect, config, preset);
        });
    }
}

/// Process dropped/pasted text content
fn process_text_content(text: String) {
    let cursor_pos = get_cursor_pos();

    // Show text preset wheel without mode filter (shows both select and type presets)
    let selected = show_preset_wheel("text", None, cursor_pos);

    if let Some(preset_idx) = selected {
        let (config, preset) = {
            let mut app = APP.lock().unwrap();
            // Update active preset for auto-paste to work correctly
            app.config.active_preset_idx = preset_idx;
            (app.config.clone(), app.config.presets[preset_idx].clone())
        };

        let rect = get_screen_rect_at_cursor();
        let ui_lang = config.ui_language.clone();
        let localized_name =
            crate::gui::settings_ui::get_localized_preset_name(&preset.id, &ui_lang);
        let cancel_hotkey = preset
            .hotkeys
            .first()
            .map(|h| h.name.clone())
            .unwrap_or_default();

        // Spawn processing in background thread
        std::thread::spawn(move || {
            start_text_processing(text, rect, config, preset, localized_name, cancel_hotkey);
        });
    }
}

/// Process image content in parallel (show wheel immediately, wait for load)
fn process_image_parallel(rx: mpsc::Receiver<Option<CapturedImagePayload>>) {
    let cursor_pos = get_cursor_pos();
    let selected = show_preset_wheel("image", None, cursor_pos);

    if let Some(preset_idx) = selected {
        crate::log_info!("Image preset selected: {}", preset_idx);
        let (config, preset) = {
            let mut app = APP.lock().unwrap();
            app.config.active_preset_idx = preset_idx;
            (app.config.clone(), app.config.presets[preset_idx].clone())
        };
        let rect = get_screen_rect_at_cursor();

        // Use parallel pipeline to show window immediately while waiting for data
        start_processing_pipeline_parallel(rx, rect, config, preset);
    }
}

/// Process text content in parallel
fn process_text_parallel(rx: mpsc::Receiver<Option<String>>) {
    let cursor_pos = get_cursor_pos();
    let selected = show_preset_wheel("text", None, cursor_pos);

    if let Some(preset_idx) = selected {
        crate::log_info!("Text preset selected: {}", preset_idx);
        let (config, preset) = {
            let mut app = APP.lock().unwrap();
            app.config.active_preset_idx = preset_idx;
            (app.config.clone(), app.config.presets[preset_idx].clone())
        };

        let rect = get_screen_rect_at_cursor();
        let ui_lang = config.ui_language.clone();
        let localized_name =
            crate::gui::settings_ui::get_localized_preset_name(&preset.id, &ui_lang);
        let cancel_hotkey = preset
            .hotkeys
            .first()
            .map(|h| h.name.clone())
            .unwrap_or_default();

        std::thread::spawn(move || {
            if let Ok(Some(text)) = rx.recv() {
                start_text_processing(text, rect, config, preset, localized_name, cancel_hotkey);
            }
        });
    }
}

fn process_audio_parallel_with_preset(rx: mpsc::Receiver<Option<Vec<u8>>>, preset_idx: usize) {
    let preset = {
        let mut app = APP.lock().unwrap();
        app.config.active_preset_idx = preset_idx;
        app.config.presets[preset_idx].clone()
    };

    std::thread::spawn(move || {
        if let Ok(Some(wav_data)) = rx.recv() {
            crate::api::audio::process_audio_file_request(preset, wav_data);
        }
    });
}

fn open_video_in_screen_record(path: &Path, action: &str) {
    let path = path.to_string_lossy().to_string();
    let action = action.to_string();
    crate::overlay::screen_record::queue_video_drop_action(path, action);
    crate::overlay::screen_record::show_screen_record();
    std::thread::spawn(move || {
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
            locale.video_drop_work_record.to_string(),
            locale.video_drop_generate_subtitles.to_string(),
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
    std::thread::spawn(move || {
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
    std::thread::spawn(move || {
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
        locale.audio_drop_add_to_record.to_string()
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
        locale.audio_drop_add_to_record.to_string()
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
            std::thread::spawn(move || {
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
        process_video_path(path);
    } else if is_subtitle_extension(ext) {
        crate::log_info!("Type detected: SUBTITLE");
        open_subtitle_in_screen_record(path);
    } else if is_image_extension(ext) {
        crate::log_info!("Type detected: IMAGE");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // Read file bytes directly (preserves original format e.g. JPEG)
            if let Ok(bytes) = std::fs::read(&path_clone)
                && let Ok(img) = image::load_from_memory(&bytes)
            {
                let _ = tx.send(Some((img.to_rgba8(), bytes)));
                return;
            }
            let _ = tx.send(None);
        });
        process_image_parallel(rx);
    } else if is_audio_extension(ext) {
        crate::log_info!("Type detected: AUDIO");
        process_audio_path(path);
    } else {
        crate::log_info!("Type detected: TEXT (Default)");
        // Default to Text (covers text files and unknown extensions)
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(load_text_file(&path_clone));
        });
        process_text_parallel(rx);
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
        .filter_map(|file| file.path.clone())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(is_audio_extension)
                .unwrap_or(false)
        })
        .collect();
    if !audio_paths.is_empty() && audio_paths.len() == dropped_files.len() {
        crate::log_info!("Handling dropped audio files: {}", audio_paths.len());
        process_audio_paths(&audio_paths);
        return true;
    }

    // Process the first dropped file
    if let Some(file) = dropped_files.first() {
        if let Some(path) = &file.path {
            crate::log_info!("Handling dropped file: {:?}", path);
            process_file_path(path);
            return true;
        }
        // If path is not available, use existing byte handling (already threaded but serial load->process)
        else if let Some(bytes) = &file.bytes {
            let bytes_clone = bytes.clone();
            std::thread::spawn(move || {
                // Try to interpret as image first
                if let Ok(img) = image::load_from_memory(&bytes_clone) {
                    let rgba = img.to_rgba8();
                    // For direct bytes drop, we also pass the bytes as "original"
                    process_image_content(rgba); // Fallback to serial for bytes-drop or update process_image_content?
                // NOTE: process_image_content expects just ImageBuffer.
                // To support zero-copy for bytes-drop too, we would need to update process_image_content.
                // But user specifically asked for "dragging job" (files).
                // Leaving bytes-drop as-is for now (it uses process_image_content, not parallel pipeline yet? No wait, process_image_content spawns thread).
                }
                // Try as text
                else if let Ok(text) = String::from_utf8(bytes_clone.to_vec()) {
                    process_text_content(text);
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

/// Get text from Windows clipboard
fn get_clipboard_text() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    unsafe {
        // Try to open clipboard
        for _attempt in 0..5 {
            if OpenClipboard(None).is_ok() {
                // CF_UNICODETEXT = 13
                if let Ok(h_data) = GetClipboardData(13) {
                    let ptr = GlobalLock(HGLOBAL(h_data.0));
                    if !ptr.is_null() {
                        // Read as wide string
                        let wide_ptr = ptr as *const u16;
                        let mut len = 0;
                        while *wide_ptr.add(len) != 0 {
                            len += 1;
                        }
                        let slice = std::slice::from_raw_parts(wide_ptr, len);
                        let text = String::from_utf16_lossy(slice);

                        let _ = GlobalUnlock(HGLOBAL(h_data.0));
                        let _ = CloseClipboard();

                        if !text.is_empty() {
                            return Some(text);
                        }
                        return None;
                    }
                }
                let _ = CloseClipboard();
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        None
    }
}

/// Handle Ctrl+V paste - uses Windows API for keyboard detection
pub fn handle_paste(ctx: &egui::Context) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_V};

    // Skip paste handling if help assistant modal is open
    // This allows normal Ctrl+V paste into the text input field
    if crate::gui::settings_ui::help_assistant::is_modal_open() {
        return false;
    }

    // Only process if our window has focus
    let has_focus = ctx.input(|i| i.focused);
    if !has_focus {
        return false;
    }

    // skip paste handling if any of the api key fields are focused
    // this prevents the wheel from appearing when the user paste their api key
    let focused_id = ctx.memory(|mem| mem.focused());
    if let Some(id) = focused_id {
        let api_key_ids = [
            egui::Id::new("settings_api_key_groq"),
            egui::Id::new("settings_api_key_cerebras"),
            egui::Id::new("settings_api_key_gemini"),
            egui::Id::new("settings_api_key_openrouter"),
            egui::Id::new("settings_api_key_ollama_url"),
        ];
        if api_key_ids.contains(&id) {
            return false;
        }
    }

    // Debounce: prevent multiple triggers per key press
    static LAST_V_STATE: AtomicBool = AtomicBool::new(false);

    // Check keyboard state using Windows API
    let ctrl_down = unsafe { (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 };
    let v_down = unsafe { (GetAsyncKeyState(VK_V.0 as i32) as u16 & 0x8000) != 0 };
    let v_was_down = LAST_V_STATE.swap(v_down, Ordering::SeqCst);

    // Trigger on V key press (not release)
    let ctrl_v_just_pressed = ctrl_down && v_down && !v_was_down;

    // Also check egui events as fallback
    let paste_event = ctx.input(|i| {
        i.raw
            .events
            .iter()
            .any(|e| matches!(e, egui::Event::Paste(_)))
    });

    if !ctrl_v_just_pressed && !paste_event {
        return false;
    }

    // First try to get image from clipboard (images take priority)
    if let Some(img_bytes) = get_clipboard_image_bytes()
        && let Ok(img) = image::load_from_memory(&img_bytes)
    {
        let rgba = img.to_rgba8();
        std::thread::spawn(move || {
            process_image_content(rgba);
        });
        return true;
    }

    // Try to get text from clipboard via Windows API
    if let Some(text) = get_clipboard_text()
        && !text.is_empty()
    {
        std::thread::spawn(move || {
            process_text_content(text);
        });
        return true;
    }

    false
}
