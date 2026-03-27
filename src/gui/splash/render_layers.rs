// --- SPLASH SCREEN SCENE LAYERS ---
// Paint functions for individual scene layers: celestial body, clouds, retro grid, voxels, UI text.

use super::math::{Vec3, smoothstep};
use super::scene::{Cloud, MoonFeature, Voxel};
use super::{
    ANIMATION_DURATION, C_CLOUD_CORE, C_CLOUD_WHITE, C_CYAN, C_DAY_REP, C_DAY_SEC, C_DAY_TEXT,
    C_MAGENTA, C_MOON_BASE, C_MOON_GLOW, C_MOON_HIGHLIGHT, C_MOON_SHADOW, C_SUN_BODY, C_SUN_FLARE,
    C_SUN_GLOW, C_SUN_HIGHLIGHT, C_WHITE, DrawListEntry,
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use std::cell::RefCell;
use std::cmp::Ordering;

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_celestial_body(
    painter: &egui::Painter,
    center: Pos2,
    t: f32,
    warp_prog: f32,
    master_alpha: f32,
    is_dark: bool,
    mouse_influence: &Vec2,
    moon_features: &[MoonFeature],
) {
    let moon_parallax = *mouse_influence * -30.0;
    let moon_base_pos = center + Vec2::new(0.0, -40.0) + moon_parallax;
    let moon_rad = 140.0;
    let moon_alpha = master_alpha * (1.0 - warp_prog * 3.0).clamp(0.0, 1.0);

    if moon_alpha <= 0.01 {
        return;
    }

    if is_dark {
        paint_moon(
            painter,
            center,
            t,
            moon_base_pos,
            moon_rad,
            moon_alpha,
            moon_features,
        );
    } else {
        paint_sun(
            painter,
            t,
            moon_base_pos,
            moon_rad,
            moon_alpha,
            moon_features,
        );
    }
}

fn paint_moon(
    painter: &egui::Painter,
    _center: Pos2,
    t: f32,
    moon_base_pos: Pos2,
    moon_rad: f32,
    moon_alpha: f32,
    moon_features: &[MoonFeature],
) {
    let moon_bob = (t * 0.5).sin() * 5.0;
    let final_moon_pos = moon_base_pos + Vec2::new(0.0, moon_bob);

    painter.circle_filled(
        final_moon_pos,
        moon_rad * 1.6,
        C_MOON_GLOW.linear_multiply(0.03 * moon_alpha),
    );
    painter.circle_filled(
        final_moon_pos,
        moon_rad * 1.2,
        C_MOON_GLOW.linear_multiply(0.08 * moon_alpha),
    );

    painter.circle_filled(
        final_moon_pos,
        moon_rad,
        C_MOON_BASE.linear_multiply(moon_alpha),
    );
    painter.circle_filled(
        final_moon_pos + Vec2::new(10.0, 10.0),
        moon_rad * 0.9,
        Color32::from_black_alpha((50.0 * moon_alpha) as u8),
    );
    painter.circle_filled(
        final_moon_pos - Vec2::new(10.0, 10.0),
        moon_rad * 0.85,
        Color32::from_white_alpha((20.0 * moon_alpha) as u8),
    );

    let feature_rot = t * 0.05;

    for feat in moon_features {
        let fx = feat.pos.x;
        let fy = feat.pos.y;

        let rot_cos = feature_rot.cos();
        let rot_sin = feature_rot.sin();
        let r_x = fx * rot_cos - fy * rot_sin;
        let r_y = fx * rot_sin + fy * rot_cos;

        let dist_sq = r_x * r_x + r_y * r_y;
        if dist_sq > 0.95 {
            continue;
        }

        let f_pos = final_moon_pos + Vec2::new(r_x * moon_rad, r_y * moon_rad);
        let z_depth = (1.0 - dist_sq).sqrt();
        let f_radius = feat.radius * moon_rad * (0.5 + 0.5 * z_depth);
        let f_alpha = moon_alpha * z_depth;

        if feat.is_crater {
            painter.circle_filled(
                f_pos + Vec2::new(-1.0, -1.0),
                f_radius,
                C_MOON_SHADOW.linear_multiply(f_alpha * 0.8),
            );
            painter.circle_filled(
                f_pos + Vec2::new(1.0, 1.0),
                f_radius * 0.9,
                C_MOON_HIGHLIGHT.linear_multiply(f_alpha * 0.4),
            );
        } else {
            painter.circle_filled(
                f_pos,
                f_radius,
                C_MOON_SHADOW.linear_multiply(f_alpha * 0.3),
            );
        }
    }

    painter.circle_stroke(
        final_moon_pos - Vec2::new(2.0, 2.0),
        moon_rad - 1.0,
        Stroke::new(2.0, C_MOON_HIGHLIGHT.linear_multiply(0.4 * moon_alpha)),
    );
}

fn paint_sun(
    painter: &egui::Painter,
    t: f32,
    moon_base_pos: Pos2,
    moon_rad: f32,
    moon_alpha: f32,
    moon_features: &[MoonFeature],
) {
    let sun_bob = (t * 0.5).sin() * 5.0;
    let final_sun_pos = moon_base_pos + Vec2::new(0.0, sun_bob);

    painter.circle_filled(
        final_sun_pos,
        moon_rad * 2.0,
        C_SUN_GLOW.linear_multiply(0.1 * moon_alpha),
    );
    painter.circle_filled(
        final_sun_pos,
        moon_rad * 1.4,
        C_SUN_GLOW.linear_multiply(0.2 * moon_alpha),
    );

    painter.circle_filled(
        final_sun_pos,
        moon_rad,
        C_SUN_BODY.linear_multiply(moon_alpha),
    );

    let feature_rot = t * 0.08;
    for feat in moon_features {
        let fx = feat.pos.x;
        let fy = feat.pos.y;

        let rot_cos = feature_rot.cos();
        let rot_sin = feature_rot.sin();
        let r_x = fx * rot_cos - fy * rot_sin;
        let r_y = fx * rot_sin + fy * rot_cos;

        let dist_sq = r_x * r_x + r_y * r_y;
        if dist_sq > 0.95 {
            continue;
        }

        let f_pos = final_sun_pos + Vec2::new(r_x * moon_rad, r_y * moon_rad);
        let z_depth = (1.0 - dist_sq).sqrt();
        let f_radius = feat.radius * moon_rad * (0.5 + 0.5 * z_depth);
        let f_alpha = moon_alpha * z_depth;

        if feat.is_crater {
            painter.circle_filled(
                f_pos,
                f_radius * 0.6,
                Color32::from_rgb(160, 60, 0).linear_multiply(f_alpha * 0.8),
            );
        } else {
            painter.circle_filled(
                f_pos,
                f_radius * 1.5,
                C_SUN_FLARE.linear_multiply(f_alpha * 0.3),
            );
            painter.circle_filled(
                f_pos,
                f_radius * 0.8,
                C_WHITE.linear_multiply(f_alpha * 0.5),
            );
        }
    }

    painter.circle_stroke(
        final_sun_pos,
        moon_rad - 1.0,
        Stroke::new(3.0, C_SUN_HIGHLIGHT.linear_multiply(0.5 * moon_alpha)),
    );
}

pub(super) fn paint_clouds(
    cloud_painter: &egui::Painter,
    center: Pos2,
    clouds: &[Cloud],
    mouse_influence: &Vec2,
    warp_prog: f32,
    master_alpha: f32,
    is_dark: bool,
) {
    let cloud_parallax = *mouse_influence * -15.0;

    for (i, cloud) in clouds.iter().enumerate() {
        let c_x = center.x + cloud.pos.x + cloud_parallax.x;
        let c_y = center.y + cloud.pos.y + cloud_parallax.y;

        let rnd = (i as f32 * 0.73).fract();
        let start = rnd * 0.6;
        let dur = 0.3;
        let local_fade = if warp_prog > 0.0 {
            let p = ((warp_prog - start) / dur).clamp(0.0, 1.0);
            1.0 - p
        } else {
            1.0
        };

        let cloud_alpha = cloud.opacity * master_alpha * local_fade;

        if cloud_alpha > 0.01 {
            for (offset, puff_r_mult) in &cloud.puffs {
                let p_pos = Pos2::new(c_x, c_y) + (*offset * cloud.scale);
                let radius = 30.0 * cloud.scale * puff_r_mult;

                let core_col = if is_dark {
                    C_CLOUD_CORE.linear_multiply(cloud_alpha * 0.95)
                } else {
                    C_CLOUD_WHITE.linear_multiply(cloud_alpha * 0.95)
                };

                cloud_painter.circle_filled(p_pos + Vec2::new(2.0, 5.0), radius, core_col);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_retro_grid(
    painter: &egui::Painter,
    rect: &Rect,
    center: Pos2,
    horizon: f32,
    t: f32,
    warp_prog: f32,
    master_alpha: f32,
    is_dark: bool,
) {
    let render_t = t.min(ANIMATION_DURATION + 5.0);
    let cam_y = 150.0 + (render_t * 30.0) + (warp_prog * 10000.0);

    for i in 0..16 {
        let rnd = (i as f32 * 0.9).sin() * 0.5 + 0.5;
        let start = rnd * 0.5;
        let dur = 0.25;
        let local_fade = if warp_prog > 0.0 {
            let p = ((warp_prog - start) / dur).clamp(0.0, 1.0);
            1.0 - p
        } else {
            1.0
        };

        if local_fade <= 0.0 {
            continue;
        }

        let z_dist = 1.0 + (i as f32 * 0.5) - ((cam_y * 0.05) % 0.5);
        let perspective = 250.0 / (z_dist - warp_prog * 0.8).max(0.1);
        let y = horizon + perspective * 0.6;

        if y > rect.bottom() || y < horizon {
            continue;
        }

        let w = rect.width() * (2.5 / z_dist);
        let x1 = center.x - w;
        let x2 = center.x + w;

        let alpha_grid = (1.0 - (y - horizon) / (rect.bottom() - horizon)).powf(0.5)
            * master_alpha
            * 0.5
            * local_fade;

        let (grid_col, thickness) = if is_dark {
            (C_MAGENTA, 1.5)
        } else {
            (C_DAY_REP, 4.0 * (1.0 - (y - horizon) / rect.height()))
        };

        painter.line_segment(
            [Pos2::new(x1, y), Pos2::new(x2, y)],
            Stroke::new(thickness, grid_col.linear_multiply(alpha_grid)),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_voxels(
    painter: &egui::Painter,
    cloud_painter: &egui::Painter,
    _rect: &Rect,
    center: Pos2,
    t: f32,
    warp_prog: f32,
    master_alpha: f32,
    is_dark: bool,
    voxels: &[Voxel],
    mouse_influence: &Vec2,
    draw_list: &RefCell<Vec<DrawListEntry>>,
) {
    let physics_t = t.min(ANIMATION_DURATION);
    let fov = 800.0;
    let cam_fly_dist = warp_prog * 2000.0;
    let cam_dist = (600.0 + smoothstep(0.0, 8.0, physics_t) * 100.0) - cam_fly_dist;

    let global_rot = Vec3::new(mouse_influence.y * 0.2, mouse_influence.x * 0.2, 0.0);

    let light_dir_2d = Vec2::new(-0.4, -0.4);

    let mut draw_list_ref = draw_list.borrow_mut();
    draw_list_ref.clear();
    let draw_list_vec = &mut *draw_list_ref;

    let sphere_radius_base = 8.5;

    for v in voxels {
        let mut local_debris_alpha = 1.0;
        if v.is_debris {
            let fade_start = 4.0 + (v.noise_factor * 3.0);
            let fade_end = fade_start + 2.5;
            local_debris_alpha = 1.0 - smoothstep(fade_start, fade_end, physics_t);
            if local_debris_alpha <= 0.01 {
                continue;
            }
        }

        let mut v_center = v.pos;
        v_center = v_center
            .rotate_x(global_rot.x)
            .rotate_y(global_rot.y)
            .rotate_z(global_rot.z);

        let z_depth = cam_dist - v_center.z;
        if z_depth < 0.1 {
            continue;
        }

        let scale = fov / z_depth;
        let screen_pos = Pos2::new(center.x + v_center.x * scale, center.y - v_center.y * scale);

        let r = sphere_radius_base * v.scale * scale;

        let mut alpha_local = master_alpha;
        if v.is_debris {
            alpha_local *= local_debris_alpha;
            let base_opacity = 0.4 + (v.noise_factor * 0.6);
            alpha_local *= base_opacity;
            let twinkle =
                (t * (3.0 + v.noise_factor * 2.0) + v.noise_factor * 50.0).sin() * 0.25 + 0.75;
            alpha_local *= twinkle;
        }

        let mut base_col = v.color;

        if !v.is_debris && v.color != C_WHITE {
            if is_dark {
                if v.color == C_DAY_REP {
                    base_col = C_MAGENTA;
                } else if v.color == C_DAY_SEC {
                    base_col = C_CYAN;
                }
            } else if v.color == C_MAGENTA {
                base_col = C_DAY_REP;
            } else if v.color == C_CYAN {
                base_col = C_DAY_SEC;
            }
        }

        if !is_dark && v.is_debris {
            base_col = C_CLOUD_WHITE;
        }

        if warp_prog > 0.0 {
            let start_threshold = v.noise_factor * 0.75;
            let move_duration = 0.25;
            let local_linear = ((warp_prog - start_threshold) / move_duration).clamp(0.0, 1.0);
            let fade = (local_linear * 1.5).clamp(0.0, 1.0);
            alpha_local *= 1.0 - fade;
        }

        let final_col = base_col.linear_multiply(alpha_local);

        draw_list_vec.push((
            z_depth,
            screen_pos,
            r,
            final_col,
            v.color == C_WHITE || v.color == C_DAY_SEC,
            v.is_debris,
        ));
    }

    draw_list_vec.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));

    for (_, pos, r, col, is_white_voxel, is_debris) in draw_list_vec.iter().copied() {
        let p = if !is_dark && is_debris {
            cloud_painter
        } else {
            painter
        };

        if is_dark || !is_debris {
            let shadow_col = if is_dark {
                Color32::from_black_alpha(200).linear_multiply(col.a() as f32 / 255.0)
            } else if is_white_voxel {
                Color32::from_rgb(100, 120, 150).linear_multiply(col.a() as f32 / 255.0)
            } else {
                Color32::from_rgb(0, 40, 100).linear_multiply(col.a() as f32 / 255.0)
            };
            p.circle_filled(pos, r, shadow_col);

            let body_offset = light_dir_2d * (r * 0.15);
            p.circle_filled(pos + body_offset, r * 0.85, col);

            let glow_col = if is_white_voxel {
                Color32::WHITE.linear_multiply(0.5)
            } else {
                col.linear_multiply(0.5)
            };
            let gradient_offset = light_dir_2d * (r * 0.3);
            p.circle_filled(pos + gradient_offset, r * 0.5, glow_col);
        } else {
            p.circle_filled(pos, r, col);
        }

        if !is_debris {
            let highlight_pos = pos + (light_dir_2d * (r * 0.5));
            let highlight_alpha = if is_dark { 0.8 } else { 0.9 };
            let highlight_col = Color32::from_white_alpha((255.0 * highlight_alpha) as u8)
                .linear_multiply(col.a() as f32 / 255.0);

            painter.circle_filled(highlight_pos, r * 0.25, highlight_col);
            painter.circle_filled(
                highlight_pos,
                r * 0.15,
                Color32::WHITE.linear_multiply(col.a() as f32 / 255.0),
            );
        }
    }
}

pub(super) fn paint_ui_text(
    painter: &egui::Painter,
    center: Pos2,
    t: f32,
    warp_prog: f32,
    master_alpha: f32,
    is_dark: bool,
    loading_text: &str,
) {
    if master_alpha <= 0.1 || warp_prog >= 0.1 {
        return;
    }

    let physics_t = t.min(ANIMATION_DURATION);
    let ui_alpha = 1.0 - (warp_prog * 10.0).clamp(0.0, 1.0);

    let ui_text_col = if is_dark { C_WHITE } else { C_DAY_TEXT };
    let ui_color = ui_text_col.linear_multiply(master_alpha * ui_alpha);

    let loading_col = if is_dark {
        C_CYAN.linear_multiply(master_alpha * ui_alpha)
    } else {
        C_DAY_TEXT.linear_multiply(master_alpha * ui_alpha)
    };

    let click_col = if is_dark {
        C_CYAN.linear_multiply(master_alpha * ui_alpha)
    } else {
        C_WHITE.linear_multiply(master_alpha * ui_alpha)
    };

    let magenta_color = if is_dark {
        C_MAGENTA.linear_multiply(master_alpha * ui_alpha)
    } else {
        C_DAY_REP.linear_multiply(master_alpha * ui_alpha)
    };

    let title_text = format!("SCREEN GOATED TOOLBOX {}", env!("CARGO_PKG_VERSION"));
    let title_font = FontId::proportional(30.0);
    let title_pos = center + Vec2::new(0.0, 150.0);

    let shadow_col = if is_dark {
        C_MAGENTA.linear_multiply(master_alpha * ui_alpha)
    } else {
        C_WHITE.linear_multiply(master_alpha * ui_alpha)
    };

    painter.text(
        title_pos + Vec2::new(2.0, 2.0),
        Align2::CENTER_TOP,
        &title_text,
        title_font.clone(),
        shadow_col,
    );
    painter.text(
        title_pos,
        Align2::CENTER_TOP,
        &title_text,
        title_font,
        ui_color,
    );
    painter.text(
        center + Vec2::new(0.0, 210.0),
        Align2::CENTER_TOP,
        loading_text,
        FontId::monospace(12.0),
        loading_col,
    );

    let bar_rect = Rect::from_center_size(center + Vec2::new(0.0, 230.0), Vec2::new(200.0, 4.0));
    let bar_bg_col = if is_dark {
        Color32::from_white_alpha((30.0 * ui_alpha) as u8)
    } else {
        Color32::from_black_alpha((30.0 * ui_alpha) as u8)
    };
    painter.rect_filled(bar_rect, 2.0, bar_bg_col);
    let prog = (physics_t / (ANIMATION_DURATION - 0.5)).clamp(0.0, 1.0);
    let mut fill = bar_rect;
    fill.set_width(bar_rect.width() * prog);
    painter.rect_filled(fill, 2.0, magenta_color);

    if t > ANIMATION_DURATION - 0.5 {
        let pulse = (t * 5.0).sin().abs() * 0.7 + 0.3;
        painter.text(
            center - Vec2::new(0.0, 220.0),
            Align2::CENTER_TOP,
            "Click anywhere to continue",
            FontId::proportional(14.0),
            click_col.linear_multiply(pulse),
        );
    }
}
