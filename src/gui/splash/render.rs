// --- SPLASH SCREEN UPDATE LOGIC ---
// Update physics and state for the splash screen animation.
// Painting is in pixel.rs (the framebuffer pixel-art renderer).

use super::audio::SplashAudio;
use super::math::{Vec3, lerp, smoothstep};
use super::scene::{Cloud, Voxel};
use super::{
    ANIMATION_DURATION, AUTO_EXIT_DELAY, EXIT_DURATION, IDLE_FRAME_INTERVAL, START_TRANSITION,
    SplashStatus, SplashText, UNATTENDED_EXIT_DELAY,
};
use crate::{WINDOW_HEIGHT, WINDOW_WIDTH};
use eframe::egui::{self, Pos2, Rect, Vec2};
use std::sync::{Arc, Mutex};

pub struct SplashUpdateContext<'a> {
    pub ctx: &'a egui::Context,
    pub start_time: f64,
    pub text: SplashText,
    pub exit_start_time: &'a mut Option<f64>,
    pub background_since: &'a mut Option<f64>,
    pub quiet_exit: &'a mut bool,
    pub voxels: &'a mut [Voxel],
    pub clouds: &'a mut [Cloud],
    pub mouse_influence: &'a mut Vec2,
    pub mouse_world_pos: &'a mut Vec3,
    pub loading_text: &'a mut String,
    pub is_dark: &'a mut bool,
    pub audio: &'a Arc<Mutex<Option<SplashAudio>>>,
    pub has_played_impact: &'a mut bool,
}

