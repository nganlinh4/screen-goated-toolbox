// Raster + small-math helpers for the pixel-art splash: filled discs / glows
// that write into the framebuffer, alpha blending, and the `Rgb` colour math
// used by every layer.

use super::{Frame, Rgb};
use eframe::egui::Color32;

/// Additive radial glow (core -> falloff) for the moon/sun halo.
pub(super) fn glow(
    buf: &mut [Color32],
    f: &Frame,
    c: (f32, f32),
    radius: f32,
    col: Rgb,
    intensity: f32,
) {
    if intensity <= 0.001 || radius < 1.0 {
        return;
    }
    let (cx, cy) = c;
    let x0 = (cx - radius).floor().max(0.0) as usize;
    let x1 = ((cx + radius).ceil().max(0.0) as usize).min(f.w);
    let y0 = (cy - radius).floor().max(0.0) as usize;
    let y1 = ((cy + radius).ceil().max(0.0) as usize).min(f.h);
    for yy in y0..y1 {
        for xx in x0..x1 {
            let d = (((xx as f32 + 0.5 - cx).powi(2)) + ((yy as f32 + 0.5 - cy).powi(2))).sqrt();
            if d < radius {
                let g = (1.0 - d / radius).powi(2) * intensity;
                let idx = yy * f.w + xx;
                buf[idx] = to_col(add3(rgb(buf[idx]), col, g));
            }
        }
    }
}

/// Alpha-blended filled disc (used for the moon/sun + features).
pub(super) fn disc(buf: &mut [Color32], f: &Frame, c: (f32, f32), r: f32, col: Rgb, a: f32) {
    fill_disc(buf, f, c, r, col, a);
}

pub(super) fn fill_disc(buf: &mut [Color32], f: &Frame, c: (f32, f32), r: f32, col: Rgb, a: f32) {
    let (cx, cy) = c;
    if r < 0.8 {
        put(buf, f.w, f.h, cx, cy, col, a);
        return;
    }
    let r2 = r * r;
    let x0 = (cx - r).floor().max(0.0) as usize;
    let x1 = ((cx + r).ceil().max(0.0) as usize).min(f.w);
    let y0 = (cy - r).floor().max(0.0) as usize;
    let y1 = ((cy + r).ceil().max(0.0) as usize).min(f.h);
    for yy in y0..y1 {
        for xx in x0..x1 {
            let dx = xx as f32 + 0.5 - cx;
            let dy = yy as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                blend(buf, f.w, xx, yy, col, a);
            }
        }
    }
}

/// Like [`fill_disc`] but never draws below `max_y` (the cloud horizon gate).
pub(super) fn fill_disc_clip(
    buf: &mut [Color32],
    f: &Frame,
    c: (f32, f32),
    r: f32,
    col: Rgb,
    a: f32,
    max_y: f32,
) {
    let (cx, cy) = c;
    let r2 = r * r;
    let x0 = (cx - r).floor().max(0.0) as usize;
    let x1 = ((cx + r).ceil().max(0.0) as usize).min(f.w);
    let y0 = (cy - r).floor().max(0.0) as usize;
    let y1 = ((cy + r).ceil().max(0.0) as usize)
        .min(f.h)
        .min(max_y.max(0.0) as usize);
    for yy in y0..y1 {
        for xx in x0..x1 {
            let dx = xx as f32 + 0.5 - cx;
            let dy = yy as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                blend(buf, f.w, xx, yy, col, a);
            }
        }
    }
}

pub(super) fn put(buf: &mut [Color32], w: usize, h: usize, x: f32, y: f32, col: Rgb, a: f32) {
    if x < 0.0 || y < 0.0 {
        return;
    }
    let (xi, yi) = (x as usize, y as usize);
    if xi < w && yi < h {
        blend(buf, w, xi, yi, col, a);
    }
}

pub(super) fn blend(buf: &mut [Color32], w: usize, x: usize, y: usize, col: Rgb, a: f32) {
    let idx = y * w + x;
    buf[idx] = to_col(lerp3(rgb(buf[idx]), col, a.clamp(0.0, 1.0)));
}

// --- SMALL MATH -------------------------------------------------------------

pub(super) fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn lerp3(a: Rgb, b: Rgb, t: f32) -> Rgb {
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}

pub(super) fn add3(a: Rgb, b: Rgb, amt: f32) -> Rgb {
    (a.0 + b.0 * amt, a.1 + b.1 * amt, a.2 + b.2 * amt)
}

pub(super) fn mul3(a: Rgb, m: f32) -> Rgb {
    (a.0 * m, a.1 * m, a.2 * m)
}

pub(super) fn rgb<C: Into<RgbSrc>>(c: C) -> Rgb {
    c.into().0
}

/// Adapter so `rgb()` accepts both `Color32` and raw `(f32,f32,f32)` literals.
pub(super) struct RgbSrc(Rgb);
impl From<Color32> for RgbSrc {
    fn from(c: Color32) -> Self {
        RgbSrc((c.r() as f32, c.g() as f32, c.b() as f32))
    }
}
impl From<Rgb> for RgbSrc {
    fn from(c: Rgb) -> Self {
        RgbSrc(c)
    }
}

pub(super) fn to_col(c: Rgb) -> Color32 {
    Color32::from_rgb(
        c.0.clamp(0.0, 255.0) as u8,
        c.1.clamp(0.0, 255.0) as u8,
        c.2.clamp(0.0, 255.0) as u8,
    )
}

pub(super) fn bayer4(x: usize, y: usize) -> f32 {
    const M: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    (M[y % 4][x % 4] as f32 + 0.5) / 16.0
}

pub(super) fn hash2(x: u32, y: u32) -> f32 {
    let mut h = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263));
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^= h >> 16;
    (h & 0x00ff_ffff) as f32 / 0x00ff_ffff as f32
}
