fn buffer_corner_radius(w: i32, h: i32, physical_to_buffer_scale: f32) -> f32 {
    (crate::overlay::BOX_CORNER_RADIUS_PHYSICAL_PX * physical_to_buffer_scale)
        .min(w as f32 / 2.0)
        .min(h as f32 / 2.0)
}

#[inline(always)]
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> u32 {
    let c = v * s;
    let h_prime = (h % 360.0) / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    let r_u = ((r + m) * 255.0) as u32;
    let g_u = ((g + m) * 255.0) as u32;
    let b_u = ((b + m) * 255.0) as u32;

    (r_u << 16) | (g_u << 8) | b_u
}

// OPTIMIZED DRAWING: Writes directly to cached buffer
// Defers expensive atan2 until after visibility check
pub unsafe fn draw_direct_sdf_glow(
    pixels_ptr: *mut u32,
    w: i32,
    h: i32,
    physical_to_buffer_scale: f32,
    time_offset: f32,
    alpha_mult: f32,
    is_glowing: bool,
) {
    unsafe {
        if pixels_ptr.is_null() {
            return;
        }

        let pixels = std::slice::from_raw_parts_mut(pixels_ptr, (w * h) as usize);
        let bx = (w as f32) / 2.0;
        let by = (h as f32) / 2.0;
        let center_x = bx;
        let center_y = by;
        let eff_radius = buffer_corner_radius(w, h, physical_to_buffer_scale);

        // ADAPTIVE GLOW SCALING: Scale based on window size to keep center hollow
        // For small windows (e.g., 100px): scales to ~20px glow
        // For large windows (e.g., 600px+): scales to 60px glow
        let min_dim = (w as f32).min(h as f32);
        let dynamic_base_scale = (min_dim * 0.2).clamp(20.0, 60.0);

        // Optimization: Skip the middle of the box to save CPU
        // Increase margin slightly to ensure we don't process internal pixels
        let safe_margin = 85.0;
        let skip_min_x = center_x - bx + safe_margin;
        let skip_max_x = center_x + bx - safe_margin;
        let skip_min_y = center_y - by + safe_margin;
        let skip_max_y = center_y + by - safe_margin;

        // Pre-calculate constants outside loop
        let time_rad = time_offset.to_radians();
        let glow_threshold = -dynamic_base_scale * 1.5;

        for y in 0..h {
            let py = (y as f32) - center_y;
            let fy = y as f32;
            let is_y_safe = fy > skip_min_y && fy < skip_max_y;

            for x in 0..w {
                let idx = (y * w + x) as usize;

                // Fast Path: Clear middle pixels and skip math entirely
                if is_y_safe && (x as f32) > skip_min_x && (x as f32) < skip_max_x {
                    pixels[idx] = 0;
                    continue;
                }

                let px = (x as f32) - center_x;

                // Standard SDF Logic
                let qx = px.abs() - bx + eff_radius;
                let qy = py.abs() - by + eff_radius;
                let d = if qx > 0.0 && qy > 0.0 {
                    ((qx * qx + qy * qy).sqrt()) - eff_radius
                } else {
                    qx.max(qy) - eff_radius
                };

                if d > 0.0 {
                    // Border Logic (Outer edge)
                    // Use explicit bounds to avoid expensive clamp if outside
                    if d > 2.0 {
                        pixels[idx] = 0;
                        continue;
                    }

                    let t = (d / 2.0).clamp(0.0, 1.0);
                    let aa = 1.0 - t * t * (3.0 - 2.0 * t);

                    if aa > 0.0 {
                        let a = (aa * 255.0 * alpha_mult) as u32;
                        pixels[idx] = (a << 24) | (a << 16) | (a << 8) | a;
                    } else {
                        pixels[idx] = 0;
                    }
                } else {
                    // Inner Glow Logic
                    if !is_glowing || d < glow_threshold {
                        pixels[idx] = 0;
                    } else {
                        let dist_in = d.abs();

                        // OPTIMIZATION: Lazy Math
                        // Only calculate atan2/sin/cos (Expensive!) if pixel is actually visible.
                        // Use rough intensity check first to skip pixels that are too faint.

                        let t_rough = (dist_in / (dynamic_base_scale * 1.4)).clamp(0.0, 1.0);
                        let base_intensity_rough = (1.0 - t_rough).powi(3);

                        if base_intensity_rough < 0.005 {
                            pixels[idx] = 0;
                            continue;
                        }

                        // Now perform expensive math only for visible pixels
                        let angle = py.atan2(px);
                        let noise = (angle * 4.0 + time_rad * 2.0).sin() * 0.5;
                        let local_glow_width =
                            dynamic_base_scale + (noise * (dynamic_base_scale * 0.4));

                        let t = (dist_in / local_glow_width).clamp(0.0, 1.0);
                        let intensity = (1.0 - t).powi(3);
                        let final_alpha = if dist_in < 3.0 { 1.0 } else { intensity };

                        if final_alpha > 0.005 {
                            let deg = angle.to_degrees() + 180.0;
                            let hue = (deg + time_offset) % 360.0;

                            let rgb = if dist_in < 2.5 {
                                0x00FFFFFF
                            } else {
                                hsv_to_rgb(hue, 0.8, 1.0)
                            };

                            let a = (final_alpha * 255.0 * alpha_mult) as u32;
                            let r = ((rgb >> 16) & 0xFF) * a / 255;
                            let g = ((rgb >> 8) & 0xFF) * a / 255;
                            let b = (rgb & 0xFF) * a / 255;

                            pixels[idx] = (a << 24) | (r << 16) | (g << 8) | b;
                        } else {
                            pixels[idx] = 0;
                        }
                    }
                }
            }
        }
    }
}

