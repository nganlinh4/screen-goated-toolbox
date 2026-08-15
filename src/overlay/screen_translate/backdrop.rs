use anyhow::{Context, Result};
use base64::Engine as _;
use image::ImageEncoder as _;
use image::codecs::png::PngEncoder;

use super::geometry::{PixelRegion, background_sample_region};

pub(super) fn reconstruct_blob_image_with_background(
    image: &image::RgbaImage,
    target: PixelRegion,
    text_regions: &[PixelRegion],
    background: Option<([u8; 3], u8)>,
) -> (image::RgbaImage, String) {
    let sample = background_sample_region(target, image.width(), image.height());
    let context = image::imageops::crop_imm(image, sample.x, sample.y, sample.width, sample.height)
        .to_image();
    let trusted_background = background
        .filter(|(_, confidence)| *confidence >= 60)
        .map(|(rgb, _)| image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let reconstructed = inpaint_regions(&context, sample, text_regions, trusted_background);
    let repaired = image::imageops::crop_imm(
        &reconstructed,
        target.x - sample.x,
        target.y - sample.y,
        target.width,
        target.height,
    )
    .to_image();
    let source = image::imageops::crop_imm(image, target.x, target.y, target.width, target.height)
        .to_image();
    let foreground = foreground_color(&source, &repaired);
    (repaired, foreground)
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
        let linear = |value: u8| {
            let channel = f64::from(value) / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        let luminance =
            0.2126 * linear(pixel[0]) + 0.7152 * linear(pixel[1]) + 0.0722 * linear(pixel[2]);
        (sum + luminance, count + 1)
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

fn foreground_color(source: &image::RgbaImage, backdrop: &image::RgbaImage) -> String {
    if source.dimensions() != backdrop.dimensions() || source.is_empty() {
        return contrast_color(backdrop);
    }
    let mut differences = source
        .pixels()
        .zip(backdrop.pixels())
        .map(|(pixel, background)| color_distance(*pixel, *background))
        .collect::<Vec<_>>();
    differences.sort_unstable();
    let threshold = differences[differences.len() * 2 / 3].max(16);
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
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
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
mod tests {
    use super::*;

    #[test]
    fn reconstructed_blob_uses_surrounding_pixels_and_matches_the_detector_region() {
        let background = image::Rgba([72, 96, 120, 255]);
        let glyph = image::Rgba([244, 232, 210, 255]);
        let mut source = image::RgbaImage::from_pixel(80, 40, background);
        for y in 12..28 {
            for x in [28, 29, 30, 43, 44, 45] {
                source.put_pixel(x, y, glyph);
            }
        }
        let target = PixelRegion {
            x: 20,
            y: 10,
            width: 40,
            height: 20,
        };
        let (painted, color) =
            reconstruct_blob_image_with_background(&source, target, &[target], None);
        let url = encode_data_url(&painted).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(color, "#F4E8D2");
        let png = base64::engine::general_purpose::STANDARD
            .decode(url.trim_start_matches("data:image/png;base64,"))
            .unwrap();
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (40, 20));
        assert!(decoded.pixels().all(|pixel| pixel[3] == 255));
        assert!(decoded.pixels().all(|pixel| {
            pixel[0].abs_diff(72) <= 2 && pixel[1].abs_diff(96) <= 2 && pixel[2].abs_diff(120) <= 2
        }));
    }

    #[test]
    fn trusted_uniform_surface_does_not_create_directional_bands() {
        let mut source = image::RgbaImage::from_pixel(100, 40, image::Rgba([250, 250, 250, 255]));
        for y in 8..32 {
            for x in 10..90 {
                source.put_pixel(x, y, image::Rgba([5, 5, 5, 255]));
            }
        }
        let target = PixelRegion {
            x: 10,
            y: 8,
            width: 80,
            height: 24,
        };
        let (painted, _) = reconstruct_blob_image_with_background(
            &source,
            target,
            &[target],
            Some(([250, 250, 250], 90)),
        );
        assert!(
            painted
                .pixels()
                .all(|pixel| pixel.0 == [250, 250, 250, 255])
        );
    }

    #[test]
    fn foreground_sampling_preserves_each_regions_glyph_color() {
        let cases = [
            ([221, 181, 38, 255], [22, 45, 72, 255]),
            ([31, 34, 36, 255], [173, 178, 184, 255]),
            ([240, 240, 240, 255], [194, 48, 116, 255]),
        ];
        for (background, glyph) in cases {
            let backdrop = image::RgbaImage::from_pixel(30, 20, image::Rgba(background));
            let mut source = backdrop.clone();
            for y in 3..17 {
                for x in 8..13 {
                    source.put_pixel(x, y, image::Rgba(glyph));
                }
            }
            assert_eq!(
                foreground_color(&source, &backdrop),
                format!("#{:02X}{:02X}{:02X}", glyph[0], glyph[1], glyph[2])
            );
        }
    }

    #[test]
    fn foreground_sampling_is_stable_over_a_nonuniform_background() {
        let backdrop = image::RgbaImage::from_fn(48, 24, |x, y| {
            image::Rgba([
                30 + x as u8 * 3,
                65 + y as u8 * 2,
                150u8.saturating_sub(x as u8),
                255,
            ])
        });
        let mut source = backdrop.clone();
        let glyph = image::Rgba([238, 74, 36, 255]);
        for y in 4..20 {
            for x in 18..25 {
                source.put_pixel(x, y, glyph);
            }
        }
        assert_eq!(foreground_color(&source, &backdrop), "#EE4A24");
    }

    #[test]
    fn background_inpainting_has_no_horizontal_or_vertical_preference() {
        let image = image::RgbaImage::from_fn(44, 30, |x, y| {
            image::Rgba([
                (20 + x * 3) as u8,
                (30 + y * 5) as u8,
                (40 + x + y * 2) as u8,
                255,
            ])
        });
        let region = PixelRegion {
            x: 9,
            y: 7,
            width: 24,
            height: 14,
        };
        let sample = PixelRegion {
            x: 0,
            y: 0,
            width: image.width(),
            height: image.height(),
        };
        let filled = inpaint_regions(&image, sample, &[region], None);
        let transposed =
            image::RgbaImage::from_fn(image.height(), image.width(), |x, y| *image.get_pixel(y, x));
        let transposed_region = PixelRegion {
            x: region.y,
            y: region.x,
            width: region.height,
            height: region.width,
        };
        let transposed_sample = PixelRegion {
            x: 0,
            y: 0,
            width: transposed.width(),
            height: transposed.height(),
        };
        let transposed_filled =
            inpaint_regions(&transposed, transposed_sample, &[transposed_region], None);
        for y in 0..image.height() {
            for x in 0..image.width() {
                assert_eq!(filled.get_pixel(x, y), transposed_filled.get_pixel(y, x));
            }
        }
    }

    #[test]
    fn background_inpainting_reconstructs_a_smooth_plane_without_diagonal_seams() {
        let image = image::RgbaImage::from_fn(64, 40, |x, y| {
            image::Rgba([
                (20 + x * 2 + y) as u8,
                (30 + x + y * 2) as u8,
                (40 + x + y) as u8,
                255,
            ])
        });
        let region = PixelRegion {
            x: 12,
            y: 9,
            width: 38,
            height: 22,
        };
        let sample = PixelRegion {
            x: 0,
            y: 0,
            width: image.width(),
            height: image.height(),
        };
        let filled = inpaint_regions(&image, sample, &[region], None);
        for y in region.y..region.y + region.height {
            for x in region.x..region.x + region.width {
                let expected = image.get_pixel(x, y);
                let actual = filled.get_pixel(x, y);
                assert!(
                    expected
                        .0
                        .iter()
                        .zip(actual.0)
                        .all(|(expected, actual)| expected.abs_diff(actual) <= 1),
                    "pixel ({x},{y}) expected={expected:?} actual={actual:?}"
                );
            }
        }
    }

    #[test]
    fn background_inpainting_preserves_a_boundary_between_backgrounds() {
        let mut image = image::RgbaImage::from_fn(60, 30, |x, _| {
            if x < 30 {
                image::Rgba([32, 64, 96, 255])
            } else {
                image::Rgba([224, 192, 160, 255])
            }
        });
        let region = PixelRegion {
            x: 12,
            y: 8,
            width: 36,
            height: 14,
        };
        for y in region.y + 2..region.y + region.height - 2 {
            for x in [18, 19, 20] {
                image.put_pixel(x, y, image::Rgba([238, 238, 238, 255]));
            }
            for x in [39, 40, 41] {
                image.put_pixel(x, y, image::Rgba([12, 12, 12, 255]));
            }
        }
        let sample = PixelRegion {
            x: 0,
            y: 0,
            width: image.width(),
            height: image.height(),
        };
        let filled = inpaint_regions(&image, sample, &[region], None);
        let left = filled.get_pixel(19, 15);
        let right = filled.get_pixel(40, 15);
        assert!(left[0] < 100, "left={left:?}");
        assert!(right[0] > 156, "right={right:?}");
    }
}
