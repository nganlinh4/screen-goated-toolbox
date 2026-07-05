//! Set-of-Mark grounding: overlay a labeled coordinate grid on the view we send
//! to the model, so it picks a NUMBERED CELL instead of regressing raw pixels
//! (which the token-starved Live video channel localizes poorly). The model
//! references a label; we map label -> exact view fraction -> screen pixel.
//!
//! Everything here is uniform in view space, so the same `cols x rows` partition
//! that we DRAW on the downscaled image also defines the click/zoom geometry —
//! no per-pixel bookkeeping. Drawing is dependency-free (a hand-rolled 5x7 font).

use image::{Rgb, RgbImage};

/// A uniform `cols x rows` label grid. Cells are numbered 1.. in reading order
/// (left-to-right, top-to-bottom): label = row*cols + col + 1.
#[derive(Clone, Copy, Debug)]
pub(super) struct Grid {
    pub cols: u32,
    pub rows: u32,
}

impl Grid {
    /// Grid size, overridable via `CC_GRID_COLS` / `CC_GRID_ROWS` for tuning.
    pub fn from_env() -> Self {
        let cols = env_dim("CC_GRID_COLS", 6);
        let rows = env_dim("CC_GRID_ROWS", 5);
        Grid { cols, rows }
    }

    pub fn cell_count(&self) -> u32 {
        self.cols * self.rows
    }

    fn rc(&self, label: u32) -> Option<(u32, u32)> {
        if label == 0 || label > self.cell_count() {
            return None;
        }
        let idx = label - 1;
        Some((idx / self.cols, idx % self.cols))
    }

    /// The label of the cell containing a 0-1000 view-space point.
    pub fn cell_at(&self, mx: f64, my: f64) -> u32 {
        let c =
            ((mx / 1000.0 * self.cols as f64).floor() as i64).clamp(0, self.cols as i64 - 1) as u32;
        let r =
            ((my / 1000.0 * self.rows as f64).floor() as i64).clamp(0, self.rows as i64 - 1) as u32;
        r * self.cols + c + 1
    }

    /// Center of a labeled cell in 0-1000 view space (x, y).
    pub fn center_norm(&self, label: u32) -> Option<(f64, f64)> {
        let (r, c) = self.rc(label)?;
        let mx = (c as f64 + 0.5) / self.cols as f64 * 1000.0;
        let my = (r as f64 + 0.5) / self.rows as f64 * 1000.0;
        Some((mx, my))
    }

    /// Labeled cell rect as 0..1 view fractions (x0, y0, x1, y1), expanded by
    /// `pad` cells on each side (clamped) so a zoom keeps a little context.
    pub fn frac_rect(&self, label: u32, pad: f64) -> Option<(f64, f64, f64, f64)> {
        let (r, c) = self.rc(label)?;
        let cw = 1.0 / self.cols as f64;
        let ch = 1.0 / self.rows as f64;
        let x0 = ((c as f64 - pad) * cw).clamp(0.0, 1.0);
        let x1 = ((c as f64 + 1.0 + pad) * cw).clamp(0.0, 1.0);
        let y0 = ((r as f64 - pad) * ch).clamp(0.0, 1.0);
        let y1 = ((r as f64 + 1.0 + pad) * ch).clamp(0.0, 1.0);
        Some((x0, y0, x1, y1))
    }

    /// Draw the grid lines + cell labels onto the (final, downscaled) image.
    pub fn draw(&self, img: &mut RgbImage) {
        let (w, h) = (img.width(), img.height());
        if w < 16 || h < 16 {
            return;
        }
        let cw = w as f32 / self.cols as f32;
        let ch = h as f32 / self.rows as f32;
        // Faint lines so they don't obscure the content the model must READ
        // (the visual token budget is fixed; clarity of the underlying pixels is
        // what matters). Labels in the corners carry the addressing.
        let line = Rgb([255, 0, 255]); // magenta
        for c in 1..self.cols {
            vline(img, (c as f32 * cw) as u32, line, 0.22);
        }
        for r in 1..self.rows {
            hline(img, (r as f32 * ch) as u32, line, 0.22);
        }
        let scale = (cw.min(ch) / 26.0).clamp(2.0, 7.0) as i32;
        let inset = scale.max(3);
        for r in 0..self.rows {
            for c in 0..self.cols {
                let label = r * self.cols + c + 1;
                // Anchor the label in the cell's TOP-LEFT corner so the cell
                // center (where the click lands) stays unobscured.
                let x = (c as f32 * cw) as i32 + inset;
                let y = (r as f32 * ch) as i32 + inset;
                draw_label(img, x, y, label, scale);
            }
        }
    }
}