/// Update the splash screen physics and state
pub fn update(update_ctx: SplashUpdateContext<'_>) -> SplashStatus {
    let SplashUpdateContext {
        ctx,
        start_time,
        text,
        exit_start_time,
        background_since,
        quiet_exit,
        voxels,
        clouds,
        mouse_influence,
        mouse_world_pos,
        loading_text,
        is_dark,
        audio,
        has_played_impact,
    } = update_ctx;
    *is_dark = ctx.global_style().visuals.dark_mode;

    let now = ctx.input(|i| i.time);
    let dt = ctx.input(|i| i.stable_dt);

    if exit_start_time.is_none() {
        let t = (now - start_time) as f32;
        if t > ANIMATION_DURATION - 0.5 && ctx.input(|i| i.pointer.any_click()) {
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

    let t_abs = (now - start_time) as f32;

    // SGT is often driven by hotkeys alone, so an assembled splash can sit behind
    // other windows indefinitely. Leave quietly once it has been unwatched long enough.
    // A focused splash nobody is touching (an unattended login launch) leaves too,
    // so it never keeps a core, the GPU and the audio device busy indefinitely.
    let backgrounded = background_exit_due(ctx.input(|i| i.focused), now, background_since);
    let unattended = unattended_exit_due(ctx.input(|i| i.pointer.time_since_last_movement()));
    if exit_start_time.is_none() && t_abs >= ANIMATION_DURATION && (backgrounded || unattended) {
        *exit_start_time = Some(now);
        *quiet_exit = true;
    }
    let physics_t = t_abs.min(ANIMATION_DURATION);

    // --- EXIT LOGIC ---
    let mut warp_progress = 0.0;
    if let Some(exit_start) = *exit_start_time {
        let dt = (now - exit_start) as f32;
        if dt > EXIT_DURATION {
            if let Ok(mut lock) = audio.lock()
                && let Some(audio) = lock.as_mut()
                && let Ok(mut s) = audio.state.lock()
            {
                s.is_finished = true;
            }
            return SplashStatus::Finished;
        }
        warp_progress = (dt / EXIT_DURATION).clamp(0.0, 1.0);
    }

    // --- UPDATE AUDIO STATE ---
    if let Ok(mut lock) = audio.lock()
        && let Some(audio) = lock.as_mut()
        && let Ok(mut s) = audio.state.lock()
    {
        s.physics_t = physics_t;
        // The warp whoosh is the only exit sound; a quiet exit never sends it.
        s.warp_progress = if *quiet_exit { 0.0 } else { warp_progress };
        s.is_dark = *is_dark;

        // Trigger impact once when assembly is nearly complete
        if physics_t > 1.6 && !*has_played_impact {
            s.impact_trigger = true;
            drop(s);
            *has_played_impact = true;
        }
    }

    // Once assembled the scene only drifts. Re-rendering the CPU framebuffer at the
    // display rate while waiting for a click would keep a core busy indefinitely.
    if exit_start_time.is_none() && t_abs >= ANIMATION_DURATION {
        ctx.request_repaint_after(IDLE_FRAME_INTERVAL);
    } else {
        ctx.request_repaint();
    }

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

        // Camera stays put on exit — the voxels drift outward themselves.
        let cam_dist = 600.0 + smoothstep(0.0, ANIMATION_DURATION, physics_t) * 100.0;

        let fov = 800.0;
        let mouse_wx = (pointer.x - center.x) * cam_dist / fov;
        let mouse_wy = -(pointer.y - center.y) * cam_dist / fov;
        *mouse_world_pos = Vec3::new(mouse_wx, mouse_wy, 0.0);
    }

    if exit_start_time.is_none() {
        let line = if t_abs < 0.8 {
            text.translating
        } else if t_abs < 1.6 {
            text.ocr
        } else if t_abs < 2.4 {
            text.transcribing
        } else {
            "nganlinh4"
        };
        line.clone_into(loading_text);
    } else {
        text.ready.clone_into(loading_text);
    }

    // --- PHYSICS UPDATE (Voxels) ---
    update_voxels(voxels, physics_t, t_abs, warp_progress, mouse_world_pos);

    SplashStatus::Ongoing
}

fn update_voxels(
    voxels: &mut [Voxel],
    physics_t: f32,
    t_abs: f32,
    warp_progress: f32,
    mouse_world_pos: &Vec3,
) {
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
                // Gentle "release": each voxel lets go at a slightly staggered
                // time and drifts outward + upward, EASED-OUT (quick release then
                // decelerating) instead of rushing the camera. Outer voxels still
                // cross the window edge onto the desktop overlay, but as a calm
                // dispersal rather than a hyperspace punch.
                let start_threshold = v.noise_factor * 0.35;
                let move_duration = 0.7;
                let local_linear =
                    ((warp_progress - start_threshold) / move_duration).clamp(0.0, 1.0);
                let local_eased = 1.0 - (1.0 - local_linear).powi(3);

                if local_eased > 0.0 {
                    // Outward from centre with a slight upward lift (+y is up on
                    // screen) and a faint swirl for life.
                    let mut dir = Vec3::new(v.pos.x, v.pos.y + 80.0, 0.0).normalize();
                    dir = dir.rotate_z(local_eased * (v.noise_factor - 0.5) * 1.2);
                    let drift = 760.0;
                    target_base = target_base.add(dir.mul(local_eased * drift));
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
}

/// Tracks how long the window has been unfocused and reports when the grace
/// period has run out. Refocusing restarts the period.
fn background_exit_due(focused: bool, now: f64, background_since: &mut Option<f64>) -> bool {
    if focused {
        *background_since = None;
        return false;
    }
    let since = *background_since.get_or_insert(now);
    now - since >= AUTO_EXIT_DELAY
}

fn unattended_exit_due(pointer_idle_seconds: f32) -> bool {
    f64::from(pointer_idle_seconds) >= UNATTENDED_EXIT_DELAY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_focused_splash_leaves_after_the_unattended_delay() {
        assert!(!unattended_exit_due(UNATTENDED_EXIT_DELAY as f32 - 1.0));
        assert!(unattended_exit_due(UNATTENDED_EXIT_DELAY as f32));
        const { assert!(UNATTENDED_EXIT_DELAY > AUTO_EXIT_DELAY) };
    }

    #[test]
    fn background_exit_waits_for_the_full_grace_period() {
        let mut since = None;
        let deadline = 100.0 + AUTO_EXIT_DELAY;
        assert!(!background_exit_due(false, 100.0, &mut since));
        assert!(!background_exit_due(false, deadline - 0.1, &mut since));
        assert!(background_exit_due(false, deadline, &mut since));
    }

    #[test]
    fn refocusing_restarts_the_grace_period() {
        let mut since = None;
        assert!(!background_exit_due(false, 100.0, &mut since));
        assert!(!background_exit_due(true, 105.0, &mut since));
        assert_eq!(since, None);

        let deadline = 106.0 + AUTO_EXIT_DELAY;
        assert!(!background_exit_due(false, 106.0, &mut since));
        assert!(!background_exit_due(false, deadline - 0.1, &mut since));
        assert!(background_exit_due(false, deadline, &mut since));
    }
}
