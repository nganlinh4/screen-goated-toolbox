use super::super::contract::DetectedTextRegion;
use super::super::geometry::{PixelRegion, normalized_region};
use crate::overlay::selection::CapturedRegion;
use anyhow::{Context, Result};
use image::{ExtendedColorType, ImageEncoder as _, codecs::jpeg::JpegEncoder};
use std::path::Path;

pub(super) fn save_detector_preview(
    path: &Path,
    source_jpeg: &[u8],
    candidates: &[DetectedTextRegion],
    size: (u32, u32),
) -> Result<()> {
    let mut image = image::load_from_memory(source_jpeg)
        .context("decode detector evidence source")?
        .to_rgba8();
    for candidate in candidates {
        draw_box(
            &mut image,
            normalized_region(candidate.bounds, size.0, size.1),
        );
    }
    save_jpeg(path, &image)
}

fn draw_box(image: &mut image::RgbaImage, region: PixelRegion) {
    if region.width == 0 || region.height == 0 || image.width() == 0 || image.height() == 0 {
        return;
    }
    let left = region.x.min(image.width() - 1);
    let top = region.y.min(image.height() - 1);
    let right = region
        .x
        .saturating_add(region.width.saturating_sub(1))
        .min(image.width() - 1);
    let bottom = region
        .y
        .saturating_add(region.height.saturating_sub(1))
        .min(image.height() - 1);
    for inset in 0..3_u32 {
        let x1 = left.saturating_add(inset).min(right);
        let x2 = right.saturating_sub(inset).max(left);
        let y1 = top.saturating_add(inset).min(bottom);
        let y2 = bottom.saturating_sub(inset).max(top);
        for x in x1..=x2 {
            image.put_pixel(x, y1, image::Rgba([255, 40, 80, 255]));
            image.put_pixel(x, y2, image::Rgba([255, 40, 80, 255]));
        }
        for y in y1..=y2 {
            image.put_pixel(x1, y, image::Rgba([255, 40, 80, 255]));
            image.put_pixel(x2, y, image::Rgba([255, 40, 80, 255]));
        }
    }
}

pub(super) fn spawn_write(path: std::path::PathBuf, bytes: Vec<u8>) {
    std::thread::Builder::new()
        .name("sgt-screen-translate-evidence-source".to_string())
        .spawn(move || {
            if let Err(error) = std::fs::write(path, bytes) {
                crate::log_info!("[Screen Translate] source evidence failed: {error}");
            }
        })
        .ok();
}

pub(super) fn save_jpeg(path: &std::path::Path, image: &image::RgbaImage) -> Result<()> {
    let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
    let file = std::fs::File::create(path)?;
    JpegEncoder::new_with_quality(file, 88).write_image(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        ExtendedColorType::Rgb8,
    )?;
    Ok(())
}

/// The diagnostic viewer consumes JPEG regardless of the worker input codec.
/// Called only after development evidence capture has been explicitly enabled.
pub(super) fn source_jpeg(capture: &CapturedRegion, encoded: &[u8]) -> Result<Vec<u8>> {
    if !encoded.starts_with(b"\x89PNG") {
        return Ok(encoded.to_vec());
    }
    let rgb = image::DynamicImage::ImageRgba8(capture.image.clone()).to_rgb8();
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 95).write_image(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        ExtendedColorType::Rgb8,
    )?;
    Ok(bytes)
}
