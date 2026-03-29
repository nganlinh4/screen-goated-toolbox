//! Graphics: screen capture, pixel manipulation, overlay rendering, and window proc.

use super::*;

// === GRAPHICS ===

pub(super) unsafe fn capture_screen_now() -> anyhow::Result<GdiCapture> {
    crate::screen_capture::capture_screen_fast()
}

/// Darken pixels by dim factor and set alpha to 0xFF.
/// inv_dim_256 = 256 - dim_alpha (256 = no dim, 0 = full black).
#[inline]
pub(crate) fn dim_pixels(pixels: &mut [u32], inv_dim_256: u32) {
    for px in pixels.iter_mut() {
        let v = *px;
        let r = (((v >> 16) & 0xFF) * inv_dim_256) >> 8;
        let g = (((v >> 8) & 0xFF) * inv_dim_256) >> 8;
        let b = ((v & 0xFF) * inv_dim_256) >> 8;
        *px = 0xFF000000 | (r << 16) | (g << 8) | b;
    }
}

/// Fix alpha to 0xFF (HBITMAP from BitBlt has alpha=0).
#[inline]
pub(crate) fn fix_alpha(pixels: &mut [u32]) {
    for px in pixels.iter_mut() {
        *px |= 0xFF000000;
    }
}

/// Render frozen frame with dim outside selection + SDF border.
/// All pixels end up alpha=0xFF (fully opaque overlay).
pub(crate) struct FrozenSelectionRender<'a> {
    pub pixels: &'a mut [u32],
    pub size: (i32, i32),
    pub selection: RECT,
    pub dim_alpha: u8,
}

pub(crate) fn render_frozen_with_selection(request: FrozenSelectionRender<'_>) {
    let FrozenSelectionRender {
        pixels,
        size: (w, h),
        selection,
        dim_alpha,
    } = request;
    let RECT {
        left,
        top,
        right,
        bottom,
    } = selection;
    let clear_l = left.max(0).min(w) as usize;
    let clear_r = right.max(0).min(w) as usize;
    let inv_dim = 256u32 - dim_alpha as u32;
    let dim_f = dim_alpha as f32 / 255.0;

    // SDF parameters
    let default_radius = 8.0f32;
    let border_width = 2.0f32;
    let hw = (right - left) as f32 / 2.0;
    let hh_f = (bottom - top) as f32 / 2.0;
    let cx = left as f32 + hw;
    let cy = top as f32 + hh_f;
    let radius = default_radius.min(hw).min(hh_f);

    let b_left = (left - 10).max(0);
    let b_right = (right + 10).min(w);
    let b_top_y = (top - 10).max(0);
    let b_bottom_y = (bottom + 10).min(h);
    let rad_int = radius.ceil() as i32;
    let top_band_end = (top + rad_int).min(b_bottom_y);
    let bottom_band_start = (bottom - rad_int).max(top_band_end);
    let len = pixels.len();

    for y in 0..h {
        let row_start = (y * w) as usize;
        let row_end = (row_start + w as usize).min(len);
        let row = &mut pixels[row_start..row_end];

        if y < b_top_y || y >= b_bottom_y {
            // Far from selection: dim entire row
            dim_pixels(row, inv_dim);
        } else if y >= top_band_end && y < bottom_band_start {
            // Middle band: dim left/right, leave selection undimmed, draw border
            dim_pixels(&mut row[..clear_l], inv_dim);
            fix_alpha(&mut row[clear_l..clear_r]);
            for bx in 0..2usize {
                if clear_l + bx < row.len() {
                    row[clear_l + bx] = 0xFFFFFFFF;
                }
                if clear_r > bx && clear_r - 1 - bx < row.len() {
                    row[clear_r - 1 - bx] = 0xFFFFFFFF;
                }
            }
            dim_pixels(&mut row[clear_r..], inv_dim);
        } else {
            // Corner band: dim outside SDF zone, SDF per-pixel inside zone
            let b_l = b_left.max(0) as usize;
            let b_r = (b_right as usize).min(row.len());
            dim_pixels(&mut row[..b_l], inv_dim);
            let py = y as f32 + 0.5;
            for px_int in b_left..b_right {
                let xi = px_int as usize;
                if xi >= row.len() {
                    continue;
                }
                let px_f = px_int as f32 + 0.5;
                let dx = (px_f - cx).abs() - (hw - radius);
                let dy = (py - cy).abs() - (hh_f - radius);
                let dist = if dx > 0.0 && dy > 0.0 {
                    (dx * dx + dy * dy).sqrt() - radius
                } else {
                    dx.max(dy) - radius
                };

                let alpha_outer = (0.5 - dist).clamp(0.0, 1.0);
                let alpha_inner = (0.5 - (dist + border_width)).clamp(0.0, 1.0);
                let border_alpha = alpha_outer - alpha_inner;
                let pixel_dim = dim_f * (1.0 - alpha_outer);
                let inv_pd = 1.0 - pixel_dim;

                let v = row[xi];
                let r = ((v >> 16) & 0xFF) as f32;
                let g = ((v >> 8) & 0xFF) as f32;
                let b = (v & 0xFF) as f32;
                let dr = r * inv_pd;
                let dg = g * inv_pd;
                let db = b * inv_pd;

                let inv_ba = 1.0 - border_alpha;
                let fr = (dr * inv_ba + 255.0 * border_alpha) as u32;
                let fg = (dg * inv_ba + 255.0 * border_alpha) as u32;
                let fb = (db * inv_ba + 255.0 * border_alpha) as u32;
                row[xi] = 0xFF000000 | (fr.min(255) << 16) | (fg.min(255) << 8) | fb.min(255);
            }
            dim_pixels(&mut row[b_r..], inv_dim);
        }
    }
}

