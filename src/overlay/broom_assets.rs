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

    let rad = params.tilt_angle.to_radians();
    let sin_a = rad.sin();
    let cos_a = rad.cos();

    // 1. Draw Bristles (Bottom part)
    // The bristles hang down from the pivot, affected by 'squish' and 'bend'
    let bristle_len = 16.0 * params.squish;
    let top_w = 8.0;
    let bot_w = 16.0 + (1.0 - params.squish) * 10.0; // Spreads when squished

    for y_step in 0..bristle_len as i32 {
        let prog = y_step as f32 / bristle_len;
        
        // Calculate the center line of the bristles
        // We apply rotation + the 'bend' factor (lagging behind movement)
        let current_y_rel = y_step as f32;
        let bend_offset = params.bend * prog * prog * 10.0; // Quadratic bend

        // Rotate the center line
        let cx = pivot_x - (current_y_rel * sin_a) + (bend_offset * cos_a);
        let cy = pivot_y + (current_y_rel * cos_a) + (bend_offset * sin_a);

        let current_w = top_w + (bot_w - top_w) * prog;
        let half_w = current_w / 2.0;

        let start_x = (cx - half_w).round() as i32;
        let end_x = (cx + half_w).round() as i32;
        let py = cy.round() as i32;

        for px in start_x..=end_x {
            // Texture pattern
            let seed = (px * 7 + y_step * 13) % 5;
            let col = match seed {
                0 => c_straw_sh,
                1 | 2 => c_straw_lt,
                _ => c_straw_dk
            };
            draw_pixel(px, py, col);
        }
    }

    // 2. Draw Band (Neck)
    let band_h = 3.0;
    for y_step in 0..band_h as i32 {
        let rel_y = -(y_step as f32); // Go up from pivot
        let cx = pivot_x + (rel_y * sin_a);
        let cy = pivot_y - (rel_y * cos_a);
        
        let half_w = top_w / 2.0 + 1.0; // Slightly wider
        for px in (cx - half_w).round() as i32 ..= (cx + half_w).round() as i32 {
             draw_pixel(px, cy.round() as i32, c_band);
        }
    }

    // 3. Draw Handle (pointing UPWARD from the pivot)
    let handle_len = 20.0;
    
    for i in 0..handle_len as i32 {
        // rel_y is POSITIVE going upward from pivot
        let rel_y = (i as f32) + band_h; 
        
        // Handle is rigid, follows tilt exactly
        // When tilted right (positive angle), handle top goes right
        // When tilted left (negative angle), handle top goes left
        let cx = pivot_x + (rel_y * sin_a);
        let cy = pivot_y - (rel_y * cos_a); // Negative because Y increases downward in screen coords

        let px = cx.round() as i32;
        let py = cy.round() as i32;

        // Thickness 2
        draw_pixel(px, py, c_handle_dk);
        draw_pixel(px + 1, py, c_handle_lt);
    }

    pixels
}
