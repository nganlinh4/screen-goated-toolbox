use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose};
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageReader};
use serde_json::{Value, json};

const DEFAULT_MAX_EDGE: u32 = 1_600;
const MIN_MAX_EDGE: u32 = 64;
const MAX_MAX_EDGE: u32 = 2_048;
const MAX_SOURCE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_SOURCE_DIMENSION: u32 = 32_768;
const MAX_SOURCE_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_DECODE_BYTES: u64 = 320 * 1024 * 1024;
const JPEG_QUALITY: u8 = 82;

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

    let bytes = std::fs::read(source)
        .with_context(|| format!("Could not read image: {}", source.display()))?;
    let (width, height) = image_dimensions(&bytes)?;
    validate_dimensions(width, height)?;

    let mut reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .context("Could not identify the image format.")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_DIMENSION);
    limits.max_image_height = Some(MAX_SOURCE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let image = reader.decode().context("Could not decode the image.")?;
    let preview = image.thumbnail(
        max_edge
            .unwrap_or(DEFAULT_MAX_EDGE)
            .clamp(MIN_MAX_EDGE, MAX_MAX_EDGE),
        max_edge
            .unwrap_or(DEFAULT_MAX_EDGE)
            .clamp(MIN_MAX_EDGE, MAX_MAX_EDGE),
    );

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

fn image_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    ImageReader::new(Cursor::new(bytes))
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
