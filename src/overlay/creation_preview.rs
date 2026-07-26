use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::path::Path;
use std::sync::{Condvar, LazyLock, Mutex, Once};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose};
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageReader};
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::win_types::SendHwnd;

const DEFAULT_MAX_EDGE: u32 = 1_600;
const MIN_MAX_EDGE: u32 = 64;
const MAX_MAX_EDGE: u32 = 2_048;
const MAX_SOURCE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_SOURCE_DIMENSION: u32 = 32_768;
const MAX_SOURCE_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_DECODE_BYTES: u64 = 320 * 1024 * 1024;
const JPEG_QUALITY: u8 = 82;
const PREVIEW_WORKERS: usize = 2;
const MAX_PENDING_PREVIEWS: usize = 256;

struct PreviewRequest {
    hwnd: SendHwnd,
    generation: u64,
    reply_message: u32,
    id: String,
    path: String,
    max_edge: Option<u32>,
}

#[derive(Default)]
struct AsyncPreviewState {
    next_generation: u64,
    active_targets: HashMap<isize, u64>,
    replies: HashMap<(isize, u64), Vec<String>>,
}

static ASYNC_PREVIEW_STATE: LazyLock<Mutex<AsyncPreviewState>> =
    LazyLock::new(|| Mutex::new(AsyncPreviewState::default()));
static PREVIEW_QUEUE: LazyLock<(Mutex<VecDeque<PreviewRequest>>, Condvar)> =
    LazyLock::new(|| (Mutex::new(VecDeque::new()), Condvar::new()));
static START_PREVIEW_WORKERS: Once = Once::new();

pub fn register_async_target(hwnd: HWND) {
    let key = hwnd.0 as isize;
    let mut state = ASYNC_PREVIEW_STATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    state.next_generation = state.next_generation.wrapping_add(1).max(1);
    let generation = state.next_generation;
    state.active_targets.insert(key, generation);
    state.replies.retain(|(target, _), _| *target != key);
}

pub fn unregister_async_target(hwnd: HWND) {
    let key = hwnd.0 as isize;
    {
        let mut state = ASYNC_PREVIEW_STATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.active_targets.remove(&key);
        state.replies.retain(|(target, _), _| *target != key);
    }
    let (queue, _) = &*PREVIEW_QUEUE;
    queue
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|request| request.hwnd.as_isize() != key);
}

pub fn request_async_preview(
    hwnd: HWND,
    reply_message: u32,
    id: String,
    path: String,
    max_edge: Option<u32>,
) -> std::result::Result<(), String> {
    if id.is_empty() {
        return Ok(());
    }
    let key = hwnd.0 as isize;
    let generation = ASYNC_PREVIEW_STATE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .active_targets
        .get(&key)
        .copied()
        .ok_or_else(|| "Preview target is no longer available.".to_string())?;

    START_PREVIEW_WORKERS.call_once(|| {
        for _ in 0..PREVIEW_WORKERS {
            std::thread::spawn(preview_worker);
        }
    });

    let (queue, signal) = &*PREVIEW_QUEUE;
    let mut queue = queue.lock().unwrap_or_else(|error| error.into_inner());
    if queue.len() >= MAX_PENDING_PREVIEWS {
        return Err("Too many image previews are waiting.".to_string());
    }
    queue.push_back(PreviewRequest {
        hwnd: SendHwnd(hwnd),
        generation,
        reply_message,
        id,
        path,
        max_edge,
    });
    signal.notify_one();
    Ok(())
}

pub fn take_async_replies(hwnd: HWND) -> Vec<String> {
    let key = hwnd.0 as isize;
    let mut state = ASYNC_PREVIEW_STATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let Some(generation) = state.active_targets.get(&key).copied() else {
        return Vec::new();
    };
    state.replies.remove(&(key, generation)).unwrap_or_default()
}

fn preview_worker() {
    loop {
        let request = {
            let (queue, signal) = &*PREVIEW_QUEUE;
            let mut queue = queue.lock().unwrap_or_else(|error| error.into_inner());
            while queue.is_empty() {
                queue = signal
                    .wait(queue)
                    .unwrap_or_else(|error| error.into_inner());
            }
            queue.pop_front()
        };
        let Some(request) = request else {
            continue;
        };
        if !async_target_is_current(&request) {
            continue;
        }
        let result =
            read_image_preview(&request.path, request.max_edge).map_err(|error| error.to_string());
        queue_async_reply(request, result);
    }
}

fn async_target_is_current(request: &PreviewRequest) -> bool {
    ASYNC_PREVIEW_STATE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .active_targets
        .get(&request.hwnd.as_isize())
        .is_some_and(|generation| *generation == request.generation)
}

