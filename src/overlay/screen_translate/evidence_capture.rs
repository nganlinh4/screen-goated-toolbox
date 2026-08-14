use std::time::{Duration, Instant};

use anyhow::Result;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Dwm::DwmFlush;

const MINIMUM_REVEAL_AGE: Duration = Duration::from_millis(500);
const VISUAL_SAMPLE_INTERVAL: Duration = Duration::from_millis(80);
const VISUAL_STABILITY_TIMEOUT: Duration = Duration::from_secs(2);
const REQUIRED_STABLE_PAIRS: usize = 2;

pub(super) fn capture_stable_selection(
    selection: (i32, i32, u32, u32),
) -> Result<(image::RgbaImage, bool)> {
    std::thread::sleep(MINIMUM_REVEAL_AGE);
    flush_compositor();
    let mut current = capture_selection(selection)?;
    let deadline = Instant::now() + VISUAL_STABILITY_TIMEOUT;
    let mut stable_pairs = 0;
    while Instant::now() < deadline {
        std::thread::sleep(VISUAL_SAMPLE_INTERVAL);
        flush_compositor();
        let next = capture_selection(selection)?;
        if visually_equivalent(&current, &next) {
            stable_pairs += 1;
            if stable_pairs >= REQUIRED_STABLE_PAIRS {
                return Ok((next, true));
            }
        } else {
            stable_pairs = 0;
        }
        current = next;
    }
    Ok((current, false))
}

fn flush_compositor() {
    unsafe {
        let _ = DwmFlush();
    }
}

fn capture_selection(selection: (i32, i32, u32, u32)) -> Result<image::RgbaImage> {
    let (left, top, width, height) = selection;
    let screen = crate::screen_capture::capture_screen_fast()?;
    let right = left.saturating_add(i32::try_from(width).unwrap_or(i32::MAX));
    let bottom = top.saturating_add(i32::try_from(height).unwrap_or(i32::MAX));
    Ok(crate::overlay::selection::extract_crop_from_hbitmap_public(
        &screen,
        RECT {
            left,
            top,
            right,
            bottom,
        },
    ))
}

fn visually_equivalent(left: &image::RgbaImage, right: &image::RgbaImage) -> bool {
    if left.dimensions() != right.dimensions() {
        return false;
    }
    let pixel_count = u64::from(left.width()) * u64::from(left.height());
    let allowed_changes = pixel_count / 1_000 + 1;
    let mut changed = 0_u64;
    for (left, right) in left.pixels().zip(right.pixels()) {
        if left
            .0
            .iter()
            .zip(right.0.iter())
            .take(3)
            .any(|(left, right)| left.abs_diff(*right) > 4)
        {
            changed += 1;
            if changed > allowed_changes {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_stability_ignores_sparse_capture_noise() {
        let left = image::RgbaImage::from_pixel(100, 100, image::Rgba([20, 30, 40, 255]));
        let mut right = left.clone();
        right.put_pixel(50, 50, image::Rgba([200, 30, 40, 255]));
        assert!(visually_equivalent(&left, &right));
    }

    #[test]
    fn visual_stability_rejects_an_unfinished_text_surface() {
        let left = image::RgbaImage::from_pixel(100, 100, image::Rgba([20, 30, 40, 255]));
        let mut right = left.clone();
        for y in 40..50 {
            for x in 20..80 {
                right.put_pixel(x, y, image::Rgba([220, 220, 220, 255]));
            }
        }
        assert!(!visually_equivalent(&left, &right));
    }
}
