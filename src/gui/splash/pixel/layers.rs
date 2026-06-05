// The scene layers, drawn back-to-front into the framebuffer: sky + haze,
// stars, god rays, the big moon/sun, clouds, the synthwave grid, the 3D "SGT"
// voxel assembly, the vignette, and the pixel theme button. All colours come
// from the rolled palette via `Frame::pal`.

use super::super::math::Vec3;
use super::super::scene::{Cloud, MoonFeature, Voxel};
use super::super::{ANIMATION_DURATION, C_WHITE, DrawListEntry};
use super::raster::{
    bayer4, blend, disc, fill_disc, fill_disc_clip, glow, hash2, lerp3, mul3, put, rgb, smooth,
    to_col,
};
use super::{Frame, VoxelDraw};
use crate::WINDOW_WIDTH;
use eframe::egui::{Color32, Pos2, Vec2};
use std::cell::RefCell;

// --- LAYER 0: BACKGROUND (sky + haze) --------------------------------------

pub(super) fn paint_background(buf: &mut [Color32], f: &Frame) {
    // Sky gradient from the palette. Keeps its full colour on exit — the dither
    // dissolve is the only exit effect (the old `1 - warp*4` blackened it).
    let base = rgb(f.pal.sky_top);
    let haze = rgb(f.pal.sky_horizon);
    let haze_a = f.pal.haze_a;

    for y in 0..f.h {
        let vy = y as f32 / f.h as f32;
        let row = lerp3(base, haze, smooth(0.0, 1.0, vy) * haze_a);
        for x in 0..f.w {
            buf[y * f.w + x] = to_col(row);
        }
    }
}

// --- LAYER 1: STARS (placed, night) ----------------------------------------

pub(super) fn paint_stars(buf: &mut [Color32], f: &Frame) {
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

pub(super) fn paint_god_rays(buf: &mut [Color32], f: &Frame) {
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

pub(super) fn paint_celestial(buf: &mut [Color32], f: &Frame, features: &[MoonFeature]) {
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
        glow(buf, f, (cx, cy), r * 2.3, rgb(f.pal.glow), 0.18 * alpha);
        disc(buf, f, (cx, cy), r, rgb(f.pal.body), alpha);
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
        glow(buf, f, (cx, cy), r * 3.0, rgb(f.pal.glow), 0.30 * alpha);
        disc(buf, f, (cx, cy), r, rgb(f.pal.body), alpha);
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
                rgb(f.pal.body_lo),
                fa * 0.8,
            );
            disc(
                buf,
                f,
                (fx + 1.0, fy + 1.0),
                fr * 0.9,
                rgb(f.pal.body_hi),
                fa * 0.4,
            );
        } else {
            disc(buf, f, (fx, fy), fr, rgb(f.pal.body_lo), fa * 0.3);
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
            disc(buf, f, (fx, fy), fr * 0.6, rgb(f.pal.body_lo), fa * 0.8);
        } else {
            disc(buf, f, (fx, fy), fr * 1.5, rgb(f.pal.flare), fa * 0.3);
            disc(buf, f, (fx, fy), fr * 0.8, rgb(f.pal.body_hi), fa * 0.5);
        }
    }
}

// --- LAYER 3: CLOUDS (dithered drifting blobs) -----------------------------

pub(super) fn paint_clouds(buf: &mut [Color32], f: &Frame, clouds: &[Cloud]) {
    // Volumetric clouds: each is a cluster of puffs (soft outer fluff + denser
    // core). Colour from the palette (dark in night, white-ish in day).
    let core = rgb(f.pal.cloud);
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

pub(super) fn paint_grid(buf: &mut [Color32], f: &Frame) {
    let horizon = f.cy + f.len(120.0);
    let bottom = f.h as f32;
    let cam_y = 150.0 + f.t.min(ANIMATION_DURATION + 5.0) * 30.0;
    let col = rgb(f.pal.accent_primary);

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

pub(super) fn draw_voxels(
    buf: &mut [Color32],
    f: &Frame,
    voxels: &[Voxel],
    dl: &RefCell<Vec<DrawListEntry>>,
) {
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
        // Spark voxels are white; the rest keep the palette colour set at init
        // (re-rolled on theme toggle). Day-mode debris reads as bright motes.
        let is_white = v.color == C_WHITE;
        let vcol = if !f.is_dark && v.is_debris {
            rgb(f.pal.cloud)
        } else {
            rgb(v.color)
        };
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
        // inner glow, specular highlight. Shadow = a darkened version of the
        // voxel's own colour, so it stays coherent in any palette.
        let shadow = mul3(col, 0.3);
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

pub(super) fn paint_vignette(buf: &mut [Color32], f: &Frame) {
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

pub(super) fn paint_theme_button(buf: &mut [Color32], f: &Frame) {
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
