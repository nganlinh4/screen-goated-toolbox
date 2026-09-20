//! Shared resize-handle appearance for native region editors.
use windows::Win32::Foundation::POINT;

pub(crate) fn paint(center: POINT, scale: f32, mut blend: impl FnMut(i32, i32, u32, f32)) {
    let half = (5.0 * scale).round().max(3.0) as i32 - (2.0 * scale).round() as i32;
    let radius = 2.0 * scale;
    for y in -half..half {
        for x in -half..half {
            let dx = ((x as f32 + 0.5).abs() - half as f32 + radius).max(0.0);
            let dy = ((y as f32 + 0.5).abs() - half as f32 + radius).max(0.0);
            let coverage = (radius + 0.5 - dx.hypot(dy)).clamp(0.0, 1.0);
            blend(center.x + x, center.y + y, 0xffe8f6f7, coverage);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_symmetric_antialiased_and_fit_inside_grab_target() {
        for scale in [1.0, 1.25, 1.5, 2.0, 4.0] {
            let center = POINT { x: -120, y: 80 };
            let mut samples = std::collections::BTreeMap::new();
            paint(center, scale, |x, y, color, alpha| {
                assert_eq!(color, 0xffe8f6f7);
                assert!((0.0..=1.0).contains(&alpha));
                assert!((x - center.x).abs() < (5.0 * scale).round() as i32);
                assert!((y - center.y).abs() < (5.0 * scale).round() as i32);
                samples.insert((x - center.x, y - center.y), alpha);
            });
            assert!(samples.values().any(|a| *a > 0.0 && *a < 1.0));
            assert_eq!(samples[&(0, 0)], 1.0);
            for (&(x, y), &alpha) in &samples {
                assert_eq!(samples[&(-x - 1, -y - 1)], alpha);
            }
        }
    }
}
