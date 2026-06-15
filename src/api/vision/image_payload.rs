use crate::api::providers::Provider;
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use image::{ImageBuffer, Rgba};
use std::io::Cursor;

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
) -> Result<PreparedImage> {
    if Provider::from_wire(provider) == Some(Provider::Google)
        && let Some(bytes) = original_bytes
    {
        println!("DEBUG: Zero-Copy optimization active for Google provider");
        let mime_type = sniff_mime_type(&bytes);
        println!("DEBUG: Detected MIME type: {}", mime_type);
        return Ok(PreparedImage {
            b64_image: general_purpose::STANDARD.encode(&bytes),
            image_data: Vec::new(),
            mime_type,
            original_bytes: Some(bytes),
        });
    }

    let mut final_image = image;
    let max_dim = 2048;
    if provider != "google" && (final_image.width() > max_dim || final_image.height() > max_dim) {
        println!("DEBUG: Image exceeds {}px, resizing...", max_dim);
        let (new_width, new_height) = resized_dimensions(final_image.width(), final_image.height());
        final_image = image::imageops::resize(
            &final_image,
            new_width,
            new_height,
            image::imageops::FilterType::Lanczos3,
        );
        println!(
            "DEBUG: Resized to: {}x{}",
            final_image.width(),
            final_image.height()
        );
    }

    let mut image_data = Vec::new();
    final_image.write_to(&mut Cursor::new(&mut image_data), image::ImageFormat::Png)?;
    Ok(PreparedImage {
        b64_image: general_purpose::STANDARD.encode(&image_data),
        image_data,
        mime_type: "image/png".to_string(),
        original_bytes,
    })
}

fn resized_dimensions(width: u32, height: u32) -> (u32, u32) {
    let max_dim = 2048;
    if width > height {
        let ratio = max_dim as f32 / width as f32;
        (max_dim, (height as f32 * ratio) as u32)
    } else {
        let ratio = max_dim as f32 / height as f32;
        ((width as f32 * ratio) as u32, max_dim)
    }
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