// === MINIMAL GRAPHICS MODE ===
// Super lightweight rendering for weak computers.
// Only draws: white border + bouncing green scan line.
// NO per-pixel SDF calculations, NO trigonometry, NO expensive math.
// This is inspired by the old working version that never crashed.
pub unsafe fn draw_minimal_glow(
    pixels_ptr: *mut u32,
    w: i32,
    h: i32,
    physical_to_buffer_scale: f32,
    time_offset: f32,
    _alpha_mult: f32,
    is_glowing: bool,
) {
    unsafe {
        if pixels_ptr.is_null() {
            return;
        }

        let pixels = std::slice::from_raw_parts_mut(pixels_ptr, (w * h) as usize);

        // Clear all pixels first (transparent)
        for pixel in pixels.iter_mut() {
            *pixel = 0;
        }

        let radius = buffer_corner_radius(w, h, physical_to_buffer_scale);

        // Draw a one-pixel rounded border. This remains row-based so minimal
        // mode does not pay for a full-buffer SDF pass.
        let white: u32 = 0xFFFFFFFF; // ARGB: fully opaque white
        for y in 0..h {
            let inset = rounded_row_inset(y, h, radius).min((w - 1) / 2);
            let left = inset;
            let right = w - 1 - inset;
            pixels[(y * w + left) as usize] = white;
            pixels[(y * w + right) as usize] = white;
            if y == 0 || y == h - 1 {
                pixels[(y * w + left) as usize..=(y * w + right) as usize].fill(white);
            }
        }

        // Draw bouncing green scan line if glowing (processing)
        if is_glowing {
            // Use time_offset to calculate scan line position
            // The scan line bounces up and down between 2px from edges
            let cycle = (time_offset % 360.0) / 180.0; // 0.0 to 2.0
            let t = if cycle <= 1.0 { cycle } else { 2.0 - cycle }; // 0.0 to 1.0 (bounce)

            let margin = 3;
            let scan_range = h - (margin * 2);
            if scan_range > 0 {
                let scan_y = margin + ((t * scan_range as f32) as i32).clamp(0, scan_range - 1);

                // Draw 2px thick green line
                let green: u32 = 0xFF00FF00; // ARGB: fully opaque green
                for line_offset in 0..2 {
                    let y = scan_y + line_offset;
                    if y > 0 && y < h - 1 {
                        let inset = rounded_row_inset(y, h, radius);
                        let start = margin.max(inset);
                        let end = (w - margin).min(w - inset);
                        for x in start..end {
                            pixels[(y * w + x) as usize] = green;
                        }
                    }
                }
            }
        }
    }
}

fn rounded_row_inset(y: i32, height: i32, radius: f32) -> i32 {
    let edge_distance = y.min(height - 1 - y) as f32 + 0.5;
    if edge_distance >= radius {
        return 0;
    }
    let center_distance = radius - edge_distance;
    let horizontal = (radius * radius - center_distance * center_distance)
        .max(0.0)
        .sqrt();
    (radius - horizontal).ceil() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glow_buffer_compensates_for_downsampling() {
        for scale in [1.0, 0.75, 0.5, 1.0 / 3.0] {
            let buffer_radius = buffer_corner_radius(640, 360, scale);
            assert!((buffer_radius / scale - 8.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn minimal_border_uses_rounded_corner_insets() {
        assert!(rounded_row_inset(0, 100, 8.0) > 0);
        assert_eq!(rounded_row_inset(8, 100, 8.0), 0);
    }
}
