use crate::api::providers::Provider;
use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose};
use image::{ExtendedColorType, ImageBuffer, Rgba, codecs::jpeg::JpegEncoder};
use std::io::Cursor;

const MAX_DIMENSION: u32 = 2048;
const GROQ_SAFE_REQUEST_BYTES: usize = 3_800_000;
const GROQ_JSON_RESERVE_BYTES: usize = 16_384;
const GROQ_MAX_IMAGE_BYTES: usize = 2_500_000;
const GROQ_MIN_IMAGE_BYTES: usize = 262_144;
const GROQ_JPEG_QUALITIES: [u8; 5] = [90, 82, 74, 66, 58];
const GROQ_RESIZE_DIMENSIONS: [u32; 6] = [2048, 1792, 1536, 1280, 1024, 768];

pub(super) struct PreparedImage {
    pub(super) b64_image: String,
    pub(super) image_data: Vec<u8>,
    pub(super) mime_type: String,
    pub(super) original_bytes: Option<Vec<u8>>,
}

pub(super) fn prepare_image_payload(
    provider: &str,
    image: ImageBuffer<Rgba<u8>, Vec<u8>>,
    original_bytes: Option<Vec<u8>>,
    prompt_bytes: usize,
) -> Result<PreparedImage> {
    let provider = Provider::from_wire(provider);
    if provider == Some(Provider::Google)
        && let Some(bytes) = original_bytes
    {
        println!("DEBUG: Zero-Copy optimization active for Google provider");
        let mime_type = sniff_mime_type(&bytes);
        println!("DEBUG: Detected MIME type: {mime_type}");
        return Ok(prepared(bytes, mime_type, None));
    }

    let resized = resize_to_max(&image, MAX_DIMENSION);
    let png = encode_png(&resized)?;
    if provider != Some(Provider::Groq) {
        return Ok(prepared(png, "image/png".to_string(), original_bytes));
    }

    let budget = groq_image_byte_budget(prompt_bytes)?;
    if png.len() <= budget {
        println!(
            "DEBUG: Groq vision PNG fits budget: {} <= {budget} bytes",
            png.len()
        );
        return Ok(prepared(png, "image/png".to_string(), original_bytes));
    }

    for max_dimension in GROQ_RESIZE_DIMENSIONS {
        let candidate = resize_to_max(&resized, max_dimension);
        for quality in GROQ_JPEG_QUALITIES {
            let jpeg = encode_jpeg(&candidate, quality)?;
            if jpeg.len() <= budget {
                println!(
                    "DEBUG: Groq vision image compressed: {}x{}, JPEG q{quality}, {} bytes (budget {budget})",
                    candidate.width(),
                    candidate.height(),
                    jpeg.len()
                );
                return Ok(prepared(jpeg, "image/jpeg".to_string(), original_bytes));
            }
        }
    }

    bail!("Groq vision image cannot fit the safe request-size budget")
}

fn prepared(bytes: Vec<u8>, mime_type: String, original_bytes: Option<Vec<u8>>) -> PreparedImage {
    PreparedImage {
        b64_image: general_purpose::STANDARD.encode(&bytes),
        image_data: bytes,
        mime_type,
        original_bytes,
    }
}

fn groq_image_byte_budget(prompt_bytes: usize) -> Result<usize> {
    let available_base64 = GROQ_SAFE_REQUEST_BYTES
        .checked_sub(GROQ_JSON_RESERVE_BYTES + prompt_bytes)
        .ok_or_else(|| anyhow::anyhow!("Prompt is too large for a Groq vision request"))?;
    let raw_budget = available_base64 / 4 * 3;
    if raw_budget < GROQ_MIN_IMAGE_BYTES {
        bail!("Prompt leaves too little room for a Groq vision image");
    }
    Ok(raw_budget.min(GROQ_MAX_IMAGE_BYTES))
}

fn resize_to_max(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    max_dimension: u32,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    if image.width() <= max_dimension && image.height() <= max_dimension {
        return image.clone();
    }
    let scale = max_dimension as f64 / image.width().max(image.height()) as f64;
    let width = (image.width() as f64 * scale).round().max(1.0) as u32;
    let height = (image.height() as f64 * scale).round().max(1.0) as u32;
    image::imageops::resize(image, width, height, image::imageops::FilterType::Lanczos3)
}

fn encode_png(image: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)?;
    Ok(bytes)
}

fn encode_jpeg(image: &ImageBuffer<Rgba<u8>, Vec<u8>>, quality: u8) -> Result<Vec<u8>> {
    let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, quality).encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        ExtendedColorType::Rgb8,
    )?;
    Ok(bytes)
}

fn sniff_mime_type(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg".to_string()
    } else if bytes.starts_with(&[0x89, 0x50, 0x4e, 0x47]) {
        "image/png".to_string()
    } else if bytes.len() >= 12
        && bytes.starts_with(&[0x52, 0x49, 0x46, 0x46])
        && bytes[8..12] == [0x57, 0x45, 0x42, 0x50]
    {
        "image/webp".to_string()
    } else {
        "image/png".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groq_limits_match_mobile_parity_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/preset-system/vision-payload.json"
        ))
        .unwrap();
        let groq = &fixture["groq"];
        assert_eq!(groq["safe_request_bytes"], GROQ_SAFE_REQUEST_BYTES);
        assert_eq!(groq["json_reserve_bytes"], GROQ_JSON_RESERVE_BYTES);
        assert_eq!(groq["maximum_encoded_image_bytes"], GROQ_MAX_IMAGE_BYTES);
        assert_eq!(groq["minimum_encoded_image_bytes"], GROQ_MIN_IMAGE_BYTES);
        assert_eq!(
            groq["jpeg_qualities"],
            serde_json::json!(GROQ_JPEG_QUALITIES)
        );
        assert_eq!(
            groq["resize_dimensions"],
            serde_json::json!(GROQ_RESIZE_DIMENSIONS)
        );
    }

    #[test]
    fn groq_keeps_small_png_and_real_mime() {
        let image = ImageBuffer::from_pixel(64, 64, Rgba([20, 40, 60, 255]));
        let result = prepare_image_payload("groq", image, None, 100).unwrap();
        assert_eq!(result.mime_type, "image/png");
        assert!(result.image_data.len() <= groq_image_byte_budget(100).unwrap());
    }

    #[test]
    fn groq_compresses_noisy_image_below_prompt_aware_budget() {
        let mut state = 0x1234_5678_u32;
        let image = ImageBuffer::from_fn(1200, 1200, |_, _| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            Rgba([state as u8, (state >> 8) as u8, (state >> 16) as u8, 255])
        });
        let prompt_bytes = 32_000;
        let result = prepare_image_payload("groq", image, None, prompt_bytes).unwrap();
        assert_eq!(result.mime_type, "image/jpeg");
        assert!(result.image_data.len() <= groq_image_byte_budget(prompt_bytes).unwrap());
    }

    #[test]
    fn groq_rejects_prompt_that_leaves_no_image_budget() {
        let result = groq_image_byte_budget(GROQ_SAFE_REQUEST_BYTES);
        assert!(result.is_err());
    }
}