/// Draw a debug marker (red ring + crosshair) at frame pixel (cx, cy) showing
/// exactly where a click landed, for the click-accuracy trace.
pub(super) fn draw_click_marker(img: &mut RgbImage, cx: i32, cy: i32) {
    let red = Rgb([255, 40, 40]);
    let arm = 16;
    for d in -arm..=arm {
        // crosshair, 2px thick
        blend(img, cx + d, cy, red, 0.95);
        blend(img, cx + d, cy + 1, red, 0.75);
        blend(img, cx, cy + d, red, 0.95);
        blend(img, cx + 1, cy + d, red, 0.75);
    }
    // ring at radius ~11
    let r = 11.0;
    for k in 0..96 {
        let a = k as f32 / 96.0 * std::f32::consts::TAU;
        let x = cx + (r * a.cos()).round() as i32;
        let y = cy + (r * a.sin()).round() as i32;
        blend(img, x, y, red, 0.95);
    }
}

fn env_dim(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&d| (2..=12).contains(&d))
        .unwrap_or(default)
}

// --- drawing primitives (alpha-blended onto RgbImage) ---

fn blend(img: &mut RgbImage, x: i32, y: i32, color: Rgb<u8>, a: f32) {
    if x < 0 || y < 0 || x as u32 >= img.width() || y as u32 >= img.height() {
        return;
    }
    let p = img.get_pixel_mut(x as u32, y as u32);
    for i in 0..3 {
        p.0[i] = (p.0[i] as f32 * (1.0 - a) + color.0[i] as f32 * a).round() as u8;
    }
}

fn vline(img: &mut RgbImage, x: u32, color: Rgb<u8>, a: f32) {
    for y in 0..img.height() {
        blend(img, x as i32, y as i32, color, a);
    }
}

fn hline(img: &mut RgbImage, y: u32, color: Rgb<u8>, a: f32) {
    for x in 0..img.width() {
        blend(img, x as i32, y as i32, color, a);
    }
}

fn fill_rect(img: &mut RgbImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb<u8>, a: f32) {
    for y in y0..y1 {
        for x in x0..x1 {
            blend(img, x, y, color, a);
        }
    }
}

/// Draw a label number with its TOP-LEFT corner near (x, y): a dark plate behind
/// bright digits, so the cell center stays clear for the click target.
fn draw_label(img: &mut RgbImage, x: i32, y: i32, label: u32, scale: i32) {
    let digits: Vec<u8> = label.to_string().bytes().map(|b| b - b'0').collect();
    let glyph_w = 5 * scale;
    let glyph_h = 7 * scale;
    let gap = scale;
    let total_w = digits.len() as i32 * glyph_w + (digits.len() as i32 - 1) * gap;
    let pad = scale;
    fill_rect(
        img,
        x,
        y,
        x + total_w + 2 * pad,
        y + glyph_h + 2 * pad,
        Rgb([0, 0, 0]),
        0.72,
    );
    let mut dx = x + pad;
    let bright = Rgb([255, 235, 0]); // yellow
    for d in digits {
        draw_digit(img, dx, y + pad, d, scale, bright);
        dx += glyph_w + gap;
    }
}

fn draw_digit(img: &mut RgbImage, x: i32, y: i32, d: u8, scale: i32, color: Rgb<u8>) {
    let rows = &FONT_5X7[d as usize];
    for (ry, bits) in rows.iter().enumerate() {
        for rx in 0..5 {
            if bits & (1 << (4 - rx)) != 0 {
                let px = x + rx * scale;
                let py = y + ry as i32 * scale;
                fill_rect(img, px, py, px + scale, py + scale, color, 1.0);
            }
        }
    }
}

/// 5x7 bitmap font, digits 0-9. Each row is 5 bits (bit4 = leftmost column).
const FONT_5X7: [[u8; 7]; 10] = [
    [
        0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
    ], // 0
    [
        0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ], // 1
    [
        0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
    ], // 2
    [
        0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
    ], // 3
    [
        0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
    ], // 4
    [
        0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
    ], // 5
    [
        0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
    ], // 6
    [
        0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
    ], // 7
    [
        0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
    ], // 8
    [
        0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
    ], // 9
];
