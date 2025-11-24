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

    // Helper to blend pixels (simple AA)
    let mut draw_pixel = |x: i32, y: i32, color: u32| {
        if x >= 0 && x < BROOM_W && y >= 0 && y < BROOM_H {
            pixels[(y * BROOM_W + x) as usize] = color;
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

    // ---------------------------------------------------------
    // 1. Draw Bristles (Bottom part)
    // ---------------------------------------------------------
    let bristle_len = 16.0 * params.squish;
    let top_w = 8.0;
    let bot_w = 16.0 + (1.0 - params.squish) * 10.0; // Spreads when squished
    
    // Increase density: 2 steps per logical pixel unit to close gaps
    let steps = (bristle_len * 2.0) as i32; 

    for i in 0..steps {
        let prog = i as f32 / steps as f32; // 0.0 to 1.0
        
        // INTERPOLATION:
        // Top of bristles (prog=0) must align with Handle (handle_rad) to prevent detachment.
        // Bottom of bristles (prog=1) swings fully to bristle_target_rad.
        // We use cubic interpolation (prog^3) to keep the neck stiff and tips loose.
        let current_angle = handle_rad + (bristle_target_rad - handle_rad) * (prog * prog * prog);
        
        let b_sin = current_angle.sin();
        let b_cos = current_angle.cos();

        let current_y_rel = prog * bristle_len;
        
        // Bend applies mostly at the tips. 
        // We clamp it slightly to prevent "yellow strings detached" look at high velocity.
        let bend_offset = params.bend * prog * prog * 8.0; 

        // Rotate the center line
        let cx = pivot_x - (current_y_rel * b_sin) + (bend_offset * b_cos);
        let cy = pivot_y + (current_y_rel * b_cos) + (bend_offset * b_sin);

        let current_w = top_w + (bot_w - top_w) * prog;
        
        // Add slight buffer (+0.5) to width to prevent aliasing gaps during rotation
        let half_w = (current_w / 2.0) + 0.5;

        let start_x = (cx - half_w).round() as i32;
        let end_x = (cx + half_w).round() as i32;
        let py = cy.round() as i32;

        for px in start_x..=end_x {
            // Texture Logic Update:
            // Calculate position relative to the center (cx) to make the "strings" follow the bend.
            // Using absolute screen coordinates (px, py) causes horizontal banding noise.
            // Using relative coordinates creates continuous vertical strands.
            let rel_x = (px as f32 - cx).round() as i32;
            
            // Map relative X to a seed. +20 ensures positive index logic.
            let seed = ((rel_x + 20) * 7) % 5;
            
            let col = match seed {
                0 => c_straw_sh,
                1 | 2 => c_straw_lt,
                _ => c_straw_dk
            };
            draw_pixel(px, py, col);
        }
    }

    // ---------------------------------------------------------
    // 2. Draw Band (Neck) - Rigidly attached to Handle
    // ---------------------------------------------------------
    let band_h = 3.0;
    for y_step in 0..band_h as i32 {
        let rel_y = -(y_step as f32); // Go up from pivot
        
        // Use Handle Math (h_sin, h_cos)
        let cx = pivot_x + (rel_y * h_sin);
        let cy = pivot_y - (rel_y * h_cos);
        
        let half_w = top_w / 2.0 + 1.5; // Slightly wider to cover bristle roots
        for px in (cx - half_w).round() as i32 ..= (cx + half_w).round() as i32 {
             draw_pixel(px, cy.round() as i32, c_band);
        }
    }

    // ---------------------------------------------------------
    // 3. Draw Handle - Rigid, less sensitive
    // ---------------------------------------------------------
    let handle_len = 20.0;
    
    for i in 0..handle_len as i32 {
        let rel_y = (i as f32) + band_h; 
        
        // Use Handle Math (h_sin, h_cos)
        let cx = pivot_x + (rel_y * h_sin);
        let cy = pivot_y - (rel_y * h_cos); // Upward on screen

        let px = cx.round() as i32;
        let py = cy.round() as i32;

        // Thickness 2
        draw_pixel(px, py, c_handle_dk);
        draw_pixel(px + 1, py, c_handle_lt);
    }

    pixels
}
