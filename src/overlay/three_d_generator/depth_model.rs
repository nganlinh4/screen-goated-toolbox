//! Managed Depth Anything 3 model used for the generator's first-stage preview.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Result, anyhow, bail};
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use sha2::{Digest, Sha256};

const MODEL_URL: &str = "https://huggingface.co/zerochocobo/DepthAnything3_ONNX/resolve/main/da3_small.onnx?download=true";
const MODEL_BYTES: u64 = 100_704_575;
const MODEL_SHA256: &str = "d659ca25f1c665d5c064a64294d5fd0303e456aa94bf224a7777d9682cac7a39";
const MODEL_NAME: &str = "da3-small.onnx";

pub(crate) const DOWNLOAD_TITLE: &str = "Downloading Depth Anything 3";

pub(crate) fn depth_model_dir() -> std::path::PathBuf {
    crate::paths::app_local_data_dir()
        .join("3d-generator-runtime")
        .join("models")
}

fn model_path() -> std::path::PathBuf {
    depth_model_dir().join(MODEL_NAME)
}

fn preview_dir() -> PathBuf {
    crate::paths::app_local_data_dir()
        .join("3d-generator-runtime")
        .join("depth-previews")
}

fn decode_source_image(bytes: &[u8]) -> Result<image::RgbImage> {
    image::load_from_memory(bytes)
        .map_err(|error| anyhow!("Could not decode source image: {error}"))
        .map(|image| image.to_rgb8())
}

fn load_source_image(path: &Path) -> Result<image::RgbImage> {
    let bytes =
        std::fs::read(path).map_err(|error| anyhow!("Could not read source image: {error}"))?;
    decode_source_image(&bytes)
}

fn validate_model_file(path: &std::path::Path) -> Result<()> {
    let metadata =
        std::fs::metadata(path).map_err(|error| anyhow!("model unavailable: {error}"))?;
    if !metadata.is_file() || metadata.len() != MODEL_BYTES {
        bail!(
            "Depth Anything 3 model size {} does not match expected {MODEL_BYTES}",
            metadata.len()
        );
    }

    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
        .unwrap_or(0);
    static CACHE: OnceLock<Mutex<Option<(u64, u128, bool)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Some((bytes, modified, valid)) = *cache.lock().unwrap_or_else(|value| value.into_inner())
        && bytes == metadata.len()
        && modified == modified_ms
    {
        return if valid {
            Ok(())
        } else {
            bail!("Depth Anything 3 model checksum mismatch")
        };
    }

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        use std::io::Read as _;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let valid = format!("{:x}", hasher.finalize()) == MODEL_SHA256;
    *cache.lock().unwrap_or_else(|value| value.into_inner()) =
        Some((metadata.len(), modified_ms, valid));
    if !valid {
        bail!("Depth Anything 3 model checksum mismatch");
    }
    Ok(())
}

pub(crate) fn is_depth_model_downloaded() -> bool {
    validate_model_file(&model_path()).is_ok()
}

pub(crate) fn remove_depth_model() -> Result<()> {
    let dir = depth_model_dir();
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

/// Shared by first-use creation and the Downloaded Tools card. Progress is
/// published through the same live state that backs other managed model rows.
pub(crate) fn download_depth_model(stop: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    use crate::overlay::auto_copy_badge::{
        NotificationType, hide_progress_notification, show_detailed_notification,
        show_error_notification, show_progress_notification,
    };
    use crate::overlay::realtime_webview::state::REALTIME_STATE;

    static DOWNLOAD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let badge = crate::overlay::auto_copy_badge::locale_text();
    let badge_title = crate::overlay::auto_copy_badge::format_locale(
        badge.downloading_model_fmt,
        &[("name", "Depth Anything 3")],
    );
    let badge_preparing = crate::overlay::auto_copy_badge::format_locale(
        badge.preparing_model_fmt,
        &[("name", "Depth Anything 3")],
    );
    let _guard = DOWNLOAD_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if is_depth_model_downloaded() {
        return Ok(());
    }

    let path = model_path();
    std::fs::create_dir_all(depth_model_dir())?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    let message = "Preparing the 3D depth preview model...";
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = DOWNLOAD_TITLE.to_string();
        state.download_message = message.to_string();
        state.download_progress = 0.0;
    }
    if use_badge {
        show_progress_notification(&badge_title, &badge_preparing, 0.0);
    }

    let result = crate::api::realtime_audio::model_loader::download_file_with_progress(
        MODEL_URL,
        &path,
        &stop,
        |downloaded, total| {
            let progress = if total > 0 {
                downloaded as f32 / total as f32 * 100.0
            } else {
                0.0
            };
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = "Downloading Depth Anything 3...".to_string();
                state.download_progress = progress;
            }
            if use_badge {
                show_progress_notification(&badge_title, &badge_title, progress);
            }
        },
    )
    .and_then(|()| validate_model_file(&path));

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
        state.download_progress = if result.is_ok() { 100.0 } else { 0.0 };
    }
    if use_badge {
        hide_progress_notification();
        if result.is_ok() {
            let ready = crate::overlay::auto_copy_badge::format_locale(
                badge.model_ready_fmt,
                &[("name", "Depth Anything 3")],
            );
            let installed = crate::overlay::auto_copy_badge::format_locale(
                badge.model_installed_fmt,
                &[("name", "Depth Anything 3")],
            );
            show_detailed_notification(&ready, &installed, NotificationType::Success);
        } else {
            let failed = crate::overlay::auto_copy_badge::format_locale(
                badge.model_download_failed_fmt,
                &[("name", "Depth Anything 3")],
            );
            show_error_notification(&failed);
        }
    }
    result.map_err(|error| anyhow!("Depth Anything 3 download: {error}"))
}

