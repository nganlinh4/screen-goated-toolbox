//! Bounded detector images with a mapping back to the original capture.
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use sgt_screen_text_detector_protocol::stream::{
    Event, MAX_CAPTURE_BYTES, MAX_CAPTURE_PIXELS as MAX_PIXELS, MAX_CAPTURE_SIDE as MAX_SIDE,
};

/// Admits exactly the frames the worker decodes, so a capture is never
/// downgraded for a limit the worker does not have.
pub(super) fn validate_detector_input(bytes: &[u8], width: u32, height: u32) -> Result<()> {
    if bytes.is_empty()
        || bytes.len() > MAX_CAPTURE_BYTES
        || width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        bail!("detector image exceeds the worker input limits");
    }
    Ok(())
}

pub(super) struct Payload {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    source_width: u32,
    source_height: u32,
}

struct Bounded {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}
impl Bounded {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            exceeded: false,
        }
    }
}
impl Write for Bounded {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(io::Error::other("detector image exceeds frame limit"));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn dimensions(width: u32, height: u32) -> Result<(u32, u32)> {
    if width == 0 || height == 0 {
        bail!("empty detector image");
    }
    let scale = (f64::from(MAX_SIDE) / f64::from(width))
        .min(f64::from(MAX_SIDE) / f64::from(height))
        .min((MAX_PIXELS as f64 / (f64::from(width) * f64::from(height))).sqrt())
        .min(1.0);
    Ok((
        (f64::from(width) * scale).floor().max(1.0) as u32,
        (f64::from(height) * scale).floor().max(1.0) as u32,
    ))
}

/// Lossless PNG when it fits the worker frame, then bounded JPEG, then smaller.
pub(super) fn prepare_png(image: &RgbaImage, cancelled: &AtomicBool) -> Result<Payload> {
    prepare_within(image, cancelled, MAX_CAPTURE_BYTES)
}

fn prepare_within(image: &RgbaImage, cancelled: &AtomicBool, limit: usize) -> Result<Payload> {
    let source_width = image.width();
    let source_height = image.height();
    let (mut width, mut height) = dimensions(source_width, source_height)?;
    loop {
        if cancelled.load(Ordering::Acquire) {
            bail!("screen translation cancelled");
        }
        let resized;
        let current = if (width, height) == (source_width, source_height) {
            image
        } else {
            resized = image::imageops::resize(
                image,
                width,
                height,
                image::imageops::FilterType::Lanczos3,
            );
            &resized
        };
        let mut png = Bounded::new(limit);
        let encoded = image::codecs::png::PngEncoder::new_with_quality(
            &mut png,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::Sub,
        )
        .write_image(current.as_raw(), width, height, ExtendedColorType::Rgba8);
        if encoded.is_ok() {
            if cancelled.load(Ordering::Acquire) {
                bail!("screen translation cancelled");
            }
            return Ok(Payload {
                bytes: png.bytes,
                width,
                height,
                source_width,
                source_height,
            });
        }
        if !png.exceeded {
            encoded.context("encode detector PNG")?;
        }
        let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
        for pixel in current.as_raw().as_chunks::<4>().0 {
            rgb.extend_from_slice(&pixel[..3]);
        }
        for quality in [88, 70, 50] {
            if cancelled.load(Ordering::Acquire) {
                bail!("screen translation cancelled");
            }
            let mut jpeg = Bounded::new(limit);
            let encoded = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
                .encode(&rgb, width, height, ExtendedColorType::Rgb8);
            if encoded.is_ok() {
                if cancelled.load(Ordering::Acquire) {
                    bail!("screen translation cancelled");
                }
                return Ok(Payload {
                    bytes: jpeg.bytes,
                    width,
                    height,
                    source_width,
                    source_height,
                });
            }
            if !jpeg.exceeded {
                encoded.context("encode detector JPEG")?;
            }
        }
        if width == 1 && height == 1 {
            bail!("detector image cannot fit frame limit");
        }
        width = (u64::from(width) * 3 / 4).max(1) as u32;
        height = (u64::from(height) * 3 / 4).max(1) as u32;
    }
}

impl Payload {
    pub fn map_event(&self, event: &mut Event) {
        let Event::Geometry {
            width,
            height,
            regions,
            layout,
        } = event
        else {
            return;
        };
        let x = self.source_width as f64 / self.width as f64;
        let y = self.source_height as f64 / self.height as f64;
        for region in regions {
            for point in &mut region.quad {
                point[0] = (f64::from(point[0]) * x) as f32;
                point[1] = (f64::from(point[1]) * y) as f32;
            }
        }
        for region in layout {
            for (index, value) in region.bounds.iter_mut().enumerate() {
                *value = (f64::from(*value) * if index % 2 == 0 { x } else { y }) as f32;
            }
        }
        *width = self.source_width;
        *height = self.source_height;
    }
}

#[cfg(test)]
#[path = "image_payload_tests.rs"]
mod tests;
