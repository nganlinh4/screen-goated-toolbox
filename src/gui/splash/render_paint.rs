// --- SPLASH SCREEN PAINTING ---
// Main paint entry point, background, stars, god rays, and theme switcher.
// Scene layer painters (celestial body, clouds, grid, voxels, UI text) are in render_layers.rs.

use super::render_layers;
use super::scene::{Cloud, MoonFeature, Star, Voxel};
use super::{C_SKY_DAY_TOP, C_VOID, C_WHITE, DrawListEntry, EXIT_DURATION};
use crate::gui::icons::{Icon, paint_icon};
use crate::{WINDOW_HEIGHT, WINDOW_WIDTH};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use std::cell::RefCell;
use std::f32::consts::PI;

pub struct SplashPaintContext<'a> {
    pub ctx: &'a egui::Context,
    pub start_time: f64,
    pub exit_start_time: Option<f64>,
    pub voxels: &'a [Voxel],
    pub clouds: &'a [Cloud],
    pub stars: &'a [Star],
    pub moon_features: &'a [MoonFeature],
    pub mouse_influence: Vec2,
    pub is_dark: bool,
    pub loading_text: &'a str,
    pub draw_list: &'a RefCell<Vec<DrawListEntry>>,
}

/// Paint the splash screen
pub fn paint(paint_ctx: SplashPaintContext<'_>) -> bool {
    let SplashPaintContext {
        ctx,
        start_time,
        exit_start_time,
        voxels,
        clouds,
        stars,
        moon_features,
        mouse_influence,
        is_dark,
        loading_text,
        draw_list,
    } = paint_ctx;
    let now = ctx.input(|i| i.time);
    let t = (now - start_time) as f32;

    let mut warp_prog = 0.0;
    if let Some(exit_start) = exit_start_time {
        let dt = (now - exit_start) as f32;
        warp_prog = (dt / EXIT_DURATION).powi(5);
    }

    let viewport_rect = ctx.input(|i| {
        i.viewport()
            .inner_rect
            .unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::ZERO))
    });

    let size = if viewport_rect.width() < 100.0 || viewport_rect.height() < 100.0 {
        Vec2::new(WINDOW_WIDTH, WINDOW_HEIGHT)
    } else {
        viewport_rect.size()
    };

    let rect = Rect::from_min_size(Pos2::ZERO, size);

    // --- INTERACTION BLOCKER & DRAG HANDLE ---
    egui::Area::new(egui::Id::new("splash_blocker"))
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::ZERO)
        .show(ctx, |ui| {
            let resp = ui.allocate_response(
                size,
                egui::Sense::click_and_drag().union(egui::Sense::hover()),
            );

            if resp.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
        });

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("splash_overlay"),
    ));

    let center = rect.center();

    let alpha_fade_in = 0.4;
    let alpha = if t < alpha_fade_in {
        t / alpha_fade_in
    } else {
        1.0
    };
    let master_alpha = alpha.clamp(0.0, 1.0);

    // --- THEME SWITCHER OVERLAY ---
    let theme_clicked = paint_theme_switcher(ctx, now, exit_start_time, master_alpha, is_dark);

    // 1. Background
    let mut bg_color = if is_dark { C_VOID } else { C_SKY_DAY_TOP };
    if t < 0.5 {
        let t_fade = (t / 0.5).clamp(0.0, 1.0);
        let start_col = if is_dark {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        bg_color = Color32::from_rgb(
            super::math::lerp(start_col.r() as f32, bg_color.r() as f32, t_fade) as u8,
            super::math::lerp(start_col.g() as f32, bg_color.g() as f32, t_fade) as u8,
            super::math::lerp(start_col.b() as f32, bg_color.b() as f32, t_fade) as u8,
        );
    }

    let sky_exit_fade = (1.0 - warp_prog * 4.0).clamp(0.0, 1.0);

    if is_dark {
        painter.rect_filled(rect, 12.0, bg_color.linear_multiply(sky_exit_fade));
    } else {
        let c_top = C_SKY_DAY_TOP.linear_multiply(sky_exit_fade);
        painter.rect_filled(rect, 12.0, c_top);
    }

    if master_alpha <= 0.05 {
        return theme_clicked;
    }

    // --- LAYER 0: STARS ---
    paint_stars(
        &painter,
        &rect,
        stars,
        &mouse_influence,
        t,
        warp_prog,
        master_alpha,
        is_dark,
    );

    // --- LAYER 1.5: GOD RAYS (DAY MODE) ---
    paint_god_rays(&painter, center, t, warp_prog, master_alpha, is_dark);

    // --- LAYER 2: THE CELESTIAL BODY (MOON/SUN) ---
    render_layers::paint_celestial_body(
        &painter,
        center,
        t,
        warp_prog,
        master_alpha,
        is_dark,
        &mouse_influence,
        moon_features,
    );

    // --- LAYER 3: VOLUMETRIC CLOUDS ---
    let horizon = center.y + 120.0;
    let cloud_painter = if !is_dark {
        painter.with_clip_rect(Rect::from_min_max(
            rect.min,
            Pos2::new(rect.max.x, horizon + 30.0),
        ))
    } else {
        painter.clone()
    };

    render_layers::paint_clouds(
        &cloud_painter,
        center,
        clouds,
        &mouse_influence,
        warp_prog,
        master_alpha,
        is_dark,
    );

    // --- LAYER 4: RETRO GRID ---
    render_layers::paint_retro_grid(
        &painter,
        &rect,
        center,
        horizon,
        t,
        warp_prog,
        master_alpha,
        is_dark,
    );

    // --- LAYER 5: 3D VOXELS (SPHERES) ---
    render_layers::paint_voxels(
        &painter,
        &cloud_painter,
        &rect,
        center,
        t,
        warp_prog,
        master_alpha,
        is_dark,
        voxels,
        &mouse_influence,
        draw_list,
    );

    // --- LAYER 6: UI TEXT ---
    render_layers::paint_ui_text(
        &painter,
        center,
        t,
        warp_prog,
        master_alpha,
        is_dark,
        loading_text,
    );

    theme_clicked
}

