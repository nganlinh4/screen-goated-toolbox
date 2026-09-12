use anyhow::{Context, Result};
use base64::Engine as _;
use image::ImageEncoder as _;
use image::codecs::png::PngEncoder;

use super::geometry::{PixelRegion, background_sample_region};
fn reconstruct_blob_image_with_background(
    image: &image::RgbaImage,
    target: PixelRegion,
    text_regions: &[PixelRegion],
    background: Option<([u8; 3], u8)>,
) -> image::RgbaImage {
    let sample = background_sample_region(target, image.width(), image.height());
    let context = image::imageops::crop_imm(image, sample.x, sample.y, sample.width, sample.height)
        .to_image();
    let trusted_background = background
        .filter(|(_, confidence)| *confidence >= super::appearance::RELIABLE_BACKGROUND_PERCENT)
        .map(|(rgb, _)| image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let reconstructed = inpaint_regions(&context, sample, text_regions, trusted_background);
    image::imageops::crop_imm(
        &reconstructed,
        target.x - sample.x,
        target.y - sample.y,
        target.width,
        target.height,
    )
    .to_image()
}
pub(super) fn reconstruct_shaped_blob(
    image: &image::RgbaImage,
    target: PixelRegion,
    text_regions: &[PixelRegion],
    shape_regions: &[PixelRegion],
    background: Option<([u8; 3], u8)>,
) -> image::RgbaImage {
    let mut repaired =
        reconstruct_blob_image_with_background(image, target, text_regions, background);
    // Rasterize the rectangles once. Testing every fragment at every pixel
    // becomes expensive when a source footprint contains several cutouts.
    let width = repaired.width() as usize;
    let mut owned = vec![false; width * repaired.height() as usize];
    for region in shape_regions {
        let left = region.x.saturating_sub(target.x).min(target.width) as usize;
        let top = region.y.saturating_sub(target.y).min(target.height) as usize;
        let right = region
            .x
            .saturating_add(region.width)
            .saturating_sub(target.x)
            .min(target.width) as usize;
        let bottom = region
            .y
            .saturating_add(region.height)
            .saturating_sub(target.y)
            .min(target.height) as usize;
        for y in top..bottom {
            owned[y * width + left..y * width + right].fill(true);
        }
    }
    for (pixel, owned) in repaired.pixels_mut().zip(owned) {
        if !owned {
            pixel.0[3] = 0;
        }
    }
    repaired
}
pub(super) fn encode_data_url(image: &image::RgbaImage) -> Result<String> {
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .context("reconstructed region encoding failed")?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}
fn inpaint_regions(
    context: &image::RgbaImage,
    sample: PixelRegion,
    regions: &[PixelRegion],
    trusted_background: Option<image::Rgba<u8>>,
) -> image::RgbaImage {
    let width = context.width();
    let height = context.height();
    if width < 3 || height < 3 {
        return context.clone();
    }
    let mut mask = vec![false; (width * height) as usize];
    for region in regions {
        let left = region.x.saturating_sub(1).max(sample.x);
        let top = region.y.saturating_sub(1).max(sample.y);
        let right = region
            .x
            .saturating_add(region.width)
            .saturating_add(1)
            .min(sample.x.saturating_add(sample.width));
        let bottom = region
            .y
            .saturating_add(region.height)
            .saturating_add(1)
            .min(sample.y.saturating_add(sample.height));
        for y in top.saturating_sub(sample.y)..bottom.saturating_sub(sample.y) {
            for x in left.saturating_sub(sample.x)..right.saturating_sub(sample.x) {
                mask[(y * width + x) as usize] = true;
            }
        }
    }
    let mut known = 0u64;
    for (index, _) in context.pixels().enumerate() {
        if mask[index] {
            continue;
        }
        known += 1;
    }
    if known == 0 || !mask.iter().any(|masked| *masked) {
        return context.clone();
    }
    if let Some(background) = trusted_background {
        let mut result = context.clone();
        for (index, pixel) in result.pixels_mut().enumerate() {
            if mask[index] {
                *pixel = background;
            }
        }
        return result;
    }
    let directions = nearest_known_pixels(&mask, width, height);
    let mut result = context.clone();
    for (index, pixel) in result.pixels_mut().enumerate() {
        if mask[index] {
            *pixel = interpolate_background(
                context,
                index,
                [
                    directions[0][index],
                    directions[1][index],
                    directions[2][index],
                    directions[3][index],
                ],
            );
        }
    }
    result
}
fn nearest_known_pixels(mask: &[bool], width: u32, height: u32) -> [Vec<usize>; 4] {
    let mut maps = std::array::from_fn(|_| vec![usize::MAX; mask.len()]);
    for y in 0..height as usize {
        let row = y * width as usize;
        let mut known = usize::MAX;
        for x in 0..width as usize {
            let index = row + x;
            if !mask[index] {
                known = index;
            }
            maps[0][index] = known;
        }
        known = usize::MAX;
        for x in (0..width as usize).rev() {
            let index = row + x;
            if !mask[index] {
                known = index;
            }
            maps[1][index] = known;
        }
    }
    for x in 0..width as usize {
        let mut known = usize::MAX;
        for y in 0..height as usize {
            let index = y * width as usize + x;
            if !mask[index] {
                known = index;
            }
            maps[2][index] = known;
        }
        known = usize::MAX;
        for y in (0..height as usize).rev() {
            let index = y * width as usize + x;
            if !mask[index] {
                known = index;
            }
            maps[3][index] = known;
        }
    }
    maps
}
fn interpolate_background(
    image: &image::RgbaImage,
    target: usize,
    directions: [usize; 4],
) -> image::Rgba<u8> {
    let width = image.width() as usize;
    let estimates = [
        directional_estimate(image, target, directions[0], directions[1]),
        directional_estimate(image, target, directions[2], directions[3]),
    ];
    let mut total = [0.0; 4];
    let mut total_weight = 0.0;
    for (color, weight) in estimates.into_iter().flatten() {
        for (sum, channel) in total.iter_mut().zip(color) {
            *sum += channel * weight;
        }
        total_weight += weight;
    }
    if total_weight == 0.0 {
        let x = target % width;
        let y = target / width;
        return *image.get_pixel(x as u32, y as u32);
    }
    image::Rgba(total.map(|channel| (channel / total_weight).round().clamp(0.0, 255.0) as u8))
}
fn directional_estimate(
    image: &image::RgbaImage,
    target: usize,
    before: usize,
    after: usize,
) -> Option<([f64; 4], f64)> {
    if before == usize::MAX && after == usize::MAX {
        return None;
    }
    let width = image.width() as usize;
    let target_xy = (target % width, target / width);
    let distance = |index: usize| {
        let xy = (index % width, index / width);
        (target_xy.0.abs_diff(xy.0) + target_xy.1.abs_diff(xy.1)).max(1) as f64
    };
    let pixel =
        |index: usize| -> [u8; 4] { image.as_raw()[index * 4..index * 4 + 4].try_into().unwrap() };
    let (color, span, disagreement) = match (before, after) {
        (usize::MAX, index) | (index, usize::MAX) => {
            (pixel(index).map(f64::from), distance(index) * 4.0, 64.0)
        }
        (before, after) => {
            let before_color: [u8; 4] = pixel(before);
            let after_color: [u8; 4] = pixel(after);
            let before_distance = distance(before);
            let after_distance = distance(after);
            let span = before_distance + after_distance;
            let color = std::array::from_fn(|channel| {
                (f64::from(before_color[channel]) * after_distance
                    + f64::from(after_color[channel]) * before_distance)
                    / span
            });
            let disagreement = before_color[..3]
                .iter()
                .zip(&after_color[..3])
                .map(|(left, right)| left.abs_diff(*right))
                .max()
                .unwrap_or(0) as f64;
            (color, span, disagreement)
        }
    };
    let edge_penalty = 1.0 + disagreement / 16.0;
    Some((color, 1.0 / (span.sqrt() * edge_penalty * edge_penalty)))
}
fn contrast_color(image: &image::RgbaImage) -> String {
    let (total, count) = image.pixels().fold((0.0f64, 0u64), |(sum, count), pixel| {
        if pixel[3] < 128 {
            (sum, count)
        } else {
            (
                sum + relative_luminance([pixel[0], pixel[1], pixel[2]]),
                count + 1,
            )
        }
    });
    if count > 0 && total / count as f64 > 0.42 {
        "#111111".to_string()
    } else {
        "#FFFFFF".to_string()
    }
}
#[derive(Clone, Copy, Default)]
struct ColorBucket {
    score: u64,
    weighted_rgb: [u64; 3],
    weight: u64,
    samples: u32,
}

pub(super) fn foreground_color(source: &image::RgbaImage, backdrop: &image::RgbaImage) -> String {
    if source.dimensions() != backdrop.dimensions() || source.is_empty() {
        return contrast_color(backdrop);
    }
    let threshold = difference_threshold(source, backdrop);
    let mut buckets = [ColorBucket::default(); 512];
    for y in 0..source.height() {
        for x in 0..source.width() {
            let pixel = *source.get_pixel(x, y);
            if pixel[3] < 128 {
                continue;
            }
            let difference = color_distance(pixel, *backdrop.get_pixel(x, y));
            if difference < threshold {
                continue;
            }
            let edge = local_edge(source, x, y);
            let weight = u64::from(difference).pow(2) * u64::from(edge + 24);
            let index = usize::from(pixel[0] >> 5) * 64
                + usize::from(pixel[1] >> 5) * 8
                + usize::from(pixel[2] >> 5);
            let bucket = &mut buckets[index];
            bucket.score = bucket.score.saturating_add(weight);
            bucket.weight = bucket.weight.saturating_add(weight);
            bucket.samples += 1;
            for (total, channel) in bucket.weighted_rgb.iter_mut().zip(pixel.0) {
                *total = total.saturating_add(u64::from(channel) * weight);
            }
        }
    }
    let Some(bucket) = buckets.iter().max_by_key(|bucket| bucket.score) else {
        return contrast_color(backdrop);
    };
    if bucket.samples < 2 || bucket.weight == 0 {
        return contrast_color(backdrop);
    }
    let color = bucket
        .weighted_rgb
        .map(|total| (total / bucket.weight).min(255) as u8);
    let (background_luminance, background_pixels) = backdrop
        .pixels()
        .filter(|pixel| pixel[3] >= 128)
        .fold((0.0, 0_u64), |(sum, count), pixel| {
            (
                sum + relative_luminance([pixel[0], pixel[1], pixel[2]]),
                count + 1,
            )
        });
    if background_pixels > 0 {
        let background_luminance = background_luminance / background_pixels as f64;
        let foreground_luminance = relative_luminance(color);
        let contrast = (foreground_luminance.max(background_luminance) + 0.05)
            / (foreground_luminance.min(background_luminance) + 0.05);
        if contrast < 3.0 {
            return contrast_color(backdrop);
        }
    }
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

fn difference_threshold(source: &image::RgbaImage, backdrop: &image::RgbaImage) -> u16 {
    // Distances have only 256 values. Preserve the exact order statistic
    // without allocating and sorting one entry per source pixel.
    let mut counts = [0_usize; 256];
    for (pixel, background) in source.pixels().zip(backdrop.pixels()) {
        counts[usize::from(color_distance(*pixel, *background))] += 1;
    }
    let rank = source.as_raw().len() / 4 * 2 / 3;
    let mut cumulative = 0;
    for (difference, count) in counts.into_iter().enumerate() {
        cumulative += count;
        if cumulative > rank {
            return (difference as u16).max(16);
        }
    }
    16
}

fn relative_luminance(rgb: [u8; 3]) -> f64 {
    let linear = |value: u8| {
        let channel = f64::from(value) / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

fn color_distance(left: image::Rgba<u8>, right: image::Rgba<u8>) -> u16 {
    left[0]
        .abs_diff(right[0])
        .max(left[1].abs_diff(right[1]))
        .max(left[2].abs_diff(right[2]))
        .into()
}

fn local_edge(image: &image::RgbaImage, x: u32, y: u32) -> u16 {
    let center = *image.get_pixel(x, y);
    let mut edge = 0;
    if x > 0 {
        edge = edge.max(color_distance(center, *image.get_pixel(x - 1, y)));
    }
    if x + 1 < image.width() {
        edge = edge.max(color_distance(center, *image.get_pixel(x + 1, y)));
    }
    if y > 0 {
        edge = edge.max(color_distance(center, *image.get_pixel(x, y - 1)));
    }
    if y + 1 < image.height() {
        edge = edge.max(color_distance(center, *image.get_pixel(x, y + 1)));
    }
    edge
}

#[cfg(test)]
mod tests;
