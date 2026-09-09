use anyhow::{Result, bail};
use image::{RgbImage, imageops::FilterType};

pub(super) fn preprocess(crop: &RgbImage) -> Result<(usize, Vec<f32>)> {
    if crop.width() == 0 || crop.height() == 0 {
        bail!("empty script crop");
    }
    let width = (48.0 * f64::from(crop.width()) / f64::from(crop.height()))
        .round_ties_even()
        .max(1.0) as usize;
    if width * 48 > 40_000_000 {
        bail!("script crop exceeds pixel budget");
    }
    let resized = image::imageops::resize(crop, width as u32, 48, FilterType::CatmullRom);
    let mut histogram = [0_u32; 256];
    let mut gray = resized
        .pixels()
        .map(|pixel| {
            let value =
                ((3 * u16::from(pixel[0]) + 5 * u16::from(pixel[1]) + 2 * u16::from(pixel[2]) + 5)
                    / 10) as u8;
            histogram[value as usize] += 1;
            value
        })
        .collect::<Vec<_>>();
    if dark_background(&gray, width, &histogram) {
        gray.iter_mut().for_each(|value| *value = 255 - *value);
    }
    let mut minima = [0_u32; 256];
    let mut maxima = [0_u32; 256];
    for triple in gray[24 * width..25 * width].windows(3) {
        let [a, b, c] = [triple[0], triple[1], triple[2]];
        if b <= a && b <= c && (b < a || b < c) {
            minima[b as usize] += 1;
        }
        if b >= a && b >= c && (b > a || b > c) {
            maxima[b as usize] += 1;
        }
    }
    if minima.iter().all(|&n| n == 0) {
        minima[0] = 1;
    }
    if maxima.iter().all(|&n| n == 0) {
        maxima[255] = 1;
    }
    let black = percentile(&minima, 0.25);
    let white = percentile(&maxima, 0.75);
    let contrast = if white > black {
        (white - black) / 2.0
    } else {
        1.0
    };
    Ok((
        width,
        gray.into_iter()
            .map(|v| (f32::from(v) - black) / contrast - 1.0)
            .collect(),
    ))
}

fn percentile(histogram: &[u32; 256], fraction: f32) -> f32 {
    let total = histogram.iter().sum::<u32>() as f32;
    let target = (total * fraction).max(1.0).min(total);
    let mut sum = 0;
    for (index, &count) in histogram.iter().enumerate() {
        sum += count;
        if sum as f32 >= target && count > 0 {
            return index as f32 + 1.0 - (sum as f32 - target) / count as f32;
        }
    }
    0.0
}

fn dark_background(gray: &[u8], width: usize, histogram: &[u32; 256]) -> bool {
    let total = gray.len() as f64;
    let sum = histogram
        .iter()
        .enumerate()
        .map(|(i, &n)| i as f64 * f64::from(n))
        .sum::<f64>();
    let mut count = 0.0;
    let mut weighted = 0.0;
    let mut best = 0.0;
    let mut threshold = 0;
    for (value, &n) in histogram.iter().enumerate() {
        count += f64::from(n);
        weighted += value as f64 * f64::from(n);
        if count == 0.0 || count == total {
            continue;
        }
        let difference = weighted / count - (sum - weighted) / (total - count);
        let separation = count * (total - count) * difference * difference;
        if separation > best {
            best = separation;
            threshold = value as u8;
        }
    }
    if best == 0.0 {
        return false;
    }
    // Text-box margins identify the background even when bold strokes occupy
    // most of the interior. Ambiguous margins fall back to area occupancy.
    let height = gray.len() / width;
    let mut border_low = 0_usize;
    let mut border_count = 0_usize;
    let border = gray[..width]
        .iter()
        .chain(gray[(height - 1) * width..].iter())
        .copied()
        .chain(
            gray.chunks_exact(width)
                .skip(1)
                .take(height.saturating_sub(2))
                .flat_map(|row| [row[0], row[width - 1]]),
        );
    for value in border {
        border_low += usize::from(value <= threshold);
        border_count += 1;
    }
    if border_low * 4 >= border_count * 3 {
        return true;
    }
    if (border_count - border_low) * 4 >= border_count * 3 {
        return false;
    }
    histogram[..=usize::from(threshold)].iter().sum::<u32>() as usize > gray.len() / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gradient_background_keeps_strokes_dark_under_both_polarities() {
        for reverse in [false, true] {
            let source = RgbImage::from_fn(96, 48, |x, y| {
                let value = if x % 12 < 3 && y > 4 && y < 43 {
                    230
                } else {
                    70 + x as u8 / 3
                };
                image::Rgb([if reverse { 255 - value } else { value }; 3])
            });
            let (_, input) = preprocess(&source).unwrap();
            assert!(input[24 * 96 + 24] < input[24 * 96 + 28]);
        }
    }
    #[test]
    fn polarity_tracks_background_not_absolute_brightness() {
        for background in [10_u8, 70, 120, 180, 240] {
            for foreground in [0_u8, 40, 100, 160, 220, 255] {
                if foreground == background {
                    continue;
                }
                for dense in [false, true] {
                    let source = RgbImage::from_fn(48, 48, |x, y| {
                        let ink = x > 2 && x < 45 && y > 2 && y < 45 && (dense || x % 8 < 2);
                        image::Rgb([if ink { foreground } else { background }; 3])
                    });
                    let (_, input) = preprocess(&source).unwrap();
                    assert!(
                        input[24 * 48 + 8] < input[0],
                        "bg={background} fg={foreground} dense={dense}"
                    );
                    let reversed = RgbImage::from_fn(48, 48, |x, y| {
                        image::Rgb([255 - source.get_pixel(x, y)[0]; 3])
                    });
                    assert_eq!(input, preprocess(&reversed).unwrap().1);
                }
            }
        }
    }
    #[test]
    fn flat_and_narrow_inputs_remain_finite() {
        for value in [0, 63, 64, 128, 255] {
            for width in [1, 2, 320] {
                let image = RgbImage::from_pixel(width, 48, image::Rgb([value; 3]));
                let (w, pixels) = preprocess(&image).unwrap();
                assert_eq!(pixels.len(), 48 * w);
                assert!(pixels.iter().all(|p| p.is_finite()));
            }
        }
    }
}
