use anyhow::{Context, Result};
use base64::Engine as _;
use image::ImageEncoder as _;
use image::codecs::png::PngEncoder;

use super::geometry::{PixelRegion, background_sample_region};

pub(super) fn reconstruct_blob(
    image: &image::RgbaImage,
    target: PixelRegion,
    masks: &[PixelRegion],
) -> Result<(String, String)> {
    let sample = background_sample_region(target, image.width(), image.height());
    let context = image::imageops::crop_imm(image, sample.x, sample.y, sample.width, sample.height)
        .to_image();
    let reconstructed = inpaint_regions(&context, sample, masks);
    let sigma = (target.height as f32 * 0.08).clamp(0.8, 3.5);
    let reconstructed = image::imageops::blur(&reconstructed, sigma);
    let painted = image::imageops::crop_imm(
        &reconstructed,
        target.x - sample.x,
        target.y - sample.y,
        target.width,
        target.height,
    )
    .to_image();
    let source = image::imageops::crop_imm(image, target.x, target.y, target.width, target.height)
        .to_image();
    let foreground = foreground_color(&source, &painted);
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            painted.as_raw(),
            painted.width(),
            painted.height(),
            image::ExtendedColorType::Rgba8,
        )
        .context("reconstructed region encoding failed")?;
    Ok((
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ),
        foreground,
    ))
}

fn inpaint_regions(
    context: &image::RgbaImage,
    sample: PixelRegion,
    regions: &[PixelRegion],
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
        if right <= left || bottom <= top {
            continue;
        }
        for y in (top - sample.y)..(bottom - sample.y) {
            for x in (left - sample.x)..(right - sample.x) {
                let index = (y * width + x) as usize;
                mask[index] = true;
            }
        }
    }
    let mut totals = [0u64; 4];
    let mut known = 0u64;
    for (index, pixel) in context.pixels().enumerate() {
        if mask[index] {
            continue;
        }
        for (total, channel) in totals.iter_mut().zip(pixel.0) {
            *total += u64::from(channel);
        }
        known += 1;
    }
    if known == 0 || !mask.iter().any(|masked| *masked) {
        return context.clone();
    }
    let mut current = context
        .pixels()
        .map(|pixel| pixel.0.map(f64::from))
        .collect::<Vec<_>>();
    let mut filled = mask.iter().map(|masked| !masked).collect::<Vec<_>>();
    let mut queued = vec![false; mask.len()];
    let mut frontier = Vec::new();
    for index in 0..mask.len() {
        if mask[index]
            && surrounding_neighbors(index, width, height).any(|neighbor| filled[neighbor])
        {
            queued[index] = true;
            frontier.push(index);
        }
    }
    while !frontier.is_empty() {
        let updates = frontier
            .iter()
            .map(|&index| {
                let mut totals = [0.0f64; 4];
                let mut count = 0.0f64;
                for neighbor in surrounding_neighbors(index, width, height) {
                    if filled[neighbor] {
                        for (total, channel) in totals.iter_mut().zip(current[neighbor]) {
                            *total += channel;
                        }
                        count += 1.0;
                    }
                }
                (index, totals.map(|total| total / count))
            })
            .collect::<Vec<_>>();
        for &(index, pixel) in &updates {
            current[index] = pixel;
            filled[index] = true;
        }
        let mut next = Vec::new();
        for (index, _) in updates {
            for neighbor in surrounding_neighbors(index, width, height) {
                if mask[neighbor] && !filled[neighbor] && !queued[neighbor] {
                    queued[neighbor] = true;
                    next.push(neighbor);
                }
            }
        }
        frontier = next;
    }
    let mut result = context.clone();
    for (index, pixel) in result.pixels_mut().enumerate() {
        if mask[index] {
            *pixel =
                image::Rgba(current[index].map(|channel| channel.round().clamp(0.0, 255.0) as u8));
        }
    }
    result
}

fn surrounding_neighbors(index: usize, width: u32, height: u32) -> impl Iterator<Item = usize> {
    let x = index as u32 % width;
    let y = index as u32 / width;
    [
        (x.checked_sub(1), y.checked_sub(1)),
        (Some(x), y.checked_sub(1)),
        (x.checked_add(1), y.checked_sub(1)),
        (x.checked_sub(1), Some(y)),
        (x.checked_add(1), Some(y)),
        (x.checked_sub(1), y.checked_add(1)),
        (Some(x), y.checked_add(1)),
        (x.checked_add(1), y.checked_add(1)),
    ]
    .into_iter()
    .filter_map(move |(neighbor_x, neighbor_y)| {
        let neighbor_x = neighbor_x.filter(|neighbor| *neighbor < width)?;
        let neighbor_y = neighbor_y.filter(|neighbor| *neighbor < height)?;
        Some((neighbor_y * width + neighbor_x) as usize)
    })
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
        let source = image::RgbaImage::from_fn(80, 40, |x, y| {
            if (20..60).contains(&x) && (10..30).contains(&y) {
                if (x / 2) % 2 == 0 {
                    image::Rgba([0, 0, 0, 255])
                } else {
                    image::Rgba([255, 255, 255, 255])
                }
            } else {
                image::Rgba([72, 96, 120, 255])
            }
        });
        let target = PixelRegion {
            x: 20,
            y: 10,
            width: 40,
            height: 20,
        };
        let (url, color) = reconstruct_blob(&source, target, &[target]).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(matches!(color.as_str(), "#111111" | "#FFFFFF"));
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
        let filled = inpaint_regions(&image, sample, &[region]);
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
            inpaint_regions(&transposed, transposed_sample, &[transposed_region]);
        for y in 0..image.height() {
            for x in 0..image.width() {
                assert_eq!(filled.get_pixel(x, y), transposed_filled.get_pixel(y, x));
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
        for y in region.y..region.y + region.height {
            for x in region.x..region.x + region.width {
                image.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
            }
        }
        let sample = PixelRegion {
            x: 0,
            y: 0,
            width: image.width(),
            height: image.height(),
        };
        let filled = inpaint_regions(&image, sample, &[region]);
        let left = filled.get_pixel(18, 15);
        let right = filled.get_pixel(42, 15);
        assert!(left[0] < 100, "left={left:?}");
        assert!(right[0] > 156, "right={right:?}");
    }
}
