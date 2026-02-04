// --- SELECTION RENDER ---
// Rendering and bitmap extraction for selection overlay.

use super::state::*;
use crate::win_types::SendHbitmap;
use crate::GdiCapture;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// --- BITMAP EXTRACTION ---

pub unsafe fn extract_crop_from_hbitmap(
    capture: &GdiCapture,
    crop_rect: RECT,
) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    extract_crop_from_hbitmap_internal(capture, crop_rect)
}

/// Public version of extract_crop_from_hbitmap for use by image_continuous_mode
pub fn extract_crop_from_hbitmap_public(
    capture: &GdiCapture,
    crop_rect: RECT,
) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    unsafe { extract_crop_from_hbitmap_internal(capture, crop_rect) }
}

unsafe fn extract_crop_from_hbitmap_internal(
    capture: &GdiCapture,
    crop_rect: RECT,
) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    let hdc_screen = GetDC(None);
    let hdc_mem = CreateCompatibleDC(Some(hdc_screen));

    // Select the big screenshot into DC
    let old_obj = SelectObject(hdc_mem, capture.hbitmap.into());

    let w = (crop_rect.right - crop_rect.left).abs();
    let h = (crop_rect.bottom - crop_rect.top).abs();

    // Create a BMI for just the cropped area
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // Top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut buffer: Vec<u8> = vec![0; (w * h * 4) as usize];

    // Create small temp bitmap, blit crop to it, read bits
    let hdc_temp = CreateCompatibleDC(Some(hdc_screen));
    let hbm_temp = CreateCompatibleBitmap(hdc_screen, w, h);
    SelectObject(hdc_temp, hbm_temp.into());

    // Copy only the crop region from the huge screenshot
    // IMPORTANT: virtual screen coordinates calculation
    let v_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let v_y = GetSystemMetrics(SM_YVIRTUALSCREEN);

    // source x/y in the bitmap
    let src_x = crop_rect.left - v_x;
    let src_y = crop_rect.top - v_y;

    let _ = BitBlt(hdc_temp, 0, 0, w, h, Some(hdc_mem), src_x, src_y, SRCCOPY).ok();

    // Now read pixels from small bitmap
    GetDIBits(
        hdc_temp,
        hbm_temp,
        0,
        h as u32,
        Some(buffer.as_mut_ptr() as *mut _),
        &mut bmi,
        DIB_RGB_COLORS,
    );

    // BGR -> RGB correction
    for chunk in buffer.chunks_exact_mut(4) {
        chunk.swap(0, 2);
        chunk[3] = 255;
    }

    let _ = DeleteObject(hbm_temp.into());
    let _ = DeleteDC(hdc_temp);

    // Cleanup main DC
    SelectObject(hdc_mem, old_obj);
    let _ = DeleteDC(hdc_mem);
    ReleaseDC(None, hdc_screen);

    image::ImageBuffer::from_raw(w as u32, h as u32, buffer).unwrap()
}

// --- LAYERED WINDOW RENDERING ---

/// High-performance renderer using UpdateLayeredWindow
/// This allows an OPAQUE white box even when the dim background is TRANSPARENT
#[allow(static_mut_refs)]
pub unsafe fn sync_layered_window_contents(hwnd: HWND) {
    let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

    if width <= 0 || height <= 0 {
        return;
    }

    // 1. Prepare/Cache 32-bit DIB Context
    if std::ptr::addr_of!(CACHED_BITMAP).read().is_invalid()
        || CACHED_W != width
        || CACHED_H != height
    {
        if !std::ptr::addr_of!(CACHED_BITMAP).read().is_invalid() {
            let _ = DeleteObject(CACHED_BITMAP.0.into());
            CACHED_BITS = std::ptr::null_mut();
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // Top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let hdc_screen = GetDC(None);
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbm = CreateDIBSection(Some(hdc_screen), &bmi, DIB_RGB_COLORS, &mut bits, None, 0);
        ReleaseDC(None, hdc_screen);

        if let Ok(h) = hbm {
            CACHED_BITMAP = SendHbitmap(h);
            CACHED_BITS = bits as *mut u8;
            CACHED_W = width;
            CACHED_H = height;
        } else {
            return;
        }
    }

    // 2. Draw using GDI to the DIB
    let hdc_screen = GetDC(None);
    let mem_dc = CreateCompatibleDC(Some(hdc_screen));
    let old_bmp = SelectObject(mem_dc, CACHED_BITMAP.0.into());

    // OPTIMIZATION: Clear background directly via memory fill
    let effective_alpha = if let Some(zoom_alpha) = ZOOM_ALPHA_OVERRIDE {
        zoom_alpha.min(CURRENT_ALPHA)
    } else {
        CURRENT_ALPHA
    };

    let total_pixels = (width * height) as usize;
    let pixels_u32 = std::slice::from_raw_parts_mut(CACHED_BITS as *mut u32, total_pixels);

    // Fill with pre-multiplied alpha black: (0, 0, 0, alpha)
    let bg_val = (effective_alpha as u32) << 24;
    pixels_u32.fill(bg_val);

    // Draw the selection rectangle
    if IS_DRAGGING {
        let rect_abs = RECT {
            left: START_POS.x.min(CURR_POS.x),
            top: START_POS.y.min(CURR_POS.y),
            right: START_POS.x.max(CURR_POS.x),
            bottom: START_POS.y.max(CURR_POS.y),
        };

        let screen_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let screen_y = GetSystemMetrics(SM_YVIRTUALSCREEN);

        let r = RECT {
            left: rect_abs.left - screen_x,
            top: rect_abs.top - screen_y,
            right: rect_abs.right - screen_x,
            bottom: rect_abs.bottom - screen_y,
        };

        let w = (r.right - r.left).abs();
        let h = (r.bottom - r.top).abs();

        if w > 0 && h > 0 {
            draw_rounded_selection_box(pixels_u32, width, height, &r, effective_alpha);
        }
    }

    // 4. Update the Layered Window
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    let screen_pos = POINT {
        x: GetSystemMetrics(SM_XVIRTUALSCREEN),
        y: GetSystemMetrics(SM_YVIRTUALSCREEN),
    };
    let wnd_size = SIZE {
        cx: width,
        cy: height,
    };
    let src_pos = POINT { x: 0, y: 0 };

    let _ = UpdateLayeredWindow(
        hwnd,
        Some(hdc_screen),
        Some(&screen_pos),
        Some(&wnd_size),
        Some(mem_dc),
        Some(&src_pos),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );

    // Cleanup DC
    SelectObject(mem_dc, old_bmp);
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, hdc_screen);
}

