const PALETTE_LEN: usize = 1024;
const CORE_STROKE_WIDTH: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SurfaceSpec {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy)]
struct GlowSample {
    alpha: u8,
    white_mix: u8,
    phase: u16,
}

pub(super) struct GlowMask {
    samples: Vec<GlowSample>,
}

impl GlowMask {
    pub fn new(spec: SurfaceSpec, full_width: i32, full_height: i32) -> Self {
        let mut samples = Vec::with_capacity((spec.width * spec.height) as usize);
        let glow_width = glow_width(full_width, full_height);
        let radius = crate::overlay::BOX_CORNER_RADIUS_PHYSICAL_PX
            .min(full_width as f32 / 2.0)
            .min(full_height as f32 / 2.0);
        for local_y in 0..spec.height {
            for local_x in 0..spec.width {
                let x = spec.x + local_x;
                let y = spec.y + local_y;
                let distance = rounded_rect_distance(x, y, full_width, full_height, radius);
                let (alpha, white_mix) = sample_coverage(distance, glow_width);
                samples.push(GlowSample {
                    alpha,
                    white_mix,
                    phase: perimeter_phase(x, y, full_width, full_height),
                });
            }
        }
        Self { samples }
    }

    pub unsafe fn render(&self, pixels: *mut u32, phase_offset: usize) {
        if pixels.is_null() {
            return;
        }
        let output = unsafe { std::slice::from_raw_parts_mut(pixels, self.samples.len()) };
        let palette = blended_palette();
        for (pixel, sample) in output.iter_mut().zip(&self.samples) {
            if sample.alpha == 0 {
                *pixel = 0;
                continue;
            }
            let color = palette[(sample.phase as usize + phase_offset) % PALETTE_LEN];
            *pixel = premultiplied_pixel(color, sample.alpha, sample.white_mix);
        }
    }
}

pub(super) fn surface_specs(width: i32, height: i32) -> Vec<SurfaceSpec> {
    if width <= 0 || height <= 0 {
        return Vec::new();
    }
    let band = glow_band(width, height);
    let top_height = band.min(height);
    let bottom_height = band.min(height - top_height);
    let middle_height = height - top_height - bottom_height;
    let left_width = band.min(width);
    let right_width = band.min(width - left_width);
    let mut specs = Vec::with_capacity(4);
    push_spec(&mut specs, 0, 0, width, top_height);
    push_spec(&mut specs, 0, height - bottom_height, width, bottom_height);
    push_spec(&mut specs, 0, top_height, left_width, middle_height);
    push_spec(
        &mut specs,
        width - right_width,
        top_height,
        right_width,
        middle_height,
    );
    specs
}

pub(super) fn animation_phase(elapsed: std::time::Duration) -> usize {
    ((elapsed.as_secs_f64() / 1.8 * PALETTE_LEN as f64) as usize) % PALETTE_LEN
}

fn push_spec(specs: &mut Vec<SurfaceSpec>, x: i32, y: i32, width: i32, height: i32) {
    if width > 0 && height > 0 {
        specs.push(SurfaceSpec {
            x,
            y,
            width,
            height,
        });
    }
}

fn glow_width(width: i32, height: i32) -> f32 {
    ((width.min(height) as f32) * 0.18).clamp(8.0, 52.0)
}

fn glow_band(width: i32, height: i32) -> i32 {
    ((glow_width(width, height) * 1.55).ceil() as i32 + 2).min(width.min(height))
}

fn rounded_rect_distance(x: i32, y: i32, width: i32, height: i32, radius: f32) -> f32 {
    let half_width = width as f32 / 2.0;
    let half_height = height as f32 / 2.0;
    let px = x as f32 + 0.5 - half_width;
    let py = y as f32 + 0.5 - half_height;
    let qx = px.abs() - half_width + radius;
    let qy = py.abs() - half_height + radius;
    if qx > 0.0 && qy > 0.0 {
        (qx * qx + qy * qy).sqrt() - radius
    } else {
        qx.max(qy) - radius
    }
}

fn sample_coverage(distance: f32, glow_width: f32) -> (u8, u8) {
    if distance > 0.5 || distance < -glow_width {
        return (0, 0);
    }
    if distance > 0.0 {
        let alpha = (1.0 - distance / 0.5).clamp(0.0, 1.0);
        return ((alpha * 255.0).round() as u8, 255);
    }
    let inside = -distance;
    let core_coverage = (CORE_STROKE_WIDTH + 0.5 - inside).clamp(0.0, 1.0);
    let glow_distance = (inside - CORE_STROKE_WIDTH).max(0.0);
    let glow_span = (glow_width - CORE_STROKE_WIDTH).max(1.0);
    let falloff = (1.0 - glow_distance / glow_span).clamp(0.0, 1.0);
    let alpha = core_coverage.max(falloff * falloff * falloff);
    let white_mix = (core_coverage * 255.0).round() as u8;
    ((alpha * 255.0).round() as u8, white_mix)
}

