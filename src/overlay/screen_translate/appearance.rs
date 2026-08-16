use serde::{Deserialize, Serialize};

use super::contract::DetectedTextRegion;
use super::geometry::PixelRegion;

const BACKGROUND_BUCKETS: usize = 512;
const RELIABLE_BACKGROUND_PERCENT: u8 = 28;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VisualSignature {
    pub background_rgb: [u8; 3],
    pub background_confidence: u8,
    pub foreground_rgb: Option<[u8; 3]>,
    pub foreground_confidence: u8,
}

pub(crate) fn annotate_regions(image: &image::RgbaImage, regions: &mut [DetectedTextRegion]) {
    for region in regions {
        let pixels =
            super::geometry::normalized_region(region.bounds, image.width(), image.height());
        region.appearance = analyze_region(image, pixels);
    }
}

pub(crate) fn analyze_region(
    image: &image::RgbaImage,
    region: PixelRegion,
) -> Option<VisualSignature> {
    let region = clipped(region, image.width(), image.height())?;
    let background = strongest_background(image, region)?;
    let glyphs = glyph_mask_with_background(image, region, background.0, background.1);
    let mut buckets = [ColorBucket::default(); BACKGROUND_BUCKETS];
    let mut glyph_count = 0_u32;
    for y in 0..region.height {
        for x in 0..region.width {
            if !glyphs[(y * region.width + x) as usize] {
                continue;
            }
            glyph_count += 1;
            let pixel = *image.get_pixel(region.x + x, region.y + y);
            let distance = color_distance(rgb(pixel.0), background.0).max(1);
            let bucket = &mut buckets[bucket_index(pixel.0)];
            let weight = u64::from(distance).pow(2);
            bucket.score = bucket.score.saturating_add(weight);
            bucket.weight = bucket.weight.saturating_add(weight);
            for (total, channel) in bucket.rgb.iter_mut().zip(pixel.0) {
                *total = total.saturating_add(u64::from(channel) * weight);
            }
        }
    }
    let foreground = buckets
        .iter()
        .max_by_key(|bucket| bucket.score)
        .filter(|bucket| bucket.weight > 0)
        .map(|bucket| {
            bucket
                .rgb
                .map(|value| (value / bucket.weight).min(255) as u8)
        });
    let area = region.width.saturating_mul(region.height).max(1);
    Some(VisualSignature {
        background_rgb: background.0,
        background_confidence: background.1,
        foreground_rgb: foreground,
        foreground_confidence: percent(glyph_count, area),
    })
}

#[cfg(test)]
fn glyph_mask(image: &image::RgbaImage, region: PixelRegion) -> Vec<bool> {
    let Some(region) = clipped(region, image.width(), image.height()) else {
        return Vec::new();
    };
    let Some((background, confidence)) = strongest_background(image, region) else {
        return vec![false; (region.width * region.height) as usize];
    };
    glyph_mask_with_background(image, region, background, confidence)
}

pub(crate) fn color_hex(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

fn glyph_mask_with_background(
    image: &image::RgbaImage,
    region: PixelRegion,
    background: [u8; 3],
    background_confidence: u8,
) -> Vec<bool> {
    let mut mask = vec![false; (region.width * region.height) as usize];
    let radius = region.height.div_ceil(5).clamp(2, 5);
    for y in 0..region.height {
        for x in 0..region.width {
            let image_x = region.x + x;
            let image_y = region.y + y;
            let pixel = image.get_pixel(image_x, image_y).0;
            if pixel[3] < 128 {
                continue;
            }
            let local = ring_median(image, image_x, image_y, radius);
            let local_distance = color_distance(rgb(pixel), local);
            let global_distance = color_distance(rgb(pixel), background);
            let edge = local_edge(image, image_x, image_y);
            let locally_distinct = local_distance >= 22 && edge >= 12;
            let globally_supported =
                background_confidence < RELIABLE_BACKGROUND_PERCENT || global_distance >= 12;
            if locally_distinct && globally_supported {
                mask[(y * region.width + x) as usize] = true;
            }
        }
    }
    dilate(&mask, region.width, region.height)
}

fn dominant_color(image: &image::RgbaImage, region: PixelRegion) -> Option<([u8; 3], u8)> {
    let mut counts = [0_u32; BACKGROUND_BUCKETS];
    let mut totals = [[0_u64; 3]; BACKGROUND_BUCKETS];
    let mut pixels = 0_u32;
    for y in region.y..region.y + region.height {
        for x in region.x..region.x + region.width {
            let pixel = image.get_pixel(x, y).0;
            if pixel[3] < 128 {
                continue;
            }
            let index = bucket_index(pixel);
            counts[index] += 1;
            pixels += 1;
            for (total, channel) in totals[index].iter_mut().zip(pixel) {
                *total += u64::from(channel);
            }
        }
    }
    let (index, count) = counts
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|(_, count)| *count)?;
    if count == 0 {
        return None;
    }
    Some((
        totals[index].map(|total| (total / u64::from(count)).min(255) as u8),
        percent(count, pixels.max(1)),
    ))
}

