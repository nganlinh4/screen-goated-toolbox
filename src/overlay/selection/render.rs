// --- SELECTION RENDER ---
// Rendering and bitmap extraction for selection overlay.

use super::state::*;
use crate::APP;
use crate::GdiCapture;
use crate::overlay::image_continuous_mode::{dim_pixels, render_frozen_with_selection};
use crate::overlay::rounded_selection::RoundedSelection;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// --- BITMAP EXTRACTION ---

pub unsafe fn extract_crop_from_hbitmap(
    capture: &GdiCapture,
    crop_rect: RECT,
) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    unsafe { extract_crop_from_hbitmap_internal(capture, crop_rect) }
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
    unsafe {
        let w = (crop_rect.right - crop_rect.left).abs();
        let h = (crop_rect.bottom - crop_rect.top).abs();
        let empty_image = || {
            image::ImageBuffer::from_raw(
                w.max(1) as u32,
                h.max(1) as u32,
                vec![0; (w.max(1) * h.max(1) * 4) as usize],
            )
            .unwrap()
        };

        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            return empty_image();
        }
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        if hdc_mem.is_invalid() {
            let _ = ReleaseDC(None, hdc_screen);
            return empty_image();
        }

        // Select the big screenshot into DC
        let old_obj = SelectObject(hdc_mem, capture.hbitmap.into());

        // Create a BMI for just the cropped area
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // Top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut buffer: Vec<u8> = vec![0; (w * h * 4) as usize];

        // Create small temp bitmap, blit crop to it, read bits
        let hdc_temp = CreateCompatibleDC(Some(hdc_screen));
        if hdc_temp.is_invalid() {
            SelectObject(hdc_mem, old_obj);
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            return empty_image();
        }
        let hbm_temp = CreateCompatibleBitmap(hdc_screen, w, h);
        if hbm_temp.is_invalid() {
            let _ = DeleteDC(hdc_temp);
            SelectObject(hdc_mem, old_obj);
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            return empty_image();
        }
        let old_temp_obj = SelectObject(hdc_temp, hbm_temp.into());

        // Copy only the crop region from the huge screenshot
        // IMPORTANT: virtual screen coordinates calculation
        let v_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let v_y = GetSystemMetrics(SM_YVIRTUALSCREEN);

        // source x/y in the bitmap
        let src_x = crop_rect.left - v_x;
        let src_y = crop_rect.top - v_y;

        let _ = BitBlt(hdc_temp, 0, 0, w, h, Some(hdc_mem), src_x, src_y, SRCCOPY).ok();

        // GetDIBits requires the bitmap to be deselected, as does DeleteObject.
        SelectObject(hdc_temp, old_temp_obj);

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
        for chunk in buffer.as_chunks_mut::<4>().0 {
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
}

#[cfg(test)]
mod crop_tests {
    use super::*;

    #[test]
    fn repeated_crops_read_pixels_without_leaking_gdi_objects() {
        unsafe {
            let screen = GetDC(None);
            assert!(!screen.is_invalid());
            let dc = CreateCompatibleDC(Some(screen));
            assert!(!dc.is_invalid());
            let bitmap = CreateCompatibleBitmap(screen, 8, 8);
            assert!(!bitmap.is_invalid());
            let old = SelectObject(dc, bitmap.into());
            PatBlt(dc, 0, 0, 8, 8, WHITENESS).unwrap();
            SelectObject(dc, old);
            DeleteDC(dc).unwrap();
            ReleaseDC(None, screen);

            let capture = GdiCapture {
                hbitmap: bitmap,
                width: 8,
                height: 8,
            };
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let rect = RECT {
                left: x,
                top: y,
                right: x + 8,
                bottom: y + 8,
            };
            let first = extract_crop_from_hbitmap_public(&capture, rect);
            assert_eq!(first.get_pixel(0, 0).0, [255, 255, 255, 255]);
            let before = crate::screen_capture::gui_resources_snapshot().gdi_objects;
            for _ in 0..128 {
                let crop = extract_crop_from_hbitmap_public(&capture, rect);
                assert_eq!(crop.get_pixel(7, 7).0, [255, 255, 255, 255]);
            }
            let after = crate::screen_capture::gui_resources_snapshot().gdi_objects;
            assert!(
                after < before + 64,
                "GDI objects grew from {before} to {after}"
            );
        }
    }
}