#[allow(static_mut_refs)]
pub(super) unsafe fn sync_rect_overlay(hwnd: HWND) {
    unsafe {
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let sx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let sy = GetSystemMetrics(SM_YVIRTUALSCREEN);

        if w <= 0 || h <= 0 {
            return;
        }

        // 1. Cache DIB section (avoid per-frame allocation of ~8MB)
        if OVERLAY_DIB_W != w || OVERLAY_DIB_H != h {
            if !std::ptr::addr_of!(OVERLAY_DIB).read().0.is_invalid() {
                let _ = DeleteObject(OVERLAY_DIB.0.into());
                OVERLAY_DIB_BITS = std::ptr::null_mut();
            }

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let hdc_screen = GetDC(None);
            if hdc_screen.is_invalid() {
                return;
            }
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbm = CreateDIBSection(Some(hdc_screen), &bmi, DIB_RGB_COLORS, &mut bits, None, 0);
            let _ = ReleaseDC(None, hdc_screen);

            if let Ok(dib) = hbm {
                OVERLAY_DIB = SendHbitmap(dib);
                OVERLAY_DIB_BITS = bits as *mut u32;
                OVERLAY_DIB_W = w;
                OVERLAY_DIB_H = h;
            } else {
                return;
            }
        }

        // 2. Render into the cached DIB
        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            return;
        }
        let mem_dc = CreateCompatibleDC(Some(hdc_screen));
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, hdc_screen);
            return;
        }
        let old_bmp = SelectObject(mem_dc, OVERLAY_DIB.0.into());

        let len = (w * h) as usize;
        let pixels_u32 = std::slice::from_raw_parts_mut(OVERLAY_DIB_BITS, len);

        // BitBlt frozen capture into DIB (hardware-accelerated, same approach as normal mode).
        let has_frozen = {
            let guard = GESTURE_CAPTURE.lock().unwrap();
            if let Some(cap) = guard.as_ref() {
                let hdc_src = CreateCompatibleDC(Some(mem_dc));
                if hdc_src.is_invalid() {
                    false
                } else {
                    let old = SelectObject(hdc_src, cap.hbitmap.into());
                    let _ = BitBlt(mem_dc, 0, 0, w, h, Some(hdc_src), 0, 0, SRCCOPY);
                    SelectObject(hdc_src, old);
                    let _ = DeleteDC(hdc_src);
                    true
                }
            } else {
                false
            }
        };

        let dim_alpha = CURRENT_DIM_ALPHA;
        let is_dragging = RIGHT_DOWN.load(Ordering::SeqCst);

        if has_frozen && dim_alpha > 0 {
            if is_dragging {
                let s_x = START_X.load(Ordering::SeqCst);
                let s_y = START_Y.load(Ordering::SeqCst);
                let l_x = LAST_X.load(Ordering::SeqCst);
                let l_y = LAST_Y.load(Ordering::SeqCst);

                let left = s_x.min(l_x) - sx;
                let top = s_y.min(l_y) - sy;
                let right = s_x.max(l_x) - sx;
                let bottom = s_y.max(l_y) - sy;

                if (right - left).abs() > 0 && (bottom - top).abs() > 0 {
                    render_frozen_with_selection(FrozenSelectionRender {
                        pixels: pixels_u32,
                        size: (w, h),
                        selection: RECT {
                            left,
                            top,
                            right,
                            bottom,
                        },
                        dim_alpha,
                    });
                } else {
                    dim_pixels(pixels_u32, 256u32 - dim_alpha as u32);
                }
            } else {
                // Frozen frame with dim only (fade-in before drag starts, or shouldn't happen)
                dim_pixels(pixels_u32, 256u32 - dim_alpha as u32);
            }
        } else {
            // No frozen frame: transparent overlay (original behavior)
            if CURRENT_DIM_ALPHA > 0 {
                let alpha = CURRENT_DIM_ALPHA as u32;
                pixels_u32.fill(alpha << 24);
            } else {
                pixels_u32.fill(0);
            }

            if RIGHT_DOWN.load(Ordering::SeqCst) {
                render_selection_border_transparent(pixels_u32, w, h, sx, sy);
            }
        }

        // 3. Update the Layered Window
        let pt_src = POINT { x: 0, y: 0 };
        let size = SIZE { cx: w, cy: h };
        let pt_dst = POINT { x: sx, y: sy };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        let _ = UpdateLayeredWindow(
            hwnd,
            Some(hdc_screen),
            Some(&pt_dst),
            Some(&size),
            Some(mem_dc),
            Some(&pt_src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );

        // Cleanup DC only (bitmap is cached and reused)
        SelectObject(mem_dc, old_bmp);
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, hdc_screen);
    }
}

