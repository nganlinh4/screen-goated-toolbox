pub const BROOM_W: i32 = 48; // Increased canvas size for rotation space
pub const BROOM_H: i32 = 48;

#[derive(Clone, Copy, Default)]
pub struct BroomRenderParams {
    pub tilt_angle: f32, // Degrees, negative = left, positive = right
    pub squish: f32,     // 1.0 = normal, 0.5 = smashed
    pub bend: f32,       // Curvature of bristles (drag effect)
    pub opacity: f32,    // 0.0 to 1.0
}

pub fn render_procedural_broom(params: BroomRenderParams) -> Vec<u32> {
    let mut pixels = vec![0u32; (BROOM_W * BROOM_H) as usize];

    // Palette
    let alpha = (params.opacity * 255.0) as u32;
    if alpha == 0 { return pixels; }

    let c_handle_dk = (alpha << 24) | 0x005D4037;
    let c_handle_lt = (alpha << 24) | 0x008D6E63;
    let c_band      = (alpha << 24) | 0x00B71C1C;
    let c_straw_dk  = (alpha << 24) | 0x00FBC02D;
    let c_straw_lt  = (alpha << 24) | 0x00FFF176;
    let c_straw_sh  = (alpha << 24) | 0x00F57F17;
    
    // Shadow color (Black with 30% opacity)
    let shadow_alpha = (alpha as f32 * 0.3) as u32;
    let c_shadow = (shadow_alpha << 24) | 0x00000000;

    // Helper to draw pixels
    let mut draw_pixel = |x: i32, y: i32, color: u32, is_shadow: bool| {
        if x >= 0 && x < BROOM_W && y >= 0 && y < BROOM_H {
            let idx = (y * BROOM_W + x) as usize;
            if is_shadow {
                // Only write shadow if pixel is empty
                if pixels[idx] == 0 {
                    pixels[idx] = color;
                }
            } else {
                pixels[idx] = color;
            }
        }
    };

    // Center of the broom's "neck" (pivot point)
    let pivot_x = (BROOM_W / 2) as f32;
    let pivot_y = (BROOM_H as f32) * 0.65; // Lower pivot to allow handle swing

    // --- PHYSICS SEPARATION ---
    // 1. Handle Angle: Dampened (0.25x) to be less sensitive/jittery
    let handle_rad = (params.tilt_angle * 0.25).to_radians();
    let h_sin = handle_rad.sin();
    let h_cos = handle_rad.cos();

    // 2. Bristle Angle: Uses half tilt for "swishy" effect, blended later
    let bristle_target_rad = (params.tilt_angle * 0.5).to_radians();

    let bristle_len = 16.0 * params.squish;
    let top_w = 8.0;
    let bot_w = 16.0 + (1.0 - params.squish) * 10.0;
    let steps = (bristle_len * 2.0) as i32; 

    // Shadow offset
    let sx = 2.0; 
    let sy = 2.0;

    for pass in 0..2 {
        let is_shadow = pass == 0;
        let offset_x = if is_shadow { sx } else { 0.0 };
        let offset_y = if is_shadow { sy } else { 0.0 };

        // ---------------------------------------------------------
        // Draw Bristles
        // ---------------------------------------------------------
        for i in 0..steps {
            let prog = i as f32 / steps as f32;
            let current_angle = handle_rad + (bristle_target_rad - handle_rad) * (prog * prog * prog);
            let b_sin = current_angle.sin();
            let b_cos = current_angle.cos();
            let current_y_rel = prog * bristle_len;
            let bend_offset = params.bend * prog * prog * 8.0; 

            let cx = pivot_x - (current_y_rel * b_sin) + (bend_offset * b_cos) + offset_x;
            let cy = pivot_y + (current_y_rel * b_cos) + (bend_offset * b_sin) + offset_y;

            let current_w = top_w + (bot_w - top_w) * prog;
            let half_w = (current_w / 2.0) + 0.5;

            let start_x = (cx - half_w).round() as i32;
            let end_x = (cx + half_w).round() as i32;
            let py = cy.round() as i32;

            for px in start_x..=end_x {
                if is_shadow {
                    draw_pixel(px, py, c_shadow, true);
                } else {
                    let rel_x = (px as f32 - cx).round() as i32;
                    let seed = ((rel_x + 20) * 7) % 5;
                    let col = match seed {
                        0 => c_straw_sh,
                        1 | 2 => c_straw_lt,
                        _ => c_straw_dk
                    };
                    draw_pixel(px, py, col, false);
                }
            }
        }
        
        // ---------------------------------------------------------
        // Draw Band (Neck) - Rigidly attached to Handle
        // ---------------------------------------------------------
        if !is_shadow {
            let band_h = 3.0;
            for y_step in 0..band_h as i32 {
                let rel_y = -(y_step as f32);
                let cx = pivot_x + (rel_y * h_sin);
                let cy = pivot_y - (rel_y * h_cos);
                let half_w = top_w / 2.0 + 1.5;
                for px in (cx - half_w).round() as i32 ..= (cx + half_w).round() as i32 {
                     draw_pixel(px, cy.round() as i32, c_band, false);
                }
            }

            // ---------------------------------------------------------
            // Draw Handle - Rigid, less sensitive
            // ---------------------------------------------------------
            let handle_len = 20.0;
            for i in 0..handle_len as i32 {
                let rel_y = (i as f32) + 3.0; 
                let cx = pivot_x + (rel_y * h_sin);
                let cy = pivot_y - (rel_y * h_cos);
                let px = cx.round() as i32;
                let py = cy.round() as i32;
                draw_pixel(px, py, c_handle_dk, false);
                draw_pixel(px + 1, py, c_handle_lt, false);
            }
        }
    }

    pixels
}