// --- LAYERED WINDOW RENDERING ---

/// High-performance renderer using UpdateLayeredWindow
/// This allows an OPAQUE white box even when the dim background is TRANSPARENT
#[allow(static_mut_refs)]
pub unsafe fn sync_layered_window_contents(hwnd: HWND) {
    unsafe {
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        if width <= 0 || height <= 0 {
            return;
        }

        // 1. Prepare/Cache 32-bit DIB Context
        let mut dib = CACHED_DIB.lock().unwrap_or_else(|error| error.into_inner());
        if dib.ensure(width, height).is_none() {
            return;
        }

        // 2. Draw using GDI to the DIB
        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            return;
        }
        let mem_dc = CreateCompatibleDC(Some(hdc_screen));
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, hdc_screen);
            return;
        }
        let old_bmp = SelectObject(mem_dc, dib.bitmap().into());

        let effective_alpha = if let Some(zoom_alpha) = ZOOM_ALPHA_OVERRIDE {
            zoom_alpha.min(CURRENT_ALPHA)
        } else {
            CURRENT_ALPHA
        };

        let total_pixels = (width * height) as usize;
        let pixels_u32 = std::slice::from_raw_parts_mut(dib.bits().cast::<u32>(), total_pixels);

        // === FROZEN SCREEN RENDERING ===
        // Blit the captured HBITMAP as overlay background for WYSIWYG integrity.
        let frozen_hbitmap = {
            if let Ok(guard) = APP.lock() {
                guard.screenshot_handle.as_ref().map(|c| c.hbitmap)
            } else {
                None
            }
        };

        let has_frozen = if let Some(hbm) = frozen_hbitmap {
            if effective_alpha > 0 {
                let hdc_src = CreateCompatibleDC(Some(mem_dc));
                if hdc_src.is_invalid() {
                    false
                } else {
                    let old = SelectObject(hdc_src, hbm.into());
                    let _ = BitBlt(mem_dc, 0, 0, width, height, Some(hdc_src), 0, 0, SRCCOPY);
                    SelectObject(hdc_src, old);
                    let _ = DeleteDC(hdc_src);
                    true
                }
            } else {
                false
            }
        } else {
            false
        };

        // Frozen frame rendering: show captured screen from activation for WYSIWYG integrity.
        // The frozen frame is opaque — at alpha=0 it matches live desktop exactly,
        // then dims smoothly as alpha increases. No visual discontinuity.
        if has_frozen && effective_alpha > 0 {
            if selection_outline_visible(IS_DRAGGING, IS_SELECTION_COMMITTED) {
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
                    render_frozen_with_selection(
                        crate::overlay::image_continuous_mode::FrozenSelectionRender {
                            pixels: pixels_u32,
                            size: (width, height),
                            selection: r,
                            dim_alpha: effective_alpha,
                        },
                    );
                } else {
                    dim_pixels(pixels_u32, 256u32 - effective_alpha as u32);
                }
            } else {
                // Frozen frame with dim only (no selection box yet)
                dim_pixels(pixels_u32, 256u32 - effective_alpha as u32);
            }
        } else {
            // Transparent overlay (fade-in/out, zoom browse, or no frozen frame)
            let bg_val = (effective_alpha as u32) << 24;
            pixels_u32.fill(bg_val);

            if selection_outline_visible(IS_DRAGGING, IS_SELECTION_COMMITTED) {
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
        }

        super::region_editor_paint::paint(pixels_u32, width, height, effective_alpha);

        // 4. Update the Layered Window
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            // Border pixels and frozen captures are opaque regardless of dim alpha.
            // Fade the whole surface on exit instead of leaving them until destruction.
            SourceConstantAlpha: exit_surface_opacity(CURRENT_ALPHA, FADE_OUT_START_ALPHA),
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
        let _ = ReleaseDC(None, hdc_screen);
    }
}

