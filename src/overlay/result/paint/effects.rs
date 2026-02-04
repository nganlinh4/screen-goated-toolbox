// --- PAINT EFFECTS ---
// Refinement glow and particle rendering for result overlay.

use crate::overlay::paint_utils::{hsv_to_rgb, sd_rounded_box};

// --- REFINEMENT GLOW ---

pub fn render_refinement_glow(
    raw_pixels: &mut [u32],
    width: i32,
    height: i32,
    anim_offset: f32,
    graphics_mode: &str,
) {
    let is_minimal = graphics_mode == "minimal";

    if is_minimal {
        // MINIMAL MODE: Bouncing orange scan line
        let cycle = (anim_offset.abs() % 360.0) / 180.0;
        let t = if cycle <= 1.0 { cycle } else { 2.0 - cycle };

        let margin = 3;
        let scan_range = height - (margin * 2);
        if scan_range > 0 {
            let scan_y = margin + ((t * scan_range as f32) as i32).clamp(0, scan_range - 1);

            // Draw 2px thick orange line
            for line_offset in 0..2 {
                let y = scan_y + line_offset;
                if y > 0 && y < height - 1 {
                    for x in margin..(width - margin) {
                        let idx = (y * width + x) as usize;
                        if idx < raw_pixels.len() {
                            let bg_px = raw_pixels[idx];
                            let bg_b = (bg_px & 0xFF) as f32;
                            let bg_g = ((bg_px >> 8) & 0xFF) as f32;
                            let bg_r = ((bg_px >> 16) & 0xFF) as f32;

                            let intensity = 0.9;
                            let out_r = (255.0 * intensity + bg_r * (1.0 - intensity)) as u32;
                            let out_g = (140.0 * intensity + bg_g * (1.0 - intensity)) as u32;
                            let out_b = (0.0 * intensity + bg_b * (1.0 - intensity)) as u32;
                            raw_pixels[idx] = (255 << 24) | (out_r << 16) | (out_g << 8) | out_b;
                        }
                    }
                }
            }
        }
    } else {
        // STANDARD MODE: Rainbow edge glow
        let bx = width as f32 / 2.0;
        let by = height as f32 / 2.0;
        let center_x = bx;
        let center_y = by;
        let time_rad = anim_offset.to_radians();

        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let px = x as f32 - center_x;
                let py = y as f32 - center_y;
                let d = sd_rounded_box(px, py, bx, by, 8.0);

                if d <= 0.0 {
                    let dist = d.abs();
                    if dist < 20.0 {
                        let angle = py.atan2(px);
                        let noise = (angle * 12.0 - time_rad * 2.0).sin() * 0.5;
                        let glow_width = 14.0;
                        let t = (dist / glow_width).clamp(0.0, 1.0);
                        let base_intensity = (1.0 - t).powi(3);

                        if base_intensity > 0.01 {
                            let noise_mod = (1.0 + noise * 0.3).clamp(0.0, 2.0);
                            let final_intensity = (base_intensity * noise_mod).clamp(0.0, 1.0);
                            if final_intensity > 0.01 {
                                let deg = angle.to_degrees() + (anim_offset * 2.0);
                                let hue = (deg % 360.0 + 360.0) % 360.0;
                                let rgb = hsv_to_rgb(hue, 0.85, 1.0);
                                let bg_px = raw_pixels[idx];
                                let bg_b = (bg_px & 0xFF) as f32;
                                let bg_g = ((bg_px >> 8) & 0xFF) as f32;
                                let bg_r = ((bg_px >> 16) & 0xFF) as f32;
                                let fg_r = ((rgb >> 16) & 0xFF) as f32;
                                let fg_g = ((rgb >> 8) & 0xFF) as f32;
                                let fg_b = (rgb & 0xFF) as f32;

                                let out_r =
                                    (fg_r * final_intensity + bg_r * (1.0 - final_intensity)) as u32;
                                let out_g =
                                    (fg_g * final_intensity + bg_g * (1.0 - final_intensity)) as u32;
                                let out_b =
                                    (fg_b * final_intensity + bg_b * (1.0 - final_intensity)) as u32;
                                raw_pixels[idx] =
                                    (255 << 24) | (out_r << 16) | (out_g << 8) | out_b;
                            }
                        }
                    }
                }
            }
        }
    }
}

// --- PARTICLE RENDERING ---

pub fn render_particles(
    raw_pixels: &mut [u32],
    width: i32,
    height: i32,
    particles: &[(f32, f32, f32, f32, u32)],
) {
    for &(d_x, d_y, life, size, col) in particles {
        if life <= 0.0 {
            continue;
        }
        let radius = size * life;
        if radius < 0.5 {
            continue;
        }

        let p_r = ((col >> 16) & 0xFF) as f32;
        let p_g = ((col >> 8) & 0xFF) as f32;
        let p_b = (col & 0xFF) as f32;
        let p_max_alpha = 255.0 * life;

        let min_x = (d_x - radius - 1.0).floor() as i32;
        let max_x = (d_x + radius + 1.0).ceil() as i32;
        let min_y = (d_y - radius - 1.0).floor() as i32;
        let max_y = (d_y + radius + 1.0).ceil() as i32;

        let start_x = min_x.max(0);
        let end_x = max_x.min(width - 1);
        let start_y = min_y.max(0);
        let end_y = max_y.min(height - 1);

        for y in start_y..=end_y {
            for x in start_x..=end_x {
                let dx = x as f32 - d_x;
                let dy = y as f32 - d_y;
                let dist = (dx * dx + dy * dy).sqrt();
                let aa_edge = (radius + 0.5 - dist).clamp(0.0, 1.0);

                if aa_edge > 0.0 {
                    let idx = (y * width + x) as usize;
                    let bg_px = raw_pixels[idx];
                    let bg_b = (bg_px & 0xFF) as f32;
                    let bg_g = ((bg_px >> 8) & 0xFF) as f32;
                    let bg_r = ((bg_px >> 16) & 0xFF) as f32;

                    let final_alpha_norm = (p_max_alpha * aa_edge) / 255.0;
                    let inv_alpha = 1.0 - final_alpha_norm;

                    let out_r = (p_r * final_alpha_norm + bg_r * inv_alpha) as u32;
                    let out_g = (p_g * final_alpha_norm + bg_g * inv_alpha) as u32;
                    let out_b = (p_b * final_alpha_norm + bg_b * inv_alpha) as u32;

                    raw_pixels[idx] = (255 << 24) | (out_r << 16) | (out_g << 8) | out_b;
                }
            }
        }
    }
}
