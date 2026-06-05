// LAYER 6: UI overlay — the wordmark, progress bar, loading line and click
// prompt. The framebuffer is too coarse for small text, so these are drawn with
// the egui painter at controllable sizes (still the pixel font, just crisp
// `rect_filled` blocks in screen space) on top of the upscaled scene.

use super::super::ANIMATION_DURATION;
use super::super::palette::Palette;
use super::FB_H;
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

pub(super) fn paint_ui_overlay(
    painter: &egui::Painter,
    size: Vec2,
    t: f32,
    warp: f32,
    pal: Palette,
    loading: &str,
) {
    if warp >= 0.1 {
        return;
    }
    let cx = size.x * 0.5;
    let cy = size.y * 0.5;
    // One pixel size for ALL splash text = the scene's pixel size, so the font
    // is unified with the rest of the pixel art (one font cell = one scene pixel).
    let px = size.y / FB_H as f32;

    // Wordmark.
    let title = format!("SCREEN GOATED TOOLBOX V{}", env!("CARGO_PKG_VERSION"));
    text_painter(painter, &title, cx, cy + 150.0, px, pal.text_primary, false);

    // Progress bar — pixel-grid-aligned, square corners (no smooth pill).
    let bw = (52.0 * px).round();
    let bh = (2.0 * px).round();
    let by = (cy + 178.0).round(); // just below the wordmark
    let bx = (cx - bw * 0.5).round();
    let track = if pal.is_night {
        Color32::from_white_alpha(30)
    } else {
        Color32::from_black_alpha(30)
    };
    painter.rect_filled(
        Rect::from_min_size(Pos2::new(bx, by), Vec2::new(bw, bh)),
        0.0,
        track,
    );
    let prog = (t.min(ANIMATION_DURATION) / (ANIMATION_DURATION - 0.5)).clamp(0.0, 1.0);
    let fillc = pal.accent_primary;
    painter.rect_filled(
        Rect::from_min_size(Pos2::new(bx, by), Vec2::new((bw * prog).round(), bh)),
        0.0,
        fillc,
    );

    // Loading line — below the progress bar (no overlap).
    let load_col = pal.text_accent;
    text_painter(
        painter,
        &loading.to_uppercase(),
        cx,
        by + bh + 4.0,
        px,
        load_col,
        true,
    );

    // Click prompt — at the top.
    if t > ANIMATION_DURATION - 0.5 {
        let pulse = (t * 5.0).sin().abs() * 0.7 + 0.3;
        let cc = pal.text_accent;
        text_painter(
            painter,
            "CLICK ANYWHERE TO CONTINUE",
            cx,
            cy - 205.0,
            px,
            cc.linear_multiply(pulse),
            true,
        );
    }
}

/// Draw a centered string in screen space, one `px`-sized block per font pixel.
/// `tiny` swaps the 5x7 font for a compact 3x5 one — SAME pixel size, smaller
/// glyphs — so the small lines (loading / click) shrink without breaking unity.
fn text_painter(
    painter: &egui::Painter,
    s: &str,
    cx: f32,
    top: f32,
    px: f32,
    col: Color32,
    tiny: bool,
) {
    let gw = if tiny { 3usize } else { 5usize };
    let adv = (gw + 1) as f32 * px; // glyph + 1-cell gap
    let total = s.chars().count() as f32 * adv - px;
    // Snap the origin to the px grid so the font cells line up with scene pixels.
    let mut x = ((cx - total * 0.5) / px).round() * px;
    let top = (top / px).round() * px;
    for ch in s.chars() {
        let g7;
        let g5;
        let rows: &[u8] = if tiny {
            g5 = glyph3(ch);
            &g5
        } else {
            g7 = glyph(ch);
            &g7
        };
        for (ry, &row) in rows.iter().enumerate() {
            for rx in 0..gw {
                if (row >> (gw - 1 - rx)) & 1 == 1 {
                    let p = Pos2::new(x + rx as f32 * px, top + ry as f32 * px);
                    painter.rect_filled(Rect::from_min_size(p, Vec2::splat(px)), 0.0, col);
                }
            }
        }
        x += adv;
    }
}

// --- 5x7 PIXEL FONT ---------------------------------------------------------