fn perimeter_phase(x: i32, y: i32, width: i32, height: i32) -> u16 {
    let right_distance = width - 1 - x;
    let bottom_distance = height - 1 - y;
    let coordinate = match [y, right_distance, bottom_distance, x]
        .into_iter()
        .enumerate()
        .min_by_key(|(_, distance)| *distance)
        .map(|(edge, _)| edge)
        .unwrap_or(0)
    {
        0 => x,
        1 => width + y,
        2 => width + height + (width - 1 - x),
        _ => width * 2 + height + (height - 1 - y),
    };
    let perimeter = ((width + height) * 2).max(1);
    ((coordinate as i64 * PALETTE_LEN as i64) / perimeter as i64) as u16
}

fn blended_palette() -> &'static [u32; PALETTE_LEN] {
    static PALETTE: std::sync::OnceLock<[u32; PALETTE_LEN]> = std::sync::OnceLock::new();
    PALETTE.get_or_init(|| {
        const STOPS: [u32; 5] = [0x55DCFF, 0x8870FF, 0xFF5CC8, 0xFFB340, 0x55DCFF];
        std::array::from_fn(|index| {
            let scaled = index as f32 / PALETTE_LEN as f32 * (STOPS.len() - 1) as f32;
            let segment = (scaled.floor() as usize).min(STOPS.len() - 2);
            interpolate_rgb(STOPS[segment], STOPS[segment + 1], scaled - segment as f32)
        })
    })
}

fn interpolate_rgb(from: u32, to: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let start = ((from >> shift) & 0xff_u32) as f32;
        let end = ((to >> shift) & 0xff_u32) as f32;
        (start + (end - start) * amount).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn premultiplied_pixel(rgb: u32, alpha: u8, white_mix: u8) -> u32 {
    let mix = white_mix as u32;
    let blend = |channel: u32| (channel * (255 - mix) + 255 * mix + 127) / 255;
    let alpha = alpha as u32;
    let red = blend((rgb >> 16) & 0xff) * alpha / 255;
    let green = blend((rgb >> 8) & 0xff) * alpha / 255;
    let blue = blend(rgb & 0xff) * alpha / 255;
    (alpha << 24) | (red << 16) | (green << 8) | blue
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlap(first: SurfaceSpec, second: SurfaceSpec) -> bool {
        first.x < second.x + second.width
            && second.x < first.x + first.width
            && first.y < second.y + second.height
            && second.y < first.y + first.height
    }

    #[test]
    fn surfaces_are_non_overlapping_for_tiny_and_extreme_boxes() {
        for (width, height) in [(1, 1), (3, 300), (300, 3), (16, 16), (4000, 24), (24, 4000)] {
            let specs = surface_specs(width, height);
            assert!(!specs.is_empty());
            for (index, first) in specs.iter().copied().enumerate() {
                assert!(first.x >= 0 && first.y >= 0);
                assert!(first.x + first.width <= width);
                assert!(first.y + first.height <= height);
                for second in specs.iter().copied().skip(index + 1) {
                    assert!(!overlap(first, second));
                }
            }
        }
    }

    #[test]
    fn large_box_work_scales_with_its_perimeter() {
        let area: i64 = surface_specs(3840, 2160)
            .iter()
            .map(|spec| i64::from(spec.width) * i64::from(spec.height))
            .sum();
        assert!(area < i64::from(3840 * 2160) / 8);
    }

    #[test]
    fn perimeter_phase_is_continuous_and_ratio_independent() {
        let top_right = perimeter_phase(3999, 0, 4000, 24);
        let right_top = perimeter_phase(3999, 1, 4000, 24);
        assert!(top_right.abs_diff(right_top) <= 1);
        assert_ne!(perimeter_phase(2000, 0, 4000, 24), top_right);
        assert_ne!(perimeter_phase(0, 2000, 24, 4000), top_right);
    }

    #[test]
    fn rounded_corner_is_transparent_but_edge_center_is_visible() {
        let spec = SurfaceSpec {
            x: 0,
            y: 0,
            width: 100,
            height: 20,
        };
        let mask = GlowMask::new(spec, 100, 100);
        assert_eq!(mask.samples[0].alpha, 0);
        assert!(mask.samples[50].alpha > 0);
    }

    #[test]
    fn straight_core_matches_the_two_pixel_selection_outline() {
        for inside in [0.5, 1.5] {
            let (alpha, white_mix) = sample_coverage(-inside, 12.0);
            assert_eq!(alpha, 255);
            assert_eq!(white_mix, 255);
        }
        let (_, white_mix) = sample_coverage(-2.5, 12.0);
        assert_eq!(white_mix, 0);
    }
}
