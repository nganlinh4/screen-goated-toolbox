// --- SPLASH SCREEN RENDERING ---
// Update physics and paint the splash screen animation.

use super::audio::SplashAudio;
use super::math::{lerp, smoothstep, Vec3};
use super::scene::{Cloud, MoonFeature, Star, Voxel};
use super::{
    SplashStatus, ANIMATION_DURATION, C_CLOUD_CORE, C_CLOUD_WHITE, C_CYAN, C_DAY_REP, C_DAY_SEC,
    C_DAY_TEXT, C_MAGENTA, C_MOON_BASE, C_MOON_GLOW, C_MOON_HIGHLIGHT, C_MOON_SHADOW,
    C_SKY_DAY_TOP, C_SUN_BODY, C_SUN_FLARE, C_SUN_GLOW, C_SUN_HIGHLIGHT, C_VOID, C_WHITE,
    EXIT_DURATION, START_TRANSITION,
};
use crate::gui::icons::{paint_icon, Icon};
use crate::{WINDOW_HEIGHT, WINDOW_WIDTH};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::f32::consts::PI;
use std::sync::{Arc, Mutex};

/// Update the splash screen physics and state
#[allow(clippy::too_many_arguments)]
pub fn update(
    ctx: &egui::Context,
    start_time: f64,
    exit_start_time: &mut Option<f64>,
    voxels: &mut Vec<Voxel>,
    clouds: &mut Vec<Cloud>,
    mouse_influence: &mut Vec2,
    mouse_world_pos: &mut Vec3,
    loading_text: &mut String,
    is_dark: &mut bool,
    audio: &Arc<Mutex<Option<SplashAudio>>>,
    has_played_impact: &mut bool,
) -> SplashStatus {
    *is_dark = ctx.style().visuals.dark_mode;

    let now = ctx.input(|i| i.time);
    let dt = ctx.input(|i| i.stable_dt);

    if exit_start_time.is_none() {
        let t = (now - start_time) as f32;
        if t > ANIMATION_DURATION - 0.5 {
            if ctx.input(|i| i.pointer.any_click()) {
                // Prevent click on theme switcher from triggering splash exit
                let is_in_switcher = if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    pos.x < 100.0 && pos.y < 60.0
                } else {
                    false
                };

                if !is_in_switcher {
                    *exit_start_time = Some(now);
                }
            }
        }
    }

    let t_abs = (now - start_time) as f32;
    let physics_t = t_abs.min(ANIMATION_DURATION);

    // --- EXIT LOGIC ---
    let mut warp_progress = 0.0;
    if let Some(exit_start) = *exit_start_time {
        let dt = (now - exit_start) as f32;
        if dt > EXIT_DURATION {
            if let Ok(mut lock) = audio.lock() {
                if let Some(audio) = lock.as_mut() {
                    if let Ok(mut s) = audio.state.lock() {
                        s.is_finished = true;
                    }
                }
            }
            return SplashStatus::Finished;
        }
        warp_progress = (dt / EXIT_DURATION).clamp(0.0, 1.0);
    }

    // --- UPDATE AUDIO STATE ---
    if let Ok(mut lock) = audio.lock() {
        if let Some(audio) = lock.as_mut() {
            if let Ok(mut s) = audio.state.lock() {
                s.physics_t = physics_t;
                s.warp_progress = warp_progress;
                s.is_dark = *is_dark;

                // Trigger impact once when assembly is nearly complete
                if physics_t > 1.6 && !*has_played_impact {
                    s.impact_trigger = true;
                    drop(s);
                    *has_played_impact = true;
                }
            }
        }
    }

    ctx.request_repaint();

    // --- UPDATE CLOUDS ---
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

    for cloud in clouds.iter_mut() {
        cloud.pos.x += cloud.velocity * dt;
        if cloud.pos.x > rect.width() / 2.0 + 300.0 {
            cloud.pos.x = -rect.width() / 2.0 - 300.0;
        }
    }

    if let Some(pointer) = ctx.input(|i| i.pointer.hover_pos()) {
        let center = rect.center();
        let tx = (pointer.x - center.x) / center.x;
        let ty = (pointer.y - center.y) / center.y;
        mouse_influence.x += (tx - mouse_influence.x) * 0.05;
        mouse_influence.y += (ty - mouse_influence.y) * 0.05;

        let cam_z_offset = warp_progress * 2000.0;
        let cam_dist =
            600.0 + smoothstep(0.0, ANIMATION_DURATION, physics_t) * 100.0 - cam_z_offset;

        let fov = 800.0;
        let mouse_wx = (pointer.x - center.x) * cam_dist / fov;
        let mouse_wy = -(pointer.y - center.y) * cam_dist / fov;
        *mouse_world_pos = Vec3::new(mouse_wx, mouse_wy, 0.0);
    }

    if exit_start_time.is_none() {
        if t_abs < 0.8 {
            *loading_text = "TRANSLATING...".to_string();
        } else if t_abs < 1.6 {
            *loading_text = "OCR...".to_string();
        } else if t_abs < 2.4 {
            *loading_text = "TRANSCRIBING...".to_string();
        } else {
            *loading_text = "nganlinh4".to_string();
        }
    } else {
        *loading_text = "READY TO ROCK!".to_string();
    }

    // --- PHYSICS UPDATE (Voxels) ---
    let helix_spin = physics_t * 2.0 + (physics_t * physics_t * 0.2);

    for v in voxels.iter_mut() {
        let my_start = START_TRANSITION + (v.noise_factor * 0.6);
        let my_end = my_start + 1.0;
        let progress = smoothstep(my_start, my_end, physics_t);

        if progress <= 0.0 {
            let current_h_y = v.helix_y + (physics_t * 2.0 + v.noise_factor * 10.0).sin() * 5.0;
            let current_angle = v.helix_angle_offset + helix_spin;
            let mut current_radius = v.helix_radius * (1.0 + physics_t * 0.1);

            if v.is_debris && physics_t > ANIMATION_DURATION * 0.7 {
                let flare_start = ANIMATION_DURATION * 0.7;
                let flare = (physics_t - flare_start).powi(2) * 20.0;
                current_radius += flare;
            }

            v.pos = Vec3::new(
                current_angle.cos() * current_radius,
                current_h_y,
                current_angle.sin() * current_radius,
            );
            v.rot.y += 0.05;
            v.scale = 0.8;
            v.velocity = Vec3::ZERO;
        } else {
            let current_h_y = v.helix_y + (physics_t * 2.0 + v.noise_factor * 10.0).sin() * 5.0;
            let current_angle = v.helix_angle_offset + helix_spin;

            let mut current_radius = v.helix_radius * (1.0 + physics_t * 0.1);
            if v.is_debris && physics_t > ANIMATION_DURATION * 0.7 {
                let flare_start = ANIMATION_DURATION * 0.7;
                let flare = (physics_t - flare_start).powi(2) * 20.0;
                current_radius += flare;
            }

            let helix_pos = Vec3::new(
                current_angle.cos() * current_radius,
                current_h_y,
                current_angle.sin() * current_radius,
            );
            let mut target_base = v.target_pos;

            // Add slow cosmic drift/orbit to debris targets
            if v.is_debris {
                let orbit_speed = 0.02 + v.noise_factor * 0.08;
                target_base = target_base.rotate_y(t_abs * orbit_speed);
                target_base.y += (t_abs * 0.5 + v.noise_factor * 10.0).sin() * 20.0;
            }

            if warp_progress > 0.0 {
                let start_threshold = v.noise_factor * 0.75;
                let move_duration = 0.25;
                let local_linear =
                    ((warp_progress - start_threshold) / move_duration).clamp(0.0, 1.0);
                let local_eased = local_linear * local_linear * local_linear;

                if local_eased > 0.0 {
                    let radial = Vec3::new(v.pos.x, v.pos.y, 0.0).normalize();
                    let curl_angle = local_eased * (v.noise_factor - 0.5) * 6.0;
                    let swirl_vec = radial.rotate_z(curl_angle);
                    let dist_mult = 1200.0;

                    target_base = target_base.add(swirl_vec.mul(local_eased * dist_mult));
                    target_base.z += local_eased * (v.noise_factor - 0.5) * 800.0;
                }
            }

            let pos = helix_pos.lerp(target_base, progress);

            if progress > 0.9 && !v.is_debris && warp_progress == 0.0 {
                let to_mouse = pos.sub(*mouse_world_pos);
                let dist_sq = to_mouse.x * to_mouse.x + to_mouse.y * to_mouse.y;
                if dist_sq < 6400.0 {
                    let dist = dist_sq.sqrt();
                    let force = (80.0 - dist) / 80.0;
                    v.velocity = v.velocity.add(to_mouse.normalize().mul(force * 2.0));
                    v.rot.x += to_mouse.y * force * 0.01;
                    v.rot.y -= to_mouse.x * force * 0.01;
                }
            }

            let displacement = pos.sub(target_base);
            let spring_force = displacement.mul(-0.1);
            v.velocity = v.velocity.add(spring_force);
            v.velocity = v.velocity.mul(0.90);

            v.pos = pos.add(v.velocity);
            v.rot = v.rot.lerp(Vec3::ZERO, 0.1);

            if progress > 0.95 {
                let impact = (physics_t - my_end).max(0.0);
                let pulse = (impact * 10.0).sin() * (-3.0 * impact).exp() * 0.5;
                v.scale = 1.0 + pulse;
            } else {
                v.scale = lerp(0.8, 1.0, progress);
            }
        }
    }

    SplashStatus::Ongoing
}