fn strongest_background(image: &image::RgbaImage, region: PixelRegion) -> Option<([u8; 3], u8)> {
    match (
        surrounding_color(image, region),
        dominant_color(image, region),
    ) {
        (Some(surrounding), Some(interior)) => Some(if interior.1 > surrounding.1 {
            interior
        } else {
            surrounding
        }),
        (surrounding, interior) => surrounding.or(interior),
    }
}

fn surrounding_color(image: &image::RgbaImage, region: PixelRegion) -> Option<([u8; 3], u8)> {
    let sample = super::geometry::background_sample_region(region, image.width(), image.height());
    let mut counts = [0_u32; BACKGROUND_BUCKETS];
    let mut totals = [[0_u64; 3]; BACKGROUND_BUCKETS];
    let mut pixels = 0_u32;
    for y in sample.y..sample.y.saturating_add(sample.height) {
        for x in sample.x..sample.x.saturating_add(sample.width) {
            if x >= region.x
                && x < region.x.saturating_add(region.width)
                && y >= region.y
                && y < region.y.saturating_add(region.height)
            {
                continue;
            }
            let pixel = image.get_pixel(x, y).0;
            if pixel[3] < 128 {
                continue;
            }
            let index = bucket_index(pixel);
            counts[index] += 1;
            pixels += 1;
            for (total, channel) in totals[index].iter_mut().zip(pixel) {
                *total += u64::from(channel);
            }
        }
    }
    let (index, count) = counts
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|(_, count)| *count)?;
    (count > 0).then(|| {
        (
            totals[index].map(|total| (total / u64::from(count)).min(255) as u8),
            percent(count, pixels.max(1)),
        )
    })
}

fn ring_median(image: &image::RgbaImage, x: u32, y: u32, radius: u32) -> [u8; 3] {
    let left = x.saturating_sub(radius);
    let right = x
        .saturating_add(radius)
        .min(image.width().saturating_sub(1));
    let top = y.saturating_sub(radius);
    let bottom = y
        .saturating_add(radius)
        .min(image.height().saturating_sub(1));
    let points = [
        (left, top),
        (x, top),
        (right, top),
        (left, y),
        (right, y),
        (left, bottom),
        (x, bottom),
        (right, bottom),
    ];
    let mut channels = [[0_u8; 8]; 3];
    for (index, (sample_x, sample_y)) in points.into_iter().enumerate() {
        let pixel = image.get_pixel(sample_x, sample_y).0;
        for channel in 0..3 {
            channels[channel][index] = pixel[channel];
        }
    }
    channels.map(|mut values| {
        values.sort_unstable();
        (u16::from(values[3]) + u16::from(values[4])).div_ceil(2) as u8
    })
}

fn local_edge(image: &image::RgbaImage, x: u32, y: u32) -> u8 {
    let center = image.get_pixel(x, y).0;
    let mut edge = 0;
    for (neighbor_x, neighbor_y) in [
        (x.saturating_sub(1), y),
        (x.saturating_add(1).min(image.width() - 1), y),
        (x, y.saturating_sub(1)),
        (x, y.saturating_add(1).min(image.height() - 1)),
    ] {
        edge = edge.max(color_distance(
            rgb(center),
            rgb(image.get_pixel(neighbor_x, neighbor_y).0),
        ));
    }
    edge
}