fn paint_theme_switcher(
    ctx: &egui::Context,
    now: f64,
    exit_start_time: Option<f64>,
    master_alpha: f32,
    is_dark: bool,
) -> bool {
    let mut theme_clicked = false;

    let switcher_alpha = if let Some(exit_start) = exit_start_time {
        let exit_dt = (now - exit_start) as f32;
        (1.0 - exit_dt / 0.3).max(0.0)
    } else {
        1.0
    };

    if master_alpha > 0.1 && switcher_alpha > 0.01 {
        egui::Area::new(egui::Id::new("splash_theme_switcher"))
            .order(egui::Order::Tooltip)
            .fixed_pos(Pos2::new(14.0, 11.0))
            .show(ctx, |ui| {
                let icon = if is_dark { Icon::Sun } else { Icon::Moon };

                let pulse = (now * 2.0).sin().abs() * 0.2 + 0.8;
                let btn_bg = if is_dark {
                    Color32::from_white_alpha((30.0 * switcher_alpha) as u8)
                } else {
                    Color32::from_black_alpha((20.0 * switcher_alpha) as u8)
                };

                let icon_color = if is_dark {
                    Color32::WHITE.linear_multiply(switcher_alpha)
                } else {
                    Color32::BLACK.linear_multiply(switcher_alpha)
                };

                let (rect, resp) = ui.allocate_at_least(Vec2::splat(32.0), egui::Sense::click());

                let fill = if resp.hovered() {
                    btn_bg.linear_multiply(1.5)
                } else {
                    btn_bg.linear_multiply(pulse as f32)
                };
                ui.painter().rect_filled(rect, 8.0, fill);

                let old_panel_fill = ctx.style().visuals.panel_fill;
                if !is_dark {
                    let cutout_color =
                        Color32::from_rgb(109, 174, 235).linear_multiply(switcher_alpha);
                    ctx.style_mut(|s| s.visuals.panel_fill = cutout_color);
                }

                paint_icon(ui.painter(), rect.shrink(6.0), icon, icon_color);

                ctx.style_mut(|s| s.visuals.panel_fill = old_panel_fill);

                if resp.clicked() && switcher_alpha > 0.9 {
                    theme_clicked = true;
                }
            });
    }

    theme_clicked
}