/// Renders the selection border on a transparent overlay (no frozen frame).
unsafe fn render_selection_border_transparent(
    pixels_u32: &mut [u32],
    w: i32,
    h: i32,
    sx: i32,
    sy: i32,
) {
    let s_x = START_X.load(Ordering::SeqCst);
    let s_y = START_Y.load(Ordering::SeqCst);
    let l_x = LAST_X.load(Ordering::SeqCst);
    let l_y = LAST_Y.load(Ordering::SeqCst);

    let left = s_x.min(l_x) - sx;
    let top = s_y.min(l_y) - sy;
    let right = s_x.max(l_x) - sx;
    let bottom = s_y.max(l_y) - sy;

    if (right - left).abs() > 0 && (bottom - top).abs() > 0 && unsafe { CURRENT_DIM_ALPHA } > 0 {
        let clear_l = left.max(0).min(w);
        let clear_r = right.max(0).min(w);
        let clear_t = top.max(0).min(h);
        let clear_b = bottom.max(0).min(h);

        for y in clear_t..clear_b {
            let start = (y * w + clear_l) as usize;
            let end = (y * w + clear_r) as usize;
            if start < end && end <= pixels_u32.len() {
                pixels_u32[start..end].fill(0);
            }
        }

        let default_radius = 8.0f32;
        let border_width = 2.0f32;
        let sel_hw = (right - left) as f32 / 2.0;
        let sel_hh = (bottom - top) as f32 / 2.0;
        let cx = left as f32 + sel_hw;
        let cy = top as f32 + sel_hh;
        let radius = default_radius.min(sel_hw).min(sel_hh);

        let b_left = (left - 10).max(0);
        let b_top = (top - 10).max(0);
        let b_right = (right + 10).min(w);
        let b_bottom = (bottom + 10).min(h);
        let rad_int = radius.ceil() as i32;
        let top_band_end = (top + rad_int).min(b_bottom);
        let bottom_band_start = (bottom - rad_int).max(top_band_end);

        for py_int in b_top..b_bottom {
            let row_base = (py_int * w) as usize;
            if py_int >= top_band_end && py_int < bottom_band_start {
                let lb = left as usize;
                let rb = right as usize;
                if row_base + lb < pixels_u32.len() {
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
            let py = py_int as f32 + 0.5;
            for px_int in b_left..b_right {
                let idx = row_base + px_int as usize;
                if idx >= pixels_u32.len() {
                    continue;
                }
                let px = px_int as f32 + 0.5;
                let dx = (px - cx).abs() - (sel_hw - radius);
                let dy = (py - cy).abs() - (sel_hh - radius);
                let dist = if dx > 0.0 && dy > 0.0 {
                    (dx * dx + dy * dy).sqrt() - radius
                } else {
                    dx.max(dy) - radius
                };
                let alpha_outer = (0.5 - dist).clamp(0.0, 1.0);
                let alpha_inner = (0.5 - (dist + border_width)).clamp(0.0, 1.0);
                let border_alpha = alpha_outer - alpha_inner;
                if border_alpha > 0.001 {
                    let a = (border_alpha * 255.0) as u32;
                    let c = (border_alpha * 255.0) as u32;
                    pixels_u32[idx] = (a << 24) | (c << 16) | (c << 8) | c;
                }
            }
        }
    }
}

pub(super) unsafe extern "system" fn rect_overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_TIMER => {
                if wparam.0 == DIM_TIMER_ID {
                    let target = if RIGHT_DOWN.load(Ordering::SeqCst) {
                        TARGET_DIM_ALPHA
                    } else {
                        0
                    };
                    let mut changed = false;
                    if CURRENT_DIM_ALPHA < target {
                        CURRENT_DIM_ALPHA =
                            CURRENT_DIM_ALPHA.saturating_add(DIM_FADE_STEP).min(target);
                        changed = true;
                    } else if CURRENT_DIM_ALPHA > target {
                        CURRENT_DIM_ALPHA =
                            CURRENT_DIM_ALPHA.saturating_sub(DIM_FADE_STEP).max(target);
                        changed = true;
                    }

                    if changed || RIGHT_DOWN.load(Ordering::SeqCst) {
                        // Render during fade animation AND during drag.
                        // Timer at 60fps reads latest mouse position from atomics.
                        sync_rect_overlay(hwnd);
                    } else {
                        // Fade complete, not dragging — clean up
                        let _ = KillTimer(Some(hwnd), DIM_TIMER_ID);
                        // Restore click-through if it was removed during drag
                        let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                        if style & WS_EX_TRANSPARENT.0 as i32 == 0 {
                            SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_TRANSPARENT.0 as i32);
                        }
                        // Clear any leftover capture
                        *GESTURE_CAPTURE.lock().unwrap() = None;
                    }
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