fn queue_async_reply(request: PreviewRequest, result: std::result::Result<Value, String>) {
    let payload = match result {
        Ok(value) => json!({ "id": request.id, "result": value }),
        Err(error) => json!({ "id": request.id, "error": error }),
    };
    let script =
        format!("window.dispatchEvent(new CustomEvent('ipc-reply', {{ detail: {payload} }}));");
    let key = request.hwnd.as_isize();
    {
        let mut state = ASYNC_PREVIEW_STATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if state.active_targets.get(&key) != Some(&request.generation) {
            return;
        }
        state
            .replies
            .entry((key, request.generation))
            .or_default()
            .push(script);
    }
    if unsafe {
        PostMessageW(
            Some(request.hwnd.0),
            request.reply_message,
            WPARAM(0),
            LPARAM(0),
        )
    }
    .is_err()
    {
        unregister_async_target(request.hwnd.0);
    }
}

pub fn read_image_preview(path: &str, max_edge: Option<u32>) -> Result<Value> {
    let source = Path::new(path);
    let metadata = std::fs::metadata(source)
        .with_context(|| format!("Could not inspect image: {}", source.display()))?;
    if !metadata.is_file() {
        bail!("Image is not a file.");
    }
    if metadata.len() == 0 || metadata.len() > MAX_SOURCE_BYTES {
        bail!("Image size is outside the supported range.");
    }

    let (width, height) = image_dimensions(source)?;
    validate_dimensions(width, height)?;

    let mut reader = ImageReader::open(source)
        .with_context(|| format!("Could not open image: {}", source.display()))?
        .with_guessed_format()
        .context("Could not identify the image format.")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_DIMENSION);
    limits.max_image_height = Some(MAX_SOURCE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let image = reader.decode().context("Could not decode the image.")?;
    validate_dimensions(image.width(), image.height())?;
    let edge = max_edge
        .unwrap_or(DEFAULT_MAX_EDGE)
        .clamp(MIN_MAX_EDGE, MAX_MAX_EDGE);
    let preview = image.thumbnail(edge, edge);

    let (mime, encoded) = if preview.color().has_alpha() {
        let mut encoded = Cursor::new(Vec::new());
        preview
            .write_to(&mut encoded, image::ImageFormat::Png)
            .context("Could not encode the image preview.")?;
        ("image/png", encoded.into_inner())
    } else {
        let rgb = preview.to_rgb8();
        let mut encoded = Vec::new();
        JpegEncoder::new_with_quality(&mut encoded, JPEG_QUALITY)
            .encode(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                ExtendedColorType::Rgb8,
            )
            .context("Could not encode the image preview.")?;
        ("image/jpeg", encoded)
    };

    Ok(json!({
        "dataUrl": format!(
            "data:{mime};base64,{}",
            general_purpose::STANDARD.encode(&encoded)
        ),
        "sourceSizeBytes": metadata.len(),
        "previewSizeBytes": encoded.len(),
        "width": width,
        "height": height,
        "previewWidth": preview.width(),
        "previewHeight": preview.height()
    }))
}

fn image_dimensions(source: &Path) -> Result<(u32, u32)> {
    ImageReader::open(source)
        .with_context(|| format!("Could not open image: {}", source.display()))?
        .with_guessed_format()
        .context("Could not identify the image format.")?
        .into_dimensions()
        .context("Could not read the image dimensions.")
}

fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 {
        bail!("Image dimensions must be positive.");
    }
    if width > MAX_SOURCE_DIMENSION
        || height > MAX_SOURCE_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_SOURCE_PIXELS
    {
        bail!("Image dimensions are outside the supported range.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::read_image_preview;
    use base64::{Engine as _, engine::general_purpose};
    use image::{ImageBuffer, Rgb};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn preview_is_bounded_and_decodable() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sgt-creation-preview-{}-{unique}.png",
            std::process::id()
        ));
        let source = ImageBuffer::from_fn(2_048, 1_024, |x, y| {
            Rgb([(x % 255) as u8, (y % 255) as u8, ((x + y) % 255) as u8])
        });
        source.save(&path).unwrap();

        let value = read_image_preview(path.to_str().unwrap(), Some(128)).unwrap();
        assert_eq!(value["width"], 2_048);
        assert_eq!(value["height"], 1_024);
        assert_eq!(value["previewWidth"], 128);
        assert_eq!(value["previewHeight"], 64);

        let data_url = value["dataUrl"].as_str().unwrap();
        let encoded = data_url.split_once(',').unwrap().1;
        let decoded = general_purpose::STANDARD.decode(encoded).unwrap();
        let preview = image::load_from_memory(&decoded).unwrap();
        assert_eq!((preview.width(), preview.height()), (128, 64));

        std::fs::remove_file(path).unwrap();
    }
}