pub(crate) fn create_depth_preview(image_path: &str) -> Result<PathBuf> {
    const SIDE: u32 = 518;
    crate::unpack_dlls::ensure_ai_runtime_installed(
        Arc::new(AtomicBool::new(false)),
        crate::unpack_dlls::AiRuntimeUi::Badge,
    )?;
    crate::unpack_dlls::ensure_onnx_runtime_initialized()?;
    validate_model_file(&model_path())?;

    // Some image tools preserve a stale extension after re-encoding. Decode by
    // the file signature so a JPEG named .png still gets a depth preview.
    let source = load_source_image(Path::new(image_path))?;
    let (source_width, source_height) = source.dimensions();
    let resized =
        image::imageops::resize(&source, SIDE, SIDE, image::imageops::FilterType::Triangle);
    let input = ort::value::Value::from_array((
        vec![1i64, SIDE as i64, SIDE as i64, 3],
        resized.into_raw(),
    ))
    .map_err(|error| anyhow!("Depth input tensor: {error}"))?;

    static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();
    let mut guard = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    if guard.is_none() {
        let session = Session::builder()
            .map_err(|error| anyhow!("Depth session builder: {error}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|error| anyhow!("Depth optimization: {error}"))?
            .with_execution_providers(vec![ort::ep::CPU::default().build().error_on_failure()])
            .map_err(|error| anyhow!("Depth execution provider: {error}"))?
            .commit_from_file(model_path())
            .map_err(|error| anyhow!("Depth model load: {error}"))?;
        *guard = Some(session);
    }
    let session = guard
        .as_mut()
        .ok_or_else(|| anyhow!("Depth session is unavailable"))?;
    let outputs = session
        .run(ort::inputs!["image" => input])
        .map_err(|error| anyhow!("Depth inference: {error}"))?;
    let depth_value = outputs
        .get("depth")
        .ok_or_else(|| anyhow!("Depth model has no 'depth' output"))?;
    let (_, depth) = depth_value
        .try_extract_tensor::<f32>()
        .map_err(|error| anyhow!("Depth output: {error}"))?;
    let expected = SIDE as usize * SIDE as usize;
    if depth.len() != expected {
        bail!(
            "Depth output has {} pixels; expected {expected}",
            depth.len()
        );
    }

    let mut ordered: Vec<f32> = depth
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if ordered.is_empty() {
        bail!("Depth output has no finite pixels");
    }
    ordered.sort_unstable_by(f32::total_cmp);
    let low = ordered[(ordered.len() as f32 * 0.02).floor() as usize];
    let high_index = ((ordered.len() as f32 * 0.98).floor() as usize).min(ordered.len() - 1);
    let high = ordered[high_index];
    let span = (high - low).max(1e-6);
    let grayscale: Vec<u8> = depth
        .iter()
        .map(|value| {
            let normalized = if value.is_finite() {
                ((*value - low) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            ((1.0 - normalized) * 255.0).round() as u8
        })
        .collect();
    let depth_image = image::GrayImage::from_raw(SIDE, SIDE, grayscale)
        .ok_or_else(|| anyhow!("Could not assemble depth preview"))?;
    let full_size = image::imageops::resize(
        &depth_image,
        source_width.max(1),
        source_height.max(1),
        image::imageops::FilterType::Triangle,
    );

    std::fs::create_dir_all(preview_dir())?;
    let stem = Path::new(image_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("image")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let output = preview_dir().join(format!("{stem}-{timestamp}.png"));
    full_size
        .save_with_format(&output, image::ImageFormat::Png)
        .map_err(|error| anyhow!("Could not save depth preview: {error}"))?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};

    use super::decode_source_image;

    #[test]
    fn decodes_source_images_from_content_signature() {
        let source = RgbImage::from_pixel(3, 2, Rgb([42, 96, 180]));
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(source)
            .write_to(&mut encoded, ImageFormat::Jpeg)
            .expect("encode JPEG fixture");

        let decoded = decode_source_image(encoded.get_ref()).expect("decode JPEG bytes");

        assert_eq!(decoded.dimensions(), (3, 2));
    }
}
