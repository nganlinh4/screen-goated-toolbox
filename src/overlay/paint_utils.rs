use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use std::mem::size_of;

const CORNER_RADIUS: f32 = 12.0;

#[inline(always)]
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> u32 {
    let c = v * s;
    let h_prime = (h % 360.0) / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h_prime < 1.0 { (c, x, 0.0) }
    else if h_prime < 2.0 { (x, c, 0.0) }
    else if h_prime < 3.0 { (0.0, c, x) }
    else if h_prime < 4.0 { (0.0, x, c) }
    else if h_prime < 5.0 { (x, 0.0, c) }
    else { (c, 0.0, x) };

    let r_u = ((r + m) * 255.0) as u32;
    let g_u = ((g + m) * 255.0) as u32;
    let b_u = ((b + m) * 255.0) as u32;

    (r_u << 16) | (g_u << 8) | b_u 
}

#[inline(always)]
pub fn sd_rounded_box(px: f32, py: f32, bx: f32, by: f32, r: f32) -> f32 {
    let qx = px.abs() - bx + r;
    let qy = py.abs() - by + r;
    let len_max_q = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    let min_max_q = qx.max(qy).min(0.0);
    len_max_q + min_max_q - r
}

pub unsafe fn render_box_sdf(hdc_dest: HDC, bounds: RECT, w: i32, h: i32, is_glowing: bool, time_offset: f32) {
    let min_dim = w.min(h) as f32;
    let perimeter = 2.0 * (w + h) as f32;
    
    let dynamic_base_scale = if is_glowing {
        (min_dim * 0.2).clamp(30.0, 180.0)
    } else {
        2.0
    };

    let max_possible_reach = if is_glowing { dynamic_base_scale * 1.7 } else { 3.0 };
    let pad = max_possible_reach.ceil() as i32 + 4; 
    
    let buf_w = w + (pad * 2);
    let buf_h = h + (pad * 2);
    
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: buf_w,
            biHeight: -buf_h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut p_bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let hbm = CreateDIBSection(hdc_dest, &bmi, DIB_RGB_COLORS, &mut p_bits, None, 0).unwrap();
    
    if !p_bits.is_null() {
        let pixels = std::slice::from_raw_parts_mut(p_bits as *mut u32, (buf_w * buf_h) as usize);
        
        let bx = (w as f32) / 2.0;
        let by = (h as f32) / 2.0;
        let center_x = (pad as f32) + bx;
        let center_y = (pad as f32) + by;

        let eff_radius = CORNER_RADIUS.min(bx).min(by);
        let time_rad = time_offset.to_radians();
        let complexity_scale = 1.0 + (perimeter / 1800.0); 
        let freq1 = (2.0 * complexity_scale).round();
        let freq2 = (5.0 * complexity_scale).round();
        let time_mult = 1.0;

        // Optimization: Inner Skip Logic
        let safe_skip_dist = max_possible_reach + eff_radius + 2.0;
        let skip_x_min = (center_x - bx + safe_skip_dist).ceil() as i32;
        let skip_x_max = (center_x + bx - safe_skip_dist).floor() as i32;
        let skip_y_min = (center_y - by + safe_skip_dist).ceil() as i32;
        let skip_y_max = (center_y + by - safe_skip_dist).floor() as i32;
        
        let do_skip = skip_x_max > skip_x_min && skip_y_max > skip_y_min;

        let mut render_pixel = |x: i32, y: i32, idx: usize| {
             let px = (x as f32) - center_x;
             let py = (y as f32) - center_y;
             
             let d = sd_rounded_box(px, py, bx, by, eff_radius);
             
             let mut final_col = 0u32;
             let mut final_alpha = 0.0f32;

             if is_glowing {
                 if d > 0.0 {
                     let aa = (1.5 - d).clamp(0.0, 1.0);
                     if aa > 0.0 {
                         final_alpha = aa;
                         final_col = 0x00FFFFFF; 
                     }
                 } else {
                     let angle = py.atan2(px);
                     let noise = (angle * freq1 + time_rad * 2.0 * time_mult).sin() * 0.5 
                               + (angle * freq2 - time_rad * 3.0 * time_mult).sin() * 0.4;
                     
                     let local_glow_width = dynamic_base_scale + (noise * (dynamic_base_scale * 0.65));
                     let dist_in = d.abs();
                     
                     let t = (dist_in / local_glow_width).clamp(0.0, 1.0);
                     let intensity = (1.0 - t).powi(3); 
                     
                     final_alpha = intensity;
                     if dist_in < 4.0 { final_alpha = 1.0; }

                     if final_alpha > 0.005 {
                         let deg = angle.to_degrees() + 180.0;
                         let hue = (deg + time_offset) % 360.0;
                         let rgb = hsv_to_rgb(hue, 0.8, 1.0);
                         if dist_in < 2.0 { final_col = 0x00FFFFFF; } else { final_col = rgb; }
                     }
                 }
             } else {
                 let border_width = 3.0;
                 let alpha_outer = (1.0 - d).clamp(0.0, 1.0);
                 let alpha_inner = (d + border_width + 1.0).clamp(0.0, 1.0);
                 final_alpha = alpha_outer * alpha_inner;
                 final_col = 0x00CCCCCC; 
             }

             if final_alpha > 0.0 {
                 let r = ((final_col >> 16) & 0xFF) as f32;
                 let g = ((final_col >> 8) & 0xFF) as f32;
                 let b = (final_col & 0xFF) as f32;
                 pixels[idx] = (((r * final_alpha) as u32) << 16) | (((g * final_alpha) as u32) << 8) | ((b * final_alpha) as u32);
             } else {
                 pixels[idx] = 0;
             }
        };

        for y in 0..buf_h {
            let in_vertical_skip = do_skip && y > skip_y_min && y < skip_y_max;
            if in_vertical_skip {
                for x in 0..skip_x_min { render_pixel(x, y, (y * buf_w + x) as usize); }
                for x in skip_x_max..buf_w { render_pixel(x, y, (y * buf_w + x) as usize); }
            } else {
                for x in 0..buf_w { render_pixel(x, y, (y * buf_w + x) as usize); }
            }
        }
        
        let mem_dc = CreateCompatibleDC(hdc_dest);
        let old_bmp = SelectObject(mem_dc, hbm);
        let _ = BitBlt(hdc_dest, bounds.left - pad, bounds.top - pad, buf_w, buf_h, mem_dc, 0, 0, SRCPAINT);
        SelectObject(mem_dc, old_bmp);
        DeleteDC(mem_dc);
    }
    DeleteObject(hbm);
}
