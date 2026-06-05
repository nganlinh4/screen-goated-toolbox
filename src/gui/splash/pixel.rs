// --- PIXEL-ART SPLASH ---
// The splash renderer: the scene is drawn into a low-res CPU framebuffer (egui
// can't post-process its own output) and blitted upscaled with nearest-neighbor
// for chunky, uniform pixels.
//
// Layers (in order): sky + haze, stars, big centered moon/sun with glow +
// features, clouds, synthwave grid, the 3D "SGT" voxel assembly (sphere-shaded),
// vignette, and the UI text (wordmark, loading line, progress pill, click prompt)
// in a 5x7 / 3x5 pixel font. Scene physics (voxel assembly/drift) live in
// render.rs; entity data + colours in scene.rs / mod.rs. Logical screen units map
// to framebuffer pixels via `s = fb_h / size.y`.

use crate::{WINDOW_HEIGHT, WINDOW_WIDTH};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use std::cell::RefCell;

use super::math::Vec3;
use super::scene::{Cloud, MoonFeature, Voxel};
use super::{
    ANIMATION_DURATION, C_CLOUD_CORE, C_CLOUD_WHITE, C_CYAN, C_DAY_REP, C_DAY_SEC, C_DAY_TEXT,
    C_MAGENTA, C_MOON_BASE, C_MOON_GLOW, C_MOON_HIGHLIGHT, C_MOON_SHADOW, C_SUN_BODY, C_SUN_FLARE,
    C_SUN_GLOW, C_VOID, C_WHITE, DrawListEntry, EXIT_DURATION,
};

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
    pub is_dark: bool,
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
    s: f32,    // logical screen px -> framebuffer px
    cx: f32,   // framebuffer centre x
    cy: f32,   // framebuffer centre y
    t: f32,    // seconds since start
    warp: f32, // 0..1 exit progress
    is_dark: bool,
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
        is_dark: c.is_dark,
        mouse: c.mouse_influence,
    };
    let mut buf = vec![Color32::BLACK; w * h];

    // Clear the voxel draw-list; draw_voxels repopulates it (logical screen
    // coords) so the escape overlay can fling voxels onto the desktop on exit.
    c.draw_list.borrow_mut().clear();

    // Layer order matches the vector renderer. UI text/progress are NOT baked in
    // — they render as a crisp overlay below (the FB pixels are too coarse).
    paint_background(&mut buf, &f);
    paint_stars(&mut buf, &f);
    paint_god_rays(&mut buf, &f);
    paint_celestial(&mut buf, &f, c.moon_features);
    paint_clouds(&mut buf, &f, c.clouds);
    paint_grid(&mut buf, &f);
    draw_voxels(&mut buf, &f, c.voxels, c.draw_list);
    paint_vignette(&mut buf, &f);
    paint_theme_button(&mut buf, &f);

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
    if warp > 0.0 {
        for y in 0..h {
            for x in 0..w {
                if bayer4(x, y) < warp {
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
    paint_ui_overlay(&painter, size, t, warp, c.is_dark, c.loading_text);

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

// --- LAYER 0: BACKGROUND (sky + haze) --------------------------------------

fn paint_background(buf: &mut [Color32], f: &Frame) {
    // The sky keeps its full colour on exit — the dither dissolve is the only exit
    // effect, so the scene must NOT darken (the old `1 - warp*4` blackened it).
    let base: Rgb = if f.is_dark {
        rgb(C_VOID)
    } else {
        rgb((100.0, 180.0, 255.0)) // C_SKY_DAY_TOP
    };
    // Toward the horizon: faint deep-blue lift (night) / pale (day).
    let haze: Rgb = if f.is_dark {
        (34.0, 30.0, 64.0)
    } else {
        (226.0, 239.0, 255.0)
    };
    let haze_a = if f.is_dark { 0.16 } else { 0.6 };

    for y in 0..f.h {
        let vy = y as f32 / f.h as f32;
        let row = lerp3(base, haze, smooth(0.0, 1.0, vy) * haze_a);
        for x in 0..f.w {
            buf[y * f.w + x] = to_col(row);
        }
    }
}

// --- LAYER 1: STARS (placed, night) ----------------------------------------

fn paint_stars(buf: &mut [Color32], f: &Frame) {
    if !f.is_dark {
        return;
    }
    // Deterministic placed starfield with per-star twinkle + slight parallax.
    let par_x = -f.mouse.x * 6.0;
    let par_y = -f.mouse.y * 6.0;
    let n = 150;
    for i in 0..n {
        let hx = hash2(i as u32, 11);
        let hy = hash2(i as u32, 29);
        let phase = hash2(i as u32, 71) * std::f32::consts::TAU;
        let bright = 0.3 + hash2(i as u32, 53) * 0.7;
        let big = hash2(i as u32, 97) > 0.93;
        let sx = hx * f.w as f32 + par_x;
        let sy = hy * 0.7 * f.h as f32 + par_y;
        let tw = bright * (0.55 + 0.45 * (f.t * 2.0 + phase).sin());
        let col = (235.0, 240.0, 255.0);
        put(buf, f.w, f.h, sx, sy, col, tw);
        if big {
            put(buf, f.w, f.h, sx + 1.0, sy, col, tw * 0.6);
            put(buf, f.w, f.h, sx, sy + 1.0, col, tw * 0.6);
        }
    }
}

// --- LAYER 1.5: GOD RAYS (day-mode crepuscular rays from the sun) -----------

fn paint_god_rays(buf: &mut [Color32], f: &Frame) {
    if f.is_dark || f.warp > 0.9 {
        return;
    }
    let (sx, sy) = f.at(Vec2::new(0.0, -40.0 * (1.0 - f.warp)));
    let rot = f.t * 0.1;
    let rays = 12.0_f32;
    let max_d = f.len(1200.0); // ray_len in the vector path
    let x0 = (sx - max_d).max(0.0) as usize;
    let x1 = ((sx + max_d).max(0.0) as usize).min(f.w);
    let y0 = (sy - max_d).max(0.0) as usize;
    let y1 = ((sy + max_d).max(0.0) as usize).min(f.h);
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 - sx;
            let dy = y as f32 - sy;
            let d = (dx * dx + dy * dy).sqrt();
            if d < 1.0 || d > max_d {
                continue;
            }
            // Each ray is a SOLID wedge spanning the first half of its 1/12
            // segment (i/12 .. (i+0.5)/12) — hard angular edges, no angular
            // gradient. Only a linear RADIAL fade: white-alpha at the sun ->
            // transparent at the tip (matches the vector mesh triangles).
            let ang = dy.atan2(dx) - rot;
            let phase = (ang / std::f32::consts::TAU * rays).rem_euclid(1.0);
            if phase < 0.5 {
                // Bayer-DITHERED radial fade (pixel art, not a smooth gradient):
                // solid near the sun, dithering out to nothing at the tip.
                let density = 1.0 - d / max_d;
                if density > bayer4(x, y) {
                    let idx = y * f.w + x;
                    buf[idx] = to_col(lerp3(rgb(buf[idx]), (255.0, 255.0, 240.0), 0.4));
                }
            }
        }
    }
}

// --- LAYER 2: CELESTIAL BODY (big centered moon / sun) ---------------------

fn paint_celestial(buf: &mut [Color32], f: &Frame, features: &[MoonFeature]) {
    let alpha = (1.0 - f.warp * 3.0).clamp(0.0, 1.0);
    if alpha <= 0.01 {
        return;
    }
    let bob = (f.t * 0.5).sin() * 5.0;
    let parallax = f.mouse * -30.0;
    let center_off = Vec2::new(parallax.x, -40.0 + bob + parallax.y);
    let (cx, cy) = f.at(center_off);
    let r = f.len(140.0);

    if f.is_dark {
        glow(buf, f, (cx, cy), r * 2.3, rgb(C_MOON_GLOW), 0.18 * alpha);
        disc(buf, f, (cx, cy), r, rgb(C_MOON_BASE), alpha);
        // shadow lower-right, highlight upper-left (give it volume).
        disc(
            buf,
            f,
            (cx + f.len(10.0), cy + f.len(10.0)),
            r * 0.9,
            (0.0, 0.0, 0.0),
            0.2 * alpha,
        );
        disc(
            buf,
            f,
            (cx - f.len(10.0), cy - f.len(10.0)),
            r * 0.85,
            (255.0, 255.0, 255.0),
            0.08 * alpha,
        );
        moon_features(buf, f, (cx, cy), r, features, alpha, f.t * 0.05);
    } else {
        glow(buf, f, (cx, cy), r * 3.0, rgb(C_SUN_GLOW), 0.30 * alpha);
        disc(buf, f, (cx, cy), r, rgb(C_SUN_BODY), alpha);
        sun_features(buf, f, (cx, cy), r, features, alpha, f.t * 0.08);
    }
}

fn moon_features(
    buf: &mut [Color32],
    f: &Frame,
    c: (f32, f32),
    r: f32,
    feats: &[MoonFeature],
    alpha: f32,
    rot: f32,
) {
    let (rc, rs) = (rot.cos(), rot.sin());
    for feat in feats {
        let rx = feat.pos.x * rc - feat.pos.y * rs;
        let ry = feat.pos.x * rs + feat.pos.y * rc;
        let dist_sq = rx * rx + ry * ry;
        if dist_sq > 0.95 {
            continue;
        }
        let z = (1.0 - dist_sq).sqrt();
        let fx = c.0 + rx * r;
        let fy = c.1 + ry * r;
        let fr = feat.radius * r * (0.5 + 0.5 * z);
        let fa = alpha * z;
        if feat.is_crater {
            disc(
                buf,
                f,
                (fx - 1.0, fy - 1.0),
                fr,
                rgb(C_MOON_SHADOW),
                fa * 0.8,
            );
            disc(
                buf,
                f,
                (fx + 1.0, fy + 1.0),
                fr * 0.9,
                rgb(C_MOON_HIGHLIGHT),
                fa * 0.4,
            );
        } else {
            disc(buf, f, (fx, fy), fr, rgb(C_MOON_SHADOW), fa * 0.3);
        }
    }
}

fn sun_features(
    buf: &mut [Color32],
    f: &Frame,
    c: (f32, f32),
    r: f32,
    feats: &[MoonFeature],
    alpha: f32,
    rot: f32,
) {
    let (rc, rs) = (rot.cos(), rot.sin());
    for feat in feats {
        let rx = feat.pos.x * rc - feat.pos.y * rs;
        let ry = feat.pos.x * rs + feat.pos.y * rc;
        let dist_sq = rx * rx + ry * ry;
        if dist_sq > 0.95 {
            continue;
        }
        let z = (1.0 - dist_sq).sqrt();
        let fx = c.0 + rx * r;
        let fy = c.1 + ry * r;
        let fr = feat.radius * r * (0.5 + 0.5 * z);
        let fa = alpha * z;
        if feat.is_crater {
            disc(buf, f, (fx, fy), fr * 0.6, (160.0, 60.0, 0.0), fa * 0.8);
        } else {
            disc(buf, f, (fx, fy), fr * 1.5, rgb(C_SUN_FLARE), fa * 0.3);
            disc(buf, f, (fx, fy), fr * 0.8, rgb(C_WHITE), fa * 0.5);
        }
    }
}

// --- LAYER 3: CLOUDS (dithered drifting blobs) -----------------------------

fn paint_clouds(buf: &mut [Color32], f: &Frame, clouds: &[Cloud]) {
    // The real volumetric clouds: each cloud is a cluster of puffs (soft outer
    // fluff + denser core), mirroring the vector path. Dark in night, white in day.
    let core: Rgb = if f.is_dark {
        rgb(C_CLOUD_CORE)
    } else {
        rgb(C_CLOUD_WHITE)
    };
    let parallax = f.mouse * -15.0;
    // Gate: clouds never render below the horizon, so they don't sit in the grid
    // ("water"). Clipped at the pixel level (a puff straddling it gets cut, not
    // dropped). Matches the vector path's `with_clip_rect` at horizon + 30.
    let clip_y = f.cy + f.len(120.0 + 30.0);
    for (i, cloud) in clouds.iter().enumerate() {
        let c_x = f.cx + (cloud.pos.x + parallax.x) * f.s;
        let c_y = f.cy + (cloud.pos.y + parallax.y) * f.s;
        let local_fade = if f.warp > 0.0 {
            let rnd = (i as f32 * 0.73).fract();
            1.0 - ((f.warp - rnd * 0.6) / 0.3).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let alpha = cloud.opacity * local_fade;
        if alpha <= 0.01 {
            continue;
        }
        for (offset, r_mult) in &cloud.puffs {
            let p_x = c_x + offset.x * cloud.scale * f.s + f.len(2.0);
            let p_y = c_y + offset.y * cloud.scale * f.s + f.len(5.0);
            let radius = 30.0 * cloud.scale * r_mult * f.s;
            fill_disc_clip(
                buf,
                f,
                (p_x, p_y),
                radius * 1.35,
                core,
                alpha * 0.45,
                clip_y,
            );
            fill_disc_clip(buf, f, (p_x, p_y), radius, core, alpha, clip_y);
        }
    }
}

// --- LAYER 4: RETRO GRID (synthwave floor) ---------------------------------

fn paint_grid(buf: &mut [Color32], f: &Frame) {
    let horizon = f.cy + f.len(120.0);
    let bottom = f.h as f32;
    let cam_y = 150.0 + f.t.min(ANIMATION_DURATION + 5.0) * 30.0;
    let col: Rgb = if f.is_dark {
        rgb(C_MAGENTA)
    } else {
        rgb(C_DAY_REP)
    };

    for i in 0..16 {
        let rnd = (i as f32 * 0.9).sin() * 0.5 + 0.5;
        let local_fade = if f.warp > 0.0 {
            1.0 - ((f.warp - rnd * 0.5) / 0.25).clamp(0.0, 1.0)
        } else {
            1.0
        };
        if local_fade <= 0.0 {
            continue;
        }
        let z = 1.0 + (i as f32 * 0.5) - ((cam_y * 0.05) % 0.5);
        let persp = 250.0 / (z - f.warp * 0.8).max(0.1);
        let y = horizon + f.len(persp * 0.6);
        if y > bottom || y < horizon {
            continue;
        }
        let half = f.len(WINDOW_WIDTH) * (2.5 / z) * 0.5; // width ~ rect.width()*(2.5/z)
        let a = (1.0 - (y - horizon) / (bottom - horizon))
            .max(0.0)
            .powf(0.5)
            * 0.5
            * local_fade;
        let x0 = (f.cx - half).max(0.0);
        let x1 = (f.cx + half).min(f.w as f32);
        let yy = y as usize;
        if yy < f.h {
            let mut xx = x0 as usize;
            let xe = x1 as usize;
            while xx < xe {
                let idx = yy * f.w + xx;
                buf[idx] = to_col(lerp3(rgb(buf[idx]), col, a));
                xx += 1;
            }
        }
    }
}

// --- LAYER 5: VOXELS (the 3D "SGT" assembly as shaded spheres) --------------

fn draw_voxels(buf: &mut [Color32], f: &Frame, voxels: &[Voxel], dl: &RefCell<Vec<DrawListEntry>>) {
    let physics_t = f.t.min(ANIMATION_DURATION);
    let fov = 800.0_f32;
    let cam_dist = 600.0 + smooth(0.0, 8.0, physics_t) * 100.0;
    let grot = Vec3::new(f.mouse.y * 0.2, f.mouse.x * 0.2, 0.0);
    let light = Vec2::new(-0.4, -0.4); // 2D light dir (matches vector path)

    let mut list: Vec<VoxelDraw> = Vec::with_capacity(voxels.len());
    for v in voxels {
        let mut a = 1.0_f32;
        if v.is_debris {
            let fs = 4.0 + v.noise_factor * 3.0;
            a = 1.0 - smooth(fs, fs + 2.5, physics_t);
            if a <= 0.02 {
                continue;
            }
            a *= 0.4 + v.noise_factor * 0.6;
            a *= (f.t * (3.0 + v.noise_factor * 2.0) + v.noise_factor * 50.0).sin() * 0.25 + 0.75;
        }
        if f.warp > 0.0 {
            let ll = ((f.warp - v.noise_factor * 0.75) / 0.25).clamp(0.0, 1.0);
            a *= 1.0 - (ll * 1.5).clamp(0.0, 1.0);
        }
        if a <= 0.02 {
            continue;
        }
        let vc = v.pos.rotate_x(grot.x).rotate_y(grot.y).rotate_z(grot.z);
        let z = cam_dist - vc.z;
        if z < 0.1 {
            continue;
        }
        let sc = fov / z;
        let fx = f.cx + vc.x * sc * f.s;
        let fy = f.cy - vc.y * sc * f.s;
        let r = (8.5 * v.scale * sc * f.s).max(0.6);
        let is_white = v.color == C_WHITE || v.color == C_DAY_SEC;
        // Theme-aware colour remap (mirrors the vector path): the scene may have
        // been initialised under the other theme, so convert day<->night brand
        // colours, and make day-mode debris white.
        let mut base = v.color;
        if !v.is_debris && v.color != C_WHITE {
            if f.is_dark {
                if v.color == C_DAY_REP {
                    base = C_MAGENTA;
                } else if v.color == C_DAY_SEC {
                    base = C_CYAN;
                }
            } else if v.color == C_MAGENTA {
                base = C_DAY_REP;
            } else if v.color == C_CYAN {
                base = C_DAY_SEC;
            }
        }
        if !f.is_dark && v.is_debris {
            base = C_CLOUD_WHITE;
        }
        let vcol = rgb(base);
        list.push((z, fx, fy, r, vcol, a, is_white, v.is_debris));
        // Record the LOGICAL screen position/size so the escape overlay can fling
        // voxels that drift past the window edge onto the desktop (the "trick").
        dl.borrow_mut().push((
            z,
            Pos2::new(fx / f.s, fy / f.s),
            r / f.s,
            Color32::from_rgba_unmultiplied(
                vcol.0 as u8,
                vcol.1 as u8,
                vcol.2 as u8,
                (a * 255.0).clamp(0.0, 255.0) as u8,
            ),
            is_white,
            v.is_debris,
        ));
    }
    list.sort_by(|p, q| q.0.partial_cmp(&p.0).unwrap_or(std::cmp::Ordering::Equal));

    for (_, fx, fy, r, col, a, is_white, is_debris) in list {
        if is_debris {
            // Debris stays flat — a single solid disc, no sphere shading.
            fill_disc(buf, f, (fx, fy), r, col, a);
            continue;
        }
        // Sphere shading for the SGT text voxels: shadow disc, offset lit body,
        // inner glow, specular highlight.
        let shadow = if f.is_dark {
            (0.0, 0.0, 0.0)
        } else if is_white {
            (100.0, 120.0, 150.0)
        } else {
            (0.0, 40.0, 100.0)
        };
        fill_disc(buf, f, (fx, fy), r, shadow, a * 0.8);
        let body_off = (light.x * r * 0.15, light.y * r * 0.15);
        fill_disc(buf, f, (fx + body_off.0, fy + body_off.1), r * 0.85, col, a);
        let glow_off = (light.x * r * 0.3, light.y * r * 0.3);
        let glowc = if is_white { (255.0, 255.0, 255.0) } else { col };
        fill_disc(
            buf,
            f,
            (fx + glow_off.0, fy + glow_off.1),
            r * 0.5,
            glowc,
            a * 0.5,
        );
        if r > 1.6 {
            let hp = (fx + light.x * r * 0.5, fy + light.y * r * 0.5);
            fill_disc(buf, f, hp, r * 0.25, (255.0, 255.0, 255.0), a * 0.9);
        }
    }
}

// --- VIGNETTE ---------------------------------------------------------------

fn paint_vignette(buf: &mut [Color32], f: &Frame) {
    let fw = f.w as f32;
    let fh = f.h as f32;
    // Fade the vignette out on exit so the dark corners don't dither as dark dots.
    let strength = 0.45 * (1.0 - f.warp);
    for y in 0..f.h {
        for x in 0..f.w {
            let vx = (x as f32 / fw - 0.5) * 2.0;
            let vy = (y as f32 / fh - 0.5) * 2.0;
            // Squared distance (no sqrt) — same look for a vignette, cheaper.
            let vd2 = vx * vx + vy * vy;
            let vig = 1.0 - smooth(0.49, 2.25, vd2) * strength;
            if vig < 1.0 {
                let idx = y * f.w + x;
                buf[idx] = to_col(mul3(rgb(buf[idx]), vig));
            }
        }
    }
}

// --- THEME BUTTON (pixel sun/moon, top-left, drawn into the FB) -------------

fn paint_theme_button(buf: &mut [Color32], f: &Frame) {
    let s = f.s;
    let cx = 32.0 * s;
    let cy = 30.0 * s;
    let r = 13.0 * s;
    // Subtle chip backing.
    fill_disc(buf, f, (cx, cy), 17.0 * s, (8.0, 8.0, 14.0), 0.35);
    if f.is_dark {
        // Sun (click -> switch to light): disc + 8 rays.
        fill_disc(buf, f, (cx, cy), r, (255.0, 210.0, 80.0), 0.95);
        for k in 0..8 {
            let a = k as f32 / 8.0 * std::f32::consts::TAU;
            put(
                buf,
                f.w,
                f.h,
                cx + a.cos() * r * 1.7,
                cy + a.sin() * r * 1.7,
                (255.0, 220.0, 110.0),
                0.9,
            );
        }
    } else {
        // Moon (click -> switch to dark): a TRUE crescent — paint only the lit
        // pixels (moon disc minus an offset disc), so there's no black overlay.
        let mcx = cx + r * 0.55;
        let mcy = cy - r * 0.2;
        let mr = r * 0.95;
        let x0 = (cx - r).floor().max(0.0) as usize;
        let x1 = ((cx + r).ceil().max(0.0) as usize).min(f.w);
        let y0 = (cy - r).floor().max(0.0) as usize;
        let y1 = ((cy + r).ceil().max(0.0) as usize).min(f.h);
        for yy in y0..y1 {
            for xx in x0..x1 {
                let pxc = xx as f32 + 0.5;
                let pyc = yy as f32 + 0.5;
                let in_moon = (pxc - cx).powi(2) + (pyc - cy).powi(2) <= r * r;
                let in_carve = (pxc - mcx).powi(2) + (pyc - mcy).powi(2) <= mr * mr;
                if in_moon && !in_carve {
                    blend(buf, f.w, xx, yy, (235.0, 240.0, 255.0), 0.95);
                }
            }
        }
    }
}

// --- LAYER 6: UI overlay (crisp, painted on top of the upscaled scene) -------
// The framebuffer is too coarse for small text, so the wordmark / loading line /
// progress / click prompt are drawn with the painter at controllable sizes
// (still the 5x7 pixel font, just as crisp `rect_filled` blocks in screen space).

fn paint_ui_overlay(
    painter: &egui::Painter,
    size: Vec2,
    t: f32,
    warp: f32,
    is_dark: bool,
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
    let title_col = if is_dark { C_WHITE } else { C_DAY_TEXT };
    text_painter(painter, &title, cx, cy + 150.0, px, title_col, false);

    // Progress bar — pixel-grid-aligned, square corners (no smooth pill).
    let bw = (52.0 * px).round();
    let bh = (2.0 * px).round();
    let by = (cy + 178.0).round(); // just below the wordmark
    let bx = (cx - bw * 0.5).round();
    let track = if is_dark {
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
    let fillc = if is_dark { C_MAGENTA } else { C_DAY_REP };
    painter.rect_filled(
        Rect::from_min_size(Pos2::new(bx, by), Vec2::new((bw * prog).round(), bh)),
        0.0,
        fillc,
    );

    // Loading line — below the progress bar (no overlap).
    let load_col = if is_dark { C_CYAN } else { C_DAY_TEXT };
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
        let cc = if is_dark { C_CYAN } else { C_WHITE };
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

// --- RASTER HELPERS ---------------------------------------------------------

/// Additive radial glow (core -> falloff) for the moon/sun halo.
fn glow(buf: &mut [Color32], f: &Frame, c: (f32, f32), radius: f32, col: Rgb, intensity: f32) {
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
fn disc(buf: &mut [Color32], f: &Frame, c: (f32, f32), r: f32, col: Rgb, a: f32) {
    fill_disc(buf, f, c, r, col, a);
}

fn fill_disc(buf: &mut [Color32], f: &Frame, c: (f32, f32), r: f32, col: Rgb, a: f32) {
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
fn fill_disc_clip(
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

fn put(buf: &mut [Color32], w: usize, h: usize, x: f32, y: f32, col: Rgb, a: f32) {
    if x < 0.0 || y < 0.0 {
        return;
    }
    let (xi, yi) = (x as usize, y as usize);
    if xi < w && yi < h {
        blend(buf, w, xi, yi, col, a);
    }
}

fn blend(buf: &mut [Color32], w: usize, x: usize, y: usize, col: Rgb, a: f32) {
    let idx = y * w + x;
    buf[idx] = to_col(lerp3(rgb(buf[idx]), col, a.clamp(0.0, 1.0)));
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

// --- SMALL MATH -------------------------------------------------------------

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp3(a: Rgb, b: Rgb, t: f32) -> Rgb {
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}

fn add3(a: Rgb, b: Rgb, amt: f32) -> Rgb {
    (a.0 + b.0 * amt, a.1 + b.1 * amt, a.2 + b.2 * amt)
}

fn mul3(a: Rgb, m: f32) -> Rgb {
    (a.0 * m, a.1 * m, a.2 * m)
}

fn rgb<C: Into<RgbSrc>>(c: C) -> Rgb {
    c.into().0
}

/// Adapter so `rgb()` accepts both `Color32` and raw `(f32,f32,f32)` literals.
struct RgbSrc(Rgb);
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

fn to_col(c: Rgb) -> Color32 {
    Color32::from_rgb(
        c.0.clamp(0.0, 255.0) as u8,
        c.1.clamp(0.0, 255.0) as u8,
        c.2.clamp(0.0, 255.0) as u8,
    )
}

fn bayer4(x: usize, y: usize) -> f32 {
    const M: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    (M[y % 4][x % 4] as f32 + 0.5) / 16.0
}

fn hash2(x: u32, y: u32) -> f32 {
    let mut h = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263));
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^= h >> 16;
    (h & 0x00ff_ffff) as f32 / 0x00ff_ffff as f32
}