/// Draw anti-aliased rounded selection box using SDF
fn draw_rounded_selection_box(
    pixels_u32: &mut [u32],
    width: i32,
    _height: i32,
    r: &RECT,
    effective_alpha: u8,
) {
    let default_radius = 12.0f32;
    let border_width = 2.0f32;

    let l_f = r.left as f32;
    let t_f = r.top as f32;
    let r_f = r.right as f32;
    let b_f = r.bottom as f32;

    let hw = (r_f - l_f) / 2.0;
    let hh = (b_f - t_f) / 2.0;
    let cx = l_f + hw;
    let cy = t_f + hh;

    // ADAPTIVE RADIUS: Scale down if box is smaller than radius
    let radius = default_radius.min(hw).min(hh);

    let bg_alpha_f = effective_alpha as f32 / 255.0;

    // Only iterate over the bounding area of the selection
    let b_left = (r.left - 10).max(0);
    let b_top = (r.top - 10).max(0);
    let b_right = (r.right + 10).min(width);
    let b_bottom = (r.bottom + 10).min(width); // Note: using width as proxy for height bound

    let rad_int = radius.ceil() as i32;
    let top_band_end = (r.top + rad_int).min(b_bottom);
    let bottom_band_start = (r.bottom - rad_int).max(top_band_end);

    for py_int in b_top..b_bottom {
        let row_base = (py_int * width) as usize;

        // --- FAST PATH: Middle Band (no corners) ---
        if py_int >= top_band_end && py_int < bottom_band_start {
            let lb = r.left as usize;
            let rb = r.right as usize;
            if row_base + lb < pixels_u32.len() {
                // 1. Clear Hole (Transparent interior)
                let start = (row_base + lb).min(pixels_u32.len());
                let end = (row_base + rb).min(pixels_u32.len());
                if start < end {
                    pixels_u32[start..end].fill(0x00000000);
                }

                // 2. Draw Left/Right Borders (2 pixels wide, opaque white)
                for x in 0..2 {
                    if row_base + lb + x < pixels_u32.len() {
                        pixels_u32[row_base + lb + x] = 0xFFFFFFFF;
                    }
                    if row_base + rb - 1 - x < pixels_u32.len() {
                        pixels_u32[row_base + rb - 1 - x] = 0xFFFFFFFF;
                    }
                }
            }
            continue;
        }

        // --- SLOW PATH: Top/Bottom Bands (SDF for corners) ---
        let py = py_int as f32 + 0.5;
        for px_int in b_left..b_right {
            let idx = row_base + px_int as usize;
            if idx >= pixels_u32.len() {
                continue;
            }

            let px = px_int as f32 + 0.5;

            // Rounded Rect SDF
            let dx = (px - cx).abs() - (hw - radius);
            let dy = (py - cy).abs() - (hh - radius);

            let dist = if dx > 0.0 && dy > 0.0 {
                (dx * dx + dy * dy).sqrt() - radius
            } else {
                dx.max(dy) - radius
            };

            let alpha_outer = (0.5 - dist).clamp(0.0, 1.0);
            let alpha_inner = (0.5 - (dist + border_width)).clamp(0.0, 1.0);
            let border_alpha = alpha_outer - alpha_inner;
            let final_bg_alpha = bg_alpha_f * (1.0 - alpha_outer);

            let a = ((final_bg_alpha + border_alpha) * 255.0) as u32;
            let c = (border_alpha * 255.0) as u32;

            pixels_u32[idx] = (a << 24) | (c << 16) | (c << 8) | c;
        }
    }
}