/// Paint the splash screen
#[allow(clippy::too_many_arguments)]
pub fn paint(
    ctx: &egui::Context,
    start_time: f64,
    exit_start_time: Option<f64>,
    voxels: &[Voxel],
    clouds: &[Cloud],
    stars: &[Star],
    moon_features: &[MoonFeature],
    mouse_influence: Vec2,
    is_dark: bool,
    loading_text: &str,
    draw_list: &RefCell<Vec<(f32, Pos2, f32, Color32, bool, bool)>>,
) -> bool {
    let mut theme_clicked = false;
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
            lerp(start_col.r() as f32, bg_color.r() as f32, t_fade) as u8,
            lerp(start_col.g() as f32, bg_color.g() as f32, t_fade) as u8,
            lerp(start_col.b() as f32, bg_color.b() as f32, t_fade) as u8,
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
    let star_offset = mouse_influence * -10.0;
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

    // --- LAYER 1.5: GOD RAYS (DAY MODE) ---
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

    // --- LAYER 2: THE REALISTIC PINK MOON ---
    let moon_parallax = mouse_influence * -30.0;
    let moon_base_pos = center + Vec2::new(0.0, -40.0) + moon_parallax;
    let moon_rad = 140.0;
    let moon_alpha = master_alpha * (1.0 - warp_prog * 3.0).clamp(0.0, 1.0);

    if moon_alpha > 0.01 {
        if is_dark {
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
        } else {
            // --- SUN VARIANT ---
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
    }

    // --- LAYER 3: VOLUMETRIC DARK CLOUDS ---
    let cloud_parallax = mouse_influence * -15.0;

    let horizon = center.y + 120.0;
    let cloud_painter = if !is_dark {
        painter.with_clip_rect(Rect::from_min_max(
            rect.min,
            Pos2::new(rect.max.x, horizon + 30.0),
        ))
    } else {
        painter.clone()
    };

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

    // --- LAYER 4: RETRO GRID ---
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

    // --- LAYER 5: 3D VOXELS (SPHERES) ---
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
            } else {
                if v.color == C_MAGENTA {
                    base_col = C_DAY_REP;
                } else if v.color == C_CYAN {
                    base_col = C_DAY_SEC;
                }
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
            &cloud_painter
        } else {
            &painter
        };

        if !(!is_dark && is_debris) {
            let shadow_col = if is_dark {
                Color32::from_black_alpha(200).linear_multiply(col.a() as f32 / 255.0)
            } else {
                if is_white_voxel {
                    Color32::from_rgb(100, 120, 150).linear_multiply(col.a() as f32 / 255.0)
                } else {
                    Color32::from_rgb(0, 40, 100).linear_multiply(col.a() as f32 / 255.0)
                }
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

    // --- LAYER 6: UI TEXT ---
    if master_alpha > 0.1 && warp_prog < 0.1 {
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

        let bar_rect =
            Rect::from_center_size(center + Vec2::new(0.0, 230.0), Vec2::new(200.0, 4.0));
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

    theme_clicked
}
