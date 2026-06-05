// --- PIXEL-ART SPLASH ---
// The splash renderer: the scene is drawn into a low-res CPU framebuffer (egui
// can't post-process its own output) and blitted upscaled with nearest-neighbor
// for chunky, uniform pixels.
//
// This module is the orchestrator (`paint_pixel`): it builds the per-frame
// `Frame`, runs the layer passes (back to front), applies the fade-in / exit
// dither, uploads the framebuffer to a texture, and draws the crisp UI overlay.
// The pieces live in submodules:
//   - `layers`  — the scene passes (sky, stars, rays, moon/sun, clouds, grid,
//                 the 3D "SGT" voxels, vignette, theme button)
//   - `ui`      — the wordmark / progress / loading / click text + pixel fonts
//   - `raster`  — filled discs / glows / blending + the `Rgb` colour math
//
// Scene physics (voxel assembly/drift) live in render.rs; entity data + colours
// in scene.rs / palette.rs. Logical screen units map to framebuffer pixels via
// `s = fb_h / size.y`.

mod layers;
mod raster;
mod ui;

use super::palette::Palette;
use super::scene::{Cloud, MoonFeature, Voxel};
use super::{DrawListEntry, EXIT_DURATION};
use crate::{WINDOW_HEIGHT, WINDOW_WIDTH};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use raster::{bayer4, lerp3, rgb, to_col};
use std::cell::RefCell;

/// Vertical resolution of the framebuffer — the one knob for chunkiness. All
/// text uses this same pixel size, so it stays unified with the scene.
const FB_H: usize = 230;

type Rgb = (f32, f32, f32);

/// A projected voxel ready to rasterize: (depth, fx, fy, radius, colour, alpha,
/// is_white, is_debris).
type VoxelDraw = (f32, f32, f32, f32, Rgb, f32, bool, bool);

pub struct PixelPaintContext<'a> {
    pub ctx: &'a egui::Context,
    pub start_time: f64,
    pub exit_start_time: Option<f64>,
    pub palette: Palette,
    pub voxels: &'a [Voxel],
    pub moon_features: &'a [MoonFeature],
    pub clouds: &'a [Cloud],
    pub mouse_influence: Vec2,
    pub loading_text: &'a str,
    pub draw_list: &'a RefCell<Vec<DrawListEntry>>,
    pub tex: &'a RefCell<Option<egui::TextureHandle>>,
}

/// Per-frame state shared by all the layer passes (logical->FB mapping etc.).
struct Frame {
    w: usize,
    h: usize,
    s: f32,        // logical screen px -> framebuffer px
    cx: f32,       // framebuffer centre x
    cy: f32,       // framebuffer centre y
    t: f32,        // seconds since start
    warp: f32,     // 0..1 exit progress
    is_dark: bool, // = palette.is_night (moon vs sun, fade base, stars vs rays)
    pal: Palette,
    mouse: Vec2,
}

impl Frame {
    /// Map a logical offset-from-centre into framebuffer pixels.
    fn at(&self, off: Vec2) -> (f32, f32) {
        (self.cx + off.x * self.s, self.cy + off.y * self.s)
    }
    fn len(&self, logical: f32) -> f32 {
        logical * self.s
    }
}

