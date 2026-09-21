//! Conservative OCR scheduling: text-edge changes are urgent, other motion is rechecked.
use image::RgbaImage;
use sgt_screen_text_detector_protocol::stream::Event;
use std::time::{Duration, Instant};

const MAX_RESCAN: Duration = Duration::from_millis(600);

pub(super) struct TextWatch {
    samples: Vec<(u32, u32, u8)>,
    observed_at: Instant,
}

impl TextWatch {
    pub fn new(image: &RgbaImage, events: &[Event], now: Instant) -> Self {
        let mut samples = Vec::new();
        for event in events {
            if let Event::Geometry { regions, .. } = event {
                for region in regions {
                    let left = region
                        .quad
                        .iter()
                        .map(|p| p[0])
                        .fold(f32::INFINITY, f32::min)
                        .max(0.0) as u32;
                    let top = region
                        .quad
                        .iter()
                        .map(|p| p[1])
                        .fold(f32::INFINITY, f32::min)
                        .max(0.0) as u32;
                    let right = region.quad.iter().map(|p| p[0]).fold(0.0, f32::max).ceil() as u32;
                    let bottom = region.quad.iter().map(|p| p[1]).fold(0.0, f32::max).ceil() as u32;
                    for y in top..bottom.min(image.height().saturating_sub(1)) {
                        for x in left..right.min(image.width().saturating_sub(1)) {
                            let edge = edges(image, x, y);
                            // Include previously blank pixels too: added strokes matter just
                            // as much as removed strokes during typewriter growth.
                            samples.push((x, y, edge));
                        }
                    }
                }
            }
        }
        Self {
            samples,
            observed_at: now,
        }
    }

    pub fn needs_ocr(&self, image: &RgbaImage, now: Instant) -> bool {
        self.samples.is_empty()
            || now.saturating_duration_since(self.observed_at) >= MAX_RESCAN
            || self.samples.iter().any(|&(x, y, old)| {
                x + 1 >= image.width() || y + 1 >= image.height() || edges(image, x, y) != old
            })
    }
}

pub(super) fn same_edges(
    reference: &RgbaImage,
    current: &RgbaImage,
    bounds: super::super::contract::NormalizedBounds,
) -> bool {
    if reference.dimensions() != current.dimensions() {
        return false;
    }
    let r = super::super::geometry::normalized_region(bounds, current.width(), current.height());
    (r.y..(r.y + r.height).min(current.height().saturating_sub(1))).all(|y| {
        (r.x..(r.x + r.width).min(current.width().saturating_sub(1)))
            .all(|x| edges(reference, x, y) == edges(current, x, y))
    })
}

/// Strong visual absence only. Textured/ambiguous backgrounds remain OCR's decision.
pub(super) fn cleared(
    reference: &RgbaImage,
    current: &RgbaImage,
    bounds: super::super::contract::NormalizedBounds,
) -> bool {
    if reference.dimensions() != current.dimensions() {
        return false;
    }
    let r = super::super::geometry::normalized_region(bounds, current.width(), current.height());
    let mut original = 0;
    let mut remaining = 0;
    for y in r.y..(r.y + r.height).min(current.height().saturating_sub(1)) {
        for x in r.x..(r.x + r.width).min(current.width().saturating_sub(1)) {
            original += usize::from(edges(reference, x, y) != 0);
            remaining += usize::from(edges(current, x, y) != 0);
            if remaining > 1 {
                return false;
            }
        }
    }
    original >= 8 && remaining <= 1
}

// Directional chromatic contrast, not absolute brightness or a language/font assumption.
// Only scheduling uses this signature. OCR still owns text identity and disappearance.
fn edges(image: &RgbaImage, x: u32, y: u32) -> u8 {
    let center = image.get_pixel(x, y);
    let mut bits = 0;
    for (axis, pixel) in [image.get_pixel(x + 1, y), image.get_pixel(x, y + 1)]
        .iter()
        .enumerate()
    {
        for channel in 0..3 {
            let delta = i16::from(center[channel]) - i16::from(pixel[channel]);
            if delta > 32 {
                bits |= 1 << (axis * 2);
            }
            if delta < -32 {
                bits |= 2 << (axis * 2);
            }
        }
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_motion_is_bounded_but_glyph_changes_are_immediate() {
        let now = Instant::now();
        let mut image = RgbaImage::from_pixel(20, 10, image::Rgba([30, 30, 30, 255]));
        image.put_pixel(5, 5, image::Rgba([240, 240, 240, 255]));
        let watch = TextWatch {
            samples: vec![(5, 5, edges(&image, 5, 5))],
            observed_at: now,
        };
        image.put_pixel(18, 8, image::Rgba([255, 0, 0, 255]));
        assert!(!watch.needs_ocr(&image, now + Duration::from_millis(180)));
        assert!(watch.needs_ocr(&image, now + MAX_RESCAN));
        image.put_pixel(5, 5, image::Rgba([30, 30, 30, 255]));
        assert!(watch.needs_ocr(&image, now + Duration::from_millis(180)));
    }
    #[test]
    fn no_text_or_no_edges_never_suppresses_detection() {
        let now = Instant::now();
        let image = RgbaImage::new(20, 10);
        assert!(TextWatch::new(&image, &[], now).needs_ocr(&image, now));
    }

    #[test]
    fn new_strokes_and_strong_clearing_are_not_background_motion() {
        let now = Instant::now();
        let blank = RgbaImage::from_pixel(30, 20, image::Rgba([30, 30, 30, 255]));
        let mut text = blank.clone();
        for y in 3..16 {
            text.put_pixel(10, y, image::Rgba([240, 240, 240, 255]));
        }
        let bounds = [0, 0, 1000, 1000].into();
        assert!(cleared(&text, &blank, bounds));
        assert!(!cleared(&text, &text, bounds));
        assert!(!same_edges(&text, &blank, bounds));
        let watch = TextWatch {
            samples: vec![(9, 8, 0)],
            observed_at: now,
        };
        assert!(watch.needs_ocr(&text, now));
    }

    #[test]
    fn uniform_brightness_changes_do_not_change_glyph_edges() {
        let first = RgbaImage::from_pixel(30, 20, image::Rgba([30, 30, 30, 255]));
        let second = RgbaImage::from_pixel(30, 20, image::Rgba([80, 80, 80, 255]));
        assert!(same_edges(&first, &second, [0, 0, 1000, 1000].into()));
        assert!(!cleared(&first, &second, [0, 0, 1000, 1000].into()));
    }
}
