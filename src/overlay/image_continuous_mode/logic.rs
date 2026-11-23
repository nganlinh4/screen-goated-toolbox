//! Core logic: color picking, region capture, zoom, and magnification.

use super::*;
use crate::overlay::selection::extract_crop_from_hbitmap_public;

const ZOOM_STEP: f32 = 0.25;
const MIN_ZOOM: f32 = 1.0;
const MAX_ZOOM: f32 = 4.0;

pub(super) unsafe fn handle_color_pick(pt: POINT) {
    unsafe {
        let capture_guard = GESTURE_CAPTURE.lock().unwrap();
        if let Some(capture) = capture_guard.as_ref() {
            let hdc_screen = GetDC(None);
            if hdc_screen.is_invalid() {
                return;
            }
            let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
            if hdc_mem.is_invalid() {
                let _ = ReleaseDC(None, hdc_screen);
                return;
            }
            let old_bmp = SelectObject(hdc_mem, capture.image.hbitmap.into());

            let sx = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let sy = GetSystemMetrics(SM_YVIRTUALSCREEN);

            let color = GetPixel(hdc_mem, pt.x - sx, pt.y - sy);

            SelectObject(hdc_mem, old_bmp);
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);

            let r = (color.0 & 0xFF) as u8;
            let g = ((color.0 >> 8) & 0xFF) as u8;
            let b = ((color.0 >> 16) & 0xFF) as u8;
            let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);

            crate::overlay::utils::copy_to_clipboard(&hex, HWND::default());
            crate::overlay::auto_copy_badge::show_auto_copy_badge_text(&hex);
        }
    }
}

pub(super) fn handle_region_capture(start_x: i32, start_y: i32, end_x: i32, end_y: i32) {
    let rect = RECT {
        left: start_x.min(end_x),
        top: start_y.min(end_y),
        right: start_x.max(end_x),
        bottom: start_y.max(end_y),
    };

    if (rect.right - rect.left) < 5 || (rect.bottom - rect.top) < 5 {
        return;
    }

    let capture_guard = GESTURE_CAPTURE.lock().unwrap();
    if let Some(capture) = capture_guard.as_ref() {
        let cropped = extract_crop_from_hbitmap_public(&capture.image, rect);
        // A queued replacement may already be visible to hotkey routing; this
        // capture remains bound to the currently owning worker.
        let target = *CAPTURE_TARGET.lock().unwrap();
        target.process(cropped, rect, capture.destination);
    }
}

pub(super) fn handle_zoom(delta: i32, cursor: POINT) {
    let mut zoom = ZOOM_LEVEL.lock().unwrap();
    if delta > 0 {
        *zoom = (*zoom + ZOOM_STEP).min(MAX_ZOOM);
    } else {
        *zoom = (*zoom - ZOOM_STEP).max(MIN_ZOOM);
    }

    let z = *zoom;
    drop(zoom);

    if z > 1.01 {
        if !MAG_LEASE_HELD.load(Ordering::SeqCst) {
            if !crate::overlay::magnifier::acquire() {
                return;
            }
            MAG_LEASE_HELD.store(true, Ordering::SeqCst);
        }
        let (sw, sh, sx, sy) = unsafe {
            (
                GetSystemMetrics(SM_CXVIRTUALSCREEN) as f32,
                GetSystemMetrics(SM_CYVIRTUALSCREEN) as f32,
                GetSystemMetrics(SM_XVIRTUALSCREEN) as f32,
                GetSystemMetrics(SM_YVIRTUALSCREEN) as f32,
            )
        };

        let view_w = sw / z;
        let view_h = sh / z;

        let mut ox = cursor.x as f32 - view_w / 2.0;
        let mut oy = cursor.y as f32 - view_h / 2.0;

        ox = ox.max(sx).min(sx + sw - view_w);
        oy = oy.max(sy).min(sy + sh - view_h);

        crate::overlay::magnifier::set_fullscreen_transform(z, ox as i32, oy as i32);
    } else {
        reset_magnification();
    }
}

pub(super) fn reset_magnification() {
    *ZOOM_LEVEL.lock().unwrap() = 1.0;
    // Return the shared lease so the magnifier is balanced across overlays.
    if MAG_LEASE_HELD.swap(false, Ordering::SeqCst) {
        crate::overlay::magnifier::set_fullscreen_transform(1.0, 0, 0);
        crate::overlay::magnifier::release();
    }
}