#[expect(
    clippy::too_many_arguments,
    reason = "star rendering needs all visual parameters"
)]
fn paint_stars(
    painter: &egui::Painter,
    rect: &Rect,
    stars: &[Star],
    mouse_influence: &Vec2,
    t: f32,
    warp_prog: f32,
    master_alpha: f32,
    is_dark: bool,
) {
    let star_offset = *mouse_influence * -10.0;
    let star_time = t * 2.0;

    for (i, star) in stars.iter().enumerate() {
        let sx = rect.left() + (star.pos.x * rect.width()) + star_offset.x;
        let sy = rect.top() + (star.pos.y * rect.height()) + star_offset.y;

        let rnd = ((i as f32 * 1.618).fract() + (star.pos.x * 10.0).fract()).fract();
        let start = rnd * 0.7;
        let dur = 0.2;
        let local_fade = if warp_prog > 0.0 {
            let p = ((warp_prog - start) / dur).clamp(0.0, 1.0);
            1.0 - p
        } else {
            1.0
        };

        let twinkle = (star.phase + star_time).sin() * 0.3 + 0.7;
        let star_alpha = (star.brightness * twinkle * master_alpha * local_fade).clamp(0.0, 1.0);

        if star_alpha > 0.1 {
            let size = star.size * (1.0 - warp_prog);
            if is_dark {
                painter.circle_filled(Pos2::new(sx, sy), size, C_WHITE.linear_multiply(star_alpha));
            } else {
                let day_star_alpha = star_alpha * 0.3;
                painter.circle_filled(
                    Pos2::new(sx, sy),
                    size,
                    C_WHITE.linear_multiply(day_star_alpha),
                );
            }
        }
    }
}

fn paint_god_rays(
    painter: &egui::Painter,
    center: Pos2,
    t: f32,
    warp_prog: f32,
    master_alpha: f32,
    is_dark: bool,
) {
    if !is_dark && master_alpha > 0.1 && warp_prog < 0.9 {
        let sun_pos = center + Vec2::new(0.0, -40.0 * (1.0 - warp_prog));
        let ray_count = 12;
        let ray_rot = t * 0.1;

        let mut mesh = egui::Mesh::default();
        let c1 = Color32::from_white_alpha(55);

        for i in 0..ray_count {
            let angle = (i as f32 / ray_count as f32) * PI * 2.0 + ray_rot;
            let next_angle = ((i as f32 + 0.5) / ray_count as f32) * PI * 2.0 + ray_rot;

            let v_idx = mesh.vertices.len() as u32;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: sun_pos,
                uv: Pos2::ZERO,
                color: Color32::TRANSPARENT,
            });

            let ray_len = 1200.0;
            let p1 = sun_pos + Vec2::new(angle.cos() * ray_len, angle.sin() * ray_len);
            let p2 = sun_pos + Vec2::new(next_angle.cos() * ray_len, next_angle.sin() * ray_len);

            mesh.vertices.push(egui::epaint::Vertex {
                pos: p1,
                uv: Pos2::ZERO,
                color: c1,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: p2,
                uv: Pos2::ZERO,
                color: c1,
            });

            mesh.add_triangle(v_idx, v_idx + 1, v_idx + 2);
        }
        painter.add(mesh);
    }
}
