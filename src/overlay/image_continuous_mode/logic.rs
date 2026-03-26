//! Core logic: color picking, region capture, zoom, and magnification.

use super::*;
use crate::overlay::process::start_processing_pipeline;
use crate::overlay::selection::extract_crop_from_hbitmap_public;
use crate::APP;

const ZOOM_STEP: f32 = 0.25;
const MIN_ZOOM: f32 = 1.0;
const MAX_ZOOM: f32 = 4.0;

pub(super) unsafe fn handle_color_pick(pt: POINT) {
    unsafe {
        let capture_guard = GESTURE_CAPTURE.lock().unwrap();
        if let Some(capture) = capture_guard.as_ref() {
            let hdc_screen = GetDC(None);
            let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
            let old_bmp = SelectObject(hdc_mem, capture.hbitmap.into());

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

    let preset_idx = PRESET_IDX.load(Ordering::SeqCst);

    let capture_guard = GESTURE_CAPTURE.lock().unwrap();
    if let Some(capture) = capture_guard.as_ref() {
        let cropped = extract_crop_from_hbitmap_public(capture, rect);

        // Prepare config on main app
        let (config, preset) = if let Ok(mut app) = APP.lock() {
            app.config.active_preset_idx = preset_idx;
            (app.config.clone(), app.config.presets[preset_idx].clone())
        } else {
            return;
        };

        // Processing on separate thread
        std::thread::spawn(move || {
            start_processing_pipeline(cropped, rect, config, preset);
        });
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
        ensure_magnification_initialized();
        unsafe {
            if let Some(func) = MAG_SET_FULLSCREEN_TRANSFORM_FN {
                let sw = GetSystemMetrics(SM_CXVIRTUALSCREEN) as f32;
                let sh = GetSystemMetrics(SM_CYVIRTUALSCREEN) as f32;
                let sx = GetSystemMetrics(SM_XVIRTUALSCREEN) as f32;
                let sy = GetSystemMetrics(SM_YVIRTUALSCREEN) as f32;

                let view_w = sw / z;
                let view_h = sh / z;

                let mut ox = cursor.x as f32 - view_w / 2.0;
                let mut oy = cursor.y as f32 - view_h / 2.0;

                ox = ox.max(sx).min(sx + sw - view_w);
                oy = oy.max(sy).min(sy + sh - view_h);

                let _ = func(z, ox as i32, oy as i32);
            }
        }
    } else {
        reset_magnification();
    }
}

pub(super) fn reset_magnification() {
    *ZOOM_LEVEL.lock().unwrap() = 1.0;
    if MAG_INITIALIZED.load(Ordering::SeqCst) {
        unsafe {
            if let Some(func) = MAG_SET_FULLSCREEN_TRANSFORM_FN {
                let _ = func(1.0, 0, 0);
            }
        }
    }
}

#[allow(static_mut_refs)]
fn ensure_magnification_initialized() {
    if MAG_INITIALIZED.load(Ordering::SeqCst) {
        return;
    }
    unsafe {
        if !MAG_DLL_LOADED && let Ok(lib) = LoadLibraryW(windows::core::w!("Magnification.dll")) {
            if let Some(init) = GetProcAddress(lib, windows::core::s!("MagInitialize")) {
                MAG_INITIALIZE_FN = Some(std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    MagInitializeFn,
                >(init));
                if let Some(f) = MAG_INITIALIZE_FN {
                    let _ = f();
                }
            }
            if let Some(u) = GetProcAddress(lib, windows::core::s!("MagUninitialize")) {
                MAG_UNINITIALIZE_FN = Some(std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    MagUninitializeFn,
                >(u));
            }
            if let Some(s) = GetProcAddress(lib, windows::core::s!("MagSetFullscreenTransform")) {
                MAG_SET_FULLSCREEN_TRANSFORM_FN = Some(std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    MagSetFullscreenTransformFn,
                >(s));
            }
            MAG_DLL_LOADED = true;
            MAG_INITIALIZED.store(true, Ordering::SeqCst);
        }
    }
}