fn dilate(mask: &[bool], width: u32, height: u32) -> Vec<bool> {
    let mut result = mask.to_vec();
    for y in 0..height {
        for x in 0..width {
            if !mask[(y * width + x) as usize] {
                continue;
            }
            for next_y in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for next_x in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    result[(next_y * width + next_x) as usize] = true;
                }
            }
        }
    }
    result
}

fn clipped(region: PixelRegion, width: u32, height: u32) -> Option<PixelRegion> {
    let right = region.x.saturating_add(region.width).min(width);
    let bottom = region.y.saturating_add(region.height).min(height);
    (right > region.x && bottom > region.y).then_some(PixelRegion {
        x: region.x,
        y: region.y,
        width: right - region.x,
        height: bottom - region.y,
    })
}

fn bucket_index(color: [u8; 4]) -> usize {
    usize::from(color[0] >> 5) * 64 + usize::from(color[1] >> 5) * 8 + usize::from(color[2] >> 5)
}

fn color_distance(left: [u8; 3], right: [u8; 3]) -> u8 {
    left[0]
        .abs_diff(right[0])
        .max(left[1].abs_diff(right[1]))
        .max(left[2].abs_diff(right[2]))
}

fn rgb(color: [u8; 4]) -> [u8; 3] {
    [color[0], color[1], color[2]]
}

fn percent(value: u32, total: u32) -> u8 {
    ((u64::from(value) * 100 + u64::from(total) / 2) / u64::from(total)).min(100) as u8
}

#[derive(Clone, Copy, Default)]
struct ColorBucket {
    score: u64,
    rgb: [u64; 3],
    weight: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_mask_preserves_clean_pixels_on_adjacent_surfaces() {
        let mut image = image::RgbaImage::from_fn(80, 28, |x, _| {
            if x < 40 {
                image::Rgba([250, 250, 250, 255])
            } else {
                image::Rgba([40, 70, 100, 255])
            }
        });
        for y in 7..21 {
            for x in [12, 13, 14, 52, 53, 54] {
                image.put_pixel(x, y, image::Rgba([180, 30, 70, 255]));
            }
        }
        let mask = glyph_mask(
            &image,
            PixelRegion {
                x: 0,
                y: 0,
                width: 80,
                height: 28,
            },
        );
        assert!(mask[12 + 12 * 80]);
        assert!(mask[52 + 12 * 80]);
        assert!(!mask[25 + 12 * 80]);
        assert!(!mask[65 + 12 * 80]);
    }

    #[test]
    fn interior_surface_wins_when_the_sampling_ring_crosses_a_container_edge() {
        let mut image = image::RgbaImage::from_pixel(100, 50, image::Rgba([180, 205, 225, 255]));
        for y in 10..42 {
            for x in 12..88 {
                image.put_pixel(x, y, image::Rgba([254, 254, 254, 255]));
            }
        }
        for y in 12..26 {
            for x in 16..20 {
                image.put_pixel(x, y, image::Rgba([12, 12, 12, 255]));
            }
        }
        let signature = analyze_region(
            &image,
            PixelRegion {
                x: 14,
                y: 10,
                width: 30,
                height: 18,
            },
        )
        .unwrap();
        assert!(signature.background_rgb[0] > 240, "{signature:?}");
    }

    #[test]
    fn surrounding_surface_wins_when_glyphs_fill_the_detector_box() {
        let mut image = image::RgbaImage::from_pixel(80, 40, image::Rgba([18, 20, 22, 255]));
        for y in 12..28 {
            for x in 18..62 {
                if (x + y) % 3 != 0 {
                    image.put_pixel(x, y, image::Rgba([245, 245, 245, 255]));
                }
            }
        }
        let signature = analyze_region(
            &image,
            PixelRegion {
                x: 16,
                y: 10,
                width: 48,
                height: 20,
            },
        )
        .unwrap();
        assert_eq!(signature.background_rgb, [18, 20, 22]);
        assert!(signature.foreground_rgb.is_some());
    }
}