fn selection_outline_visible(is_dragging: bool, is_committed: bool) -> bool {
    is_dragging || is_committed
}

fn exit_surface_opacity(current_alpha: u8, start_alpha: Option<u8>) -> u8 {
    let Some(start_alpha) = start_alpha else {
        return 255;
    };
    let progress = (f32::from(current_alpha) / f32::from(start_alpha.max(1))).clamp(0.0, 1.0);
    (255.0 * progress * progress * (3.0 - 2.0 * progress)).round() as u8
}

/// Draw anti-aliased rounded selection box using SDF
fn draw_rounded_selection_box(
    pixels_u32: &mut [u32],
    width: i32,
    height: i32,
    r: &RECT,
    effective_alpha: u8,
) {
    let selection = RoundedSelection::new(r.left, r.top, r.right, r.bottom);
    let bg_alpha_f = effective_alpha as f32 / 255.0;

    // Only iterate over the bounding area of the selection
    let b_left = (r.left - 10).max(0);
    let b_top = (r.top - 10).max(0);
    let b_right = (r.right + 10).min(width);
    let b_bottom = (r.bottom + 10).min(height);

    let rad_int = selection.corner_rows();
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
        for px_int in b_left..b_right {
            let idx = row_base + px_int as usize;
            if idx >= pixels_u32.len() {
                continue;
            }
            let (alpha_outer, border_alpha) = selection.coverage(px_int, py_int);
            let final_bg_alpha = bg_alpha_f * (1.0 - alpha_outer);

            let a = ((final_bg_alpha + border_alpha) * 255.0) as u32;
            let c = (border_alpha * 255.0) as u32;

            pixels_u32[idx] = (a << 24) | (c << 16) | (c << 8) | c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{draw_rounded_selection_box, exit_surface_opacity, selection_outline_visible};
    use windows::Win32::Foundation::RECT;

    #[test]
    fn selections_below_the_width_of_a_portrait_screen_are_drawn() {
        let (width, height) = (100, 300);
        let mut pixels = vec![0x8000_0000_u32; (width * height) as usize];
        let selection = RECT {
            left: 20,
            top: 200,
            right: 80,
            bottom: 280,
        };
        draw_rounded_selection_box(&mut pixels, width, height, &selection, 0x80);
        let at = |x: i32, y: i32| pixels[(y * width + x) as usize];
        assert_eq!(at(50, 240), 0, "selection interior is cleared");
        assert_eq!(at(20, 240), 0xFFFF_FFFF, "left border is drawn");
        assert_eq!(at(10, 240), 0x8000_0000, "outside stays dimmed");
    }

    #[test]
    fn exit_fades_opaque_border_and_frozen_surface_to_zero() {
        assert_eq!(exit_surface_opacity(1, None), 255);
        for start in [1, 40, 80, 120] {
            assert_eq!(exit_surface_opacity(start, Some(start)), 255);
            assert_eq!(exit_surface_opacity(0, Some(start)), 0);
            let mut previous = 255;
            for current in (0..=start).rev() {
                let opacity = exit_surface_opacity(current, Some(start));
                assert!(opacity <= previous);
                previous = opacity;
            }
        }
        assert_eq!(exit_surface_opacity(60, Some(120)), 128);
        assert!(exit_surface_opacity(12, Some(120)) < 10);
    }

    #[test]
    fn committed_selection_keeps_its_outline_after_mouse_release() {
        assert!(selection_outline_visible(true, false));
        assert!(selection_outline_visible(false, true));
        assert!(!selection_outline_visible(false, false));
    }
}