#[rustfmt::skip]
fn glyph(ch: char) -> [u8; 7] {
    const FONT: [[u8; 7]; 38] = [
        [14,17,17,31,17,17,17], // A
        [30,17,17,30,17,17,30], // B
        [14,17,16,16,16,17,14], // C
        [28,18,17,17,17,18,28], // D
        [31,16,16,30,16,16,31], // E
        [31,16,16,30,16,16,16], // F
        [14,17,16,23,17,17,15], // G
        [17,17,17,31,17,17,17], // H
        [14, 4, 4, 4, 4, 4,14], // I
        [ 7, 2, 2, 2, 2,18,12], // J
        [17,18,20,24,20,18,17], // K
        [16,16,16,16,16,16,31], // L
        [17,27,21,21,17,17,17], // M
        [17,17,25,21,19,17,17], // N
        [14,17,17,17,17,17,14], // O
        [30,17,17,30,16,16,16], // P
        [14,17,17,17,21,18,13], // Q
        [30,17,17,30,20,18,17], // R
        [15,16,16,14, 1, 1,30], // S
        [31, 4, 4, 4, 4, 4, 4], // T
        [17,17,17,17,17,17,14], // U
        [17,17,17,17,17,10, 4], // V
        [17,17,17,21,21,21,10], // W
        [17,17,10, 4,10,17,17], // X
        [17,17,10, 4, 4, 4, 4], // Y
        [31, 1, 2, 4, 8,16,31], // Z
        [14,17,19,21,25,17,14], // 0
        [ 4,12, 4, 4, 4, 4,14], // 1
        [14,17, 1, 6, 8,16,31], // 2
        [31, 2, 4, 2, 1,17,14], // 3
        [ 2, 6,10,18,31, 2, 2], // 4
        [31,16,30, 1, 1,17,14], // 5
        [ 6, 8,16,30,17,17,14], // 6
        [31, 1, 2, 4, 8, 8, 8], // 7
        [14,17,17,14,17,17,14], // 8
        [14,17,17,15, 1, 2,12], // 9
        [ 0, 0, 0, 0, 0, 0, 0], // space (36)
        [ 0, 0, 0, 0, 0,12,12], // .     (37)
    ];
    let c = ch.to_ascii_uppercase();
    let idx = match c {
        'A'..='Z' => (c as u8 - b'A') as usize,
        '0'..='9' => 26 + (c as u8 - b'0') as usize,
        '.' => 37,
        _ => 36,
    };
    FONT[idx]
}

/// Compact 3x5 font (bit 2 = leftmost) for the small lines — same pixel size as
/// the 5x7 font, just fewer cells per glyph, so the text is physically smaller.
#[rustfmt::skip]
fn glyph3(ch: char) -> [u8; 5] {
    const FONT: [[u8; 5]; 38] = [
        [2,5,7,5,5], // A
        [6,5,6,5,6], // B
        [3,4,4,4,3], // C
        [6,5,5,5,6], // D
        [7,4,6,4,7], // E
        [7,4,6,4,4], // F
        [3,4,5,5,3], // G
        [5,5,7,5,5], // H
        [7,2,2,2,7], // I
        [1,1,1,5,2], // J
        [5,6,4,6,5], // K
        [4,4,4,4,7], // L
        [5,7,7,5,5], // M
        [5,7,5,5,5], // N
        [2,5,5,5,2], // O
        [6,5,6,4,4], // P
        [2,5,5,3,1], // Q
        [6,5,6,5,5], // R
        [3,4,2,1,6], // S
        [7,2,2,2,2], // T
        [5,5,5,5,7], // U
        [5,5,5,5,2], // V
        [5,5,7,7,5], // W
        [5,5,2,5,5], // X
        [5,5,2,2,2], // Y
        [7,1,2,4,7], // Z
        [7,5,5,5,7], // 0
        [2,6,2,2,7], // 1
        [6,1,2,4,7], // 2
        [6,1,2,1,6], // 3
        [5,5,7,1,1], // 4
        [7,4,6,1,6], // 5
        [3,4,6,5,3], // 6
        [7,1,2,2,2], // 7
        [2,5,2,5,2], // 8
        [2,5,3,1,6], // 9
        [0,0,0,0,0], // space (36)
        [0,0,0,0,2], // .     (37)
    ];
    let c = ch.to_ascii_uppercase();
    let idx = match c {
        'A'..='Z' => (c as u8 - b'A') as usize,
        '0'..='9' => 26 + (c as u8 - b'0') as usize,
        '.' => 37,
        _ => 36,
    };
    FONT[idx]
}