pub fn paint_pixel(c: PixelPaintContext<'_>) -> bool {
    let now = c.ctx.input(|i| i.time);
    let t = (now - c.start_time) as f32;
    let warp = match c.exit_start_time {
        Some(es) => ((now - es) as f32 / EXIT_DURATION).clamp(0.0, 1.0),
        None => 0.0,
    };

    let vp = c.ctx.input(|i| {
        i.viewport()
            .inner_rect
            .unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::ZERO))
    });
    let size = if vp.width() < 100.0 || vp.height() < 100.0 {
        Vec2::new(WINDOW_WIDTH, WINDOW_HEIGHT)
    } else {
        vp.size()
    };
    let rect = Rect::from_min_size(Pos2::ZERO, size);

    // Block interaction behind the splash + keep window dragging working.
    egui::Area::new(egui::Id::new("splash_blocker"))
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::ZERO)
        .show(c.ctx, |ui| {
            let resp = ui.allocate_response(
                size,
                egui::Sense::click_and_drag().union(egui::Sense::hover()),
            );
            if resp.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
        });

    let h = FB_H;
    let w = ((h as f32) * size.x / size.y).round().clamp(16.0, 1024.0) as usize;
    let f = Frame {
        w,
        h,
        s: h as f32 / size.y,
        cx: w as f32 * 0.5,
        cy: h as f32 * 0.5,
        t,
        warp,
        is_dark: c.palette.is_night,
        pal: c.palette,
        mouse: c.mouse_influence,
    };
    let mut buf = vec![Color32::BLACK; w * h];

    // Clear the voxel draw-list; draw_voxels repopulates it (logical screen
    // coords) so the escape overlay can fling voxels onto the desktop on exit.
    c.draw_list.borrow_mut().clear();

    // Layers, back to front. UI text/progress are NOT baked in — they render as a
    // crisp painter overlay below (the FB pixels are too coarse for small text).
    layers::paint_background(&mut buf, &f);
    layers::paint_stars(&mut buf, &f);
    layers::paint_god_rays(&mut buf, &f);
    layers::paint_celestial(&mut buf, &f, c.moon_features);
    layers::paint_clouds(&mut buf, &f, c.clouds);
    layers::paint_grid(&mut buf, &f);
    layers::draw_voxels(&mut buf, &f, c.voxels, c.draw_list);
    layers::paint_vignette(&mut buf, &f);
    layers::paint_theme_button(&mut buf, &f);

    // Fade-in: keep the splash OPAQUE from the first visible frame (it must fully
    // cover the already-prepared UI behind it) by blending the whole scene up from
    // a solid base — black in dark theme, white in light — over the first 0.4s.
    // The UI is only ever exposed by the exit dither, so the reveal shows a ready
    // UI rather than the splash fading in see-through over it.
    if t < 0.4 {
        let k = 1.0 - (t / 0.4).clamp(0.0, 1.0);
        let base: Rgb = if f.is_dark {
            (0.0, 0.0, 0.0)
        } else {
            (255.0, 255.0, 255.0)
        };
        for px in buf.iter_mut() {
            *px = to_col(lerp3(rgb(*px), base, k));
        }
    }

    // Exit dissolve = HARD pixel dither, applied IN PLACE (no extra buffer/pass):
    // dithered pixels become fully transparent (revealing the app), the rest keep
    // their opaque scene colour. A hard dither (never semi-transparent) avoids the
    // gray wash that partial-alpha coloured pixels would blend over the UI. `buf`
    // is then moved straight into the texture (it's already `Vec<Color32>`).
    //
    // The dissolve runs FASTER than the full exit so the UI reveal snaps; the
    // escaped voxels keep flying over the revealed UI for the rest of the exit.
    let dissolve = (warp * 2.2).clamp(0.0, 1.0);
    if dissolve > 0.0 {
        for y in 0..h {
            for x in 0..w {
                if bayer4(x, y) < dissolve {
                    buf[y * w + x] = Color32::TRANSPARENT;
                }
            }
        }
    }
    let image = egui::ColorImage {
        size: [w, h],
        source_size: egui::vec2(w as f32, h as f32),
        pixels: buf,
    };
    let opts = egui::TextureOptions::NEAREST;
    let handle = {
        let mut slot = c.tex.borrow_mut();
        match slot.as_mut() {
            Some(h) => {
                h.set(image, opts);
                h.clone()
            }
            None => {
                let h = c.ctx.load_texture("splash_pixel_fb", image, opts);
                let clone = h.clone();
                *slot = Some(h);
                clone
            }
        }
    };

    let painter = c.ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("splash_overlay"),
    ));
    // The splash is OPAQUE (the fade-in is a colour fade baked into the FB above,
    // not a transparency fade) so it always fully covers the prepared UI.
    let fade_in = (t / 0.4).clamp(0.0, 1.0); // kept only for the theme-button gate
    painter.image(
        handle.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );

    // UI text/progress are crisp overlays (the FB is too coarse for small text).
    ui::paint_ui_overlay(&painter, size, t, warp, c.palette, c.loading_text);

    // The pixel theme button is drawn into the FB above; catch its clicks here
    // (top-left, matching the vector switcher's position/size).
    let mut theme_clicked = false;
    if fade_in > 0.1 {
        egui::Area::new(egui::Id::new("splash_pixel_theme"))
            .order(egui::Order::Tooltip)
            .fixed_pos(Pos2::new(16.0, 14.0))
            .show(c.ctx, |ui| {
                if ui
                    .allocate_response(Vec2::splat(32.0), egui::Sense::click())
                    .clicked()
                {
                    theme_clicked = true;
                }
            });
    }

    theme_clicked
}
