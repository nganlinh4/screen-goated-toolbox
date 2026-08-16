//! Debug evidence from the worker before host readability filtering.

use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder as _};
use sgt_screen_text_detector_protocol::DetectedRegion;

pub(super) fn save(directory: &Path, source_jpeg: &[u8], regions: &[DetectedRegion]) {
    let directory = directory.to_path_buf();
    let source = source_jpeg.to_vec();
    let regions = regions.to_vec();
    let _ = std::thread::Builder::new()
        .name("sgt-screen-translate-evidence-raw-detector".to_string())
        .spawn(move || {
            if let Err(error) = save_inner(&directory, &source, &regions) {
                crate::log_info!("[Screen Translate] raw detector evidence failed: {error:#}");
            }
        });
}

fn save_inner(
    directory: &Path,
    source_jpeg: &[u8],
    regions: &[DetectedRegion],
) -> anyhow::Result<()> {
    let records = regions
        .iter()
        .map(|region| serde_json::json!({
            "boxPx": [region.left, region.top, region.right - region.left, region.bottom - region.top],
            "locatorConfidence": region.confidence,
            "primaryText": region.text,
            "primaryConfidence": region.text_confidence,
            "alternatives": region.alternatives.iter().map(|alternative| serde_json::json!({
                "text": alternative.text,
                "confidence": alternative.confidence
            })).collect::<Vec<_>>()
        }))
        .collect::<Vec<_>>();
    std::fs::write(
        directory.join("detector-raw.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    let mut image = image::load_from_memory(source_jpeg)?.to_rgba8();
    for region in regions {
        draw_box(&mut image, region);
    }
    let rgb = image::DynamicImage::ImageRgba8(image).to_rgb8();
    let file = std::fs::File::create(directory.join("detector-raw.jpg"))?;
    JpegEncoder::new_with_quality(file, 88).write_image(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        ExtendedColorType::Rgb8,
    )?;
    Ok(())
}

fn draw_box(image: &mut image::RgbaImage, region: &DetectedRegion) {
    if image.width() == 0 || image.height() == 0 {
        return;
    }
    let left = region.left.min(image.width() - 1);
    let top = region.top.min(image.height() - 1);
    let right = region.right.saturating_sub(1).min(image.width() - 1);
    let bottom = region.bottom.saturating_sub(1).min(image.height() - 1);
    for x in left..=right {
        image.put_pixel(x, top, image::Rgba([255, 180, 0, 255]));
        image.put_pixel(x, bottom, image::Rgba([255, 180, 0, 255]));
    }
    for y in top..=bottom {
        image.put_pixel(left, y, image::Rgba([255, 180, 0, 255]));
        image.put_pixel(right, y, image::Rgba([255, 180, 0, 255]));
    }
}
