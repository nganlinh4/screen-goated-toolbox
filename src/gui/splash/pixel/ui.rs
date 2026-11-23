// LAYER 6: UI overlay — the wordmark, progress bar, loading line and click
// prompt, drawn with the egui painter on top of the upscaled scene. Each font
// cell and bar segment is one scene cell, placed on the scene's own grid.

use super::super::ANIMATION_DURATION;
use super::super::palette::Palette;
use super::font::{self, TextBitmap};
use super::{PixelGrid, font_raster};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Progress bar size, in cells.
const BAR_CELLS: f32 = 52.0;
const BAR_ROWS: f32 = 2.0;

pub(super) struct OverlayText<'a> {
    pub loading: &'a str,
    pub click_prompt: &'a str,
}

pub(super) fn paint_ui_overlay(
    painter: &egui::Painter,
    size: Vec2,
    grid: PixelGrid,
    t: f32,
    warp: f32,
    pal: Palette,
    text: OverlayText<'_>,
) {
    if warp >= 0.1 {
        return;
    }
    let cx = size.x * 0.5;
    let cy = size.y * 0.5;
    let cell = grid.cell;

    // Wordmark.
    let title = format!("SCREEN GOATED TOOLBOX V{}", env!("CARGO_PKG_VERSION"));
    text_painter(
        painter,
        &title,
        cx,
        cy + 150.0,
        grid,
        pal.text_primary,
        false,
    );

    // Progress bar: whole cells, square corners, filling one cell at a time.
    let bw = BAR_CELLS * cell;
    let bh = BAR_ROWS * cell;
    let by = grid.snap(cy + 178.0); // just below the wordmark
    let bx = grid.snap(cx - bw * 0.5);
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
        Rect::from_min_size(
            Pos2::new(bx, by),
            Vec2::new((BAR_CELLS * prog).floor() * cell, bh),
        ),
        0.0,
        fillc,
    );

    // Loading line — below the progress bar (no overlap).
    let load_col = pal.text_accent;
    text_painter(
        painter,
        &text.loading.to_uppercase(),
        cx,
        by + bh + cell,
        grid,
        load_col,
        true,
    );

    // Click prompt — at the top.
    if t > ANIMATION_DURATION - 0.5 {
        let pulse = (t * 5.0).sin().abs() * 0.7 + 0.3;
        let cc = pal.text_accent;
        text_painter(
            painter,
            text.click_prompt,
            cx,
            cy - 205.0,
            grid,
            cc.linear_multiply(pulse),
            true,
        );
    }
}

/// Draw a centered string in screen space, one grid cell per font pixel.
/// `tiny` asks for the compact 3x5 face (same cell size, smaller glyphs) so the
/// small lines shrink without leaving the grid.
fn text_painter(
    painter: &egui::Painter,
    s: &str,
    cx: f32,
    top: f32,
    grid: PixelGrid,
    col: Color32,
    tiny: bool,
) {
    let Some(line) = line_bitmap(s, tiny) else {
        return;
    };
    let px = grid.cell;
    // Snap the origin to the scene grid so the font cells line up with its pixels.
    let left = grid.snap(cx - line.width as f32 * px * 0.5);
    let top = grid.snap(top);
    // One rect per horizontal run of ink keeps the shape count low.
    for y in 0..line.height {
        let mut x = 0;
        while x < line.width {
            if !line.ink(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < line.width && line.ink(x, y) {
                x += 1;
            }
            let min = Pos2::new(left + start as f32 * px, top + y as f32 * px);
            let size = Vec2::new((x - start) as f32 * px, px);
            painter.rect_filled(Rect::from_min_size(min, size), 0.0, col);
        }
    }
}

/// The laid-out line: the authored pixel font where it covers the text (Latin
/// and Vietnamese), else a bilevel render of the system font. Cached, since the
/// splash repaints the same few lines every frame.
fn line_bitmap(s: &str, tiny: bool) -> Option<Rc<TextBitmap>> {
    /// Laid-out lines keyed by (text, tiny); `None` caches a line nothing can draw.
    type LineCache = HashMap<(String, bool), Option<Rc<TextBitmap>>>;
    thread_local! {
        static CACHE: RefCell<LineCache> = RefCell::new(HashMap::new());
    }
    CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry((s.to_owned(), tiny))
            .or_insert_with(|| {
                font::layout(s, tiny)
                    .or_else(|| font_raster::rasterize(s))
                    .map(Rc::new)
            })
            .clone()
    })
}
