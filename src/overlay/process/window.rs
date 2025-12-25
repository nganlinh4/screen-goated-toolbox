use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::core::*;
use std::sync::{Mutex, Once};
use std::collections::HashMap;

use super::types::{ProcessingState, MAX_GLOW_BUFFER_DIM};

// --- PROCESSING WINDOW STATIC STATE ---
static REGISTER_PROC_CLASS: Once = Once::new();

lazy_static::lazy_static! {
    static ref PROC_STATES: Mutex<HashMap<isize, ProcessingState>> = Mutex::new(HashMap::new());
}

// --- WINDOW PROC FOR OVERLAY ---
pub unsafe fn create_processing_window(rect: RECT, graphics_mode: String) -> HWND {
    let instance = GetModuleHandleW(None).unwrap();
    let class_name = w!("SGTProcessingOverlay");

    REGISTER_PROC_CLASS.call_once(|| {
        let mut wc = WNDCLASSW::default();
        wc.lpfnWndProc = Some(processing_wnd_proc);
        wc.hInstance = instance.into();
        wc.hCursor = LoadCursorW(None, IDC_WAIT).unwrap();
        wc.lpszClassName = class_name;
        wc.style = CS_HREDRAW | CS_VREDRAW;
        wc.hbrBackground = HBRUSH(std::ptr::null_mut()); 
        RegisterClassW(&wc);
    });

    let w = (rect.right - rect.left).abs();
    let h = (rect.bottom - rect.top).abs();
    let pixels = (w as i64) * (h as i64);
    let timer_interval = if pixels > 5_000_000 { 50 } else if pixels > 2_000_000 { 32 } else { 16 };

    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE, 
        class_name, w!("Processing"), WS_POPUP, rect.left, rect.top, w, h, None, None, Some(instance.into()), None
    ).unwrap_or_default();
    let mut states = PROC_STATES.lock().unwrap();
    states.insert(hwnd.0 as isize, ProcessingState::new(graphics_mode));
    drop(states);
    SetTimer(Some(hwnd), 1, timer_interval, None);
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    hwnd
}

unsafe extern "system" fn processing_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CLOSE => {
            let mut states = PROC_STATES.lock().unwrap();
            let state = states.entry(hwnd.0 as isize).or_insert(ProcessingState::new("standard".to_string()));
            if !state.is_fading_out {
                state.is_fading_out = true;
                if !state.timer_killed {
                    let _ = KillTimer(Some(hwnd), 1); state.timer_killed = true;
                    SetTimer(Some(hwnd), 2, 25, None);
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            let (should_destroy, anim_offset, alpha, is_fading) = {
                let mut states = PROC_STATES.lock().unwrap();
                let state = states.entry(hwnd.0 as isize).or_insert(ProcessingState::new("standard".to_string()));
                let mut destroy_flag = false;
                if state.is_fading_out {
                    if state.alpha > 20 { state.alpha -= 20; } else { state.alpha = 0; destroy_flag = true; }
                } else {
                    state.animation_offset += 5.0; if state.animation_offset > 360.0 { state.animation_offset -= 360.0; }
                }
                (destroy_flag, state.animation_offset, state.alpha, state.is_fading_out)
            };
            if should_destroy { 
                let _ = KillTimer(Some(hwnd), 1); let _ = KillTimer(Some(hwnd), 2); 
                let _ = DestroyWindow(hwnd); 
                return LRESULT(0); 
            }
            
            let mut rect = RECT::default(); let _ = GetWindowRect(hwnd, &mut rect);
            let w = (rect.right - rect.left).abs(); let h = (rect.bottom - rect.top).abs();
            if w > 0 && h > 0 {
                let mut states = PROC_STATES.lock().unwrap();
                let state = states.get_mut(&(hwnd.0 as isize)).unwrap();
                let scale_factor = if w > MAX_GLOW_BUFFER_DIM || h > MAX_GLOW_BUFFER_DIM {
                    (MAX_GLOW_BUFFER_DIM as f32 / w as f32).min(MAX_GLOW_BUFFER_DIM as f32 / h as f32).min(1.0)
                } else { 1.0 };
                let buf_w = ((w as f32) * scale_factor).ceil() as i32;
                let buf_h = ((h as f32) * scale_factor).ceil() as i32;
                if state.cache_hbm.is_invalid() || state.scaled_w != buf_w || state.scaled_h != buf_h {
                    state.cleanup();
                    let screen_dc = GetDC(None);
                    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: buf_w, biHeight: -buf_h, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0 as u32, ..Default::default() }, ..Default::default() };
                    let res = CreateDIBSection(Some(screen_dc), &bmi, DIB_RGB_COLORS, &mut state.cache_bits, None, 0);
                    ReleaseDC(None, screen_dc);
                    if let Ok(hbm) = res { if !hbm.is_invalid() && !state.cache_bits.is_null() { state.cache_hbm = hbm; state.scaled_w = buf_w; state.scaled_h = buf_h; } else { return LRESULT(0); } } else { return LRESULT(0); }
                }
                if !is_fading && !state.cache_bits.is_null() {
                    if state.graphics_mode == "minimal" { crate::overlay::paint_utils::draw_minimal_glow(state.cache_bits as *mut u32, state.scaled_w, state.scaled_h, anim_offset, 1.0, true); }
                    else { crate::overlay::paint_utils::draw_direct_sdf_glow(state.cache_bits as *mut u32, state.scaled_w, state.scaled_h, anim_offset, 1.0, true); }
                }
                let screen_dc = GetDC(None);
                let needs_scaling = state.scaled_w != w || state.scaled_h != h;
                let (final_hbm, final_w, final_h) = if needs_scaling {
                    let scaled_dc = CreateCompatibleDC(Some(screen_dc)); SelectObject(scaled_dc, state.cache_hbm.into());
                    let dest_bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0 as u32, ..Default::default() }, ..Default::default() };
                    let mut dest_bits: *mut core::ffi::c_void = std::ptr::null_mut();
                    let dest_hbm = CreateDIBSection(Some(screen_dc), &dest_bmi, DIB_RGB_COLORS, &mut dest_bits, None, 0);
                    if let Ok(hbm) = dest_hbm {
                        if !hbm.is_invalid() {
                            let dest_dc = CreateCompatibleDC(Some(screen_dc)); SelectObject(dest_dc, hbm.into());
                            SetStretchBltMode(dest_dc, HALFTONE); let _ = StretchBlt(dest_dc, 0, 0, w, h, Some(scaled_dc), 0, 0, state.scaled_w, state.scaled_h, SRCCOPY);
                            let _ = DeleteDC(scaled_dc); (Some((dest_dc, hbm)), w, h)
                        } else { let _ = DeleteDC(scaled_dc); (None, state.scaled_w, state.scaled_h) }
                    } else { let _ = DeleteDC(scaled_dc); (None, state.scaled_w, state.scaled_h) }
                } else { (None, w, h) };
                
                let (mem_dc, old_hbm, temp_res) = if let Some((dc, hbm)) = final_hbm { (dc, HGDIOBJ::default(), Some(hbm)) } else { let dc = CreateCompatibleDC(Some(screen_dc)); let old = SelectObject(dc, state.cache_hbm.into()); (dc, old, None) };
                let pt_src = POINT { x: 0, y: 0 }; let size = SIZE { cx: final_w, cy: final_h };
                let mut blend = BLENDFUNCTION::default(); blend.BlendOp = AC_SRC_OVER as u8; blend.SourceConstantAlpha = alpha; blend.AlphaFormat = AC_SRC_ALPHA as u8;
                let _ = UpdateLayeredWindow(hwnd, None, None, Some(&size), Some(mem_dc), Some(&pt_src), COLORREF(0), Some(&blend), ULW_ALPHA);
                
                if temp_res.is_some() { let _ = DeleteDC(mem_dc); if let Some(hbm) = temp_res { let _ = DeleteObject(hbm.into()); } } else { SelectObject(mem_dc, old_hbm); let _ = DeleteDC(mem_dc); }
                ReleaseDC(None, screen_dc);
            }
            LRESULT(0)
        }
        WM_PAINT => { let mut ps = PAINTSTRUCT::default(); BeginPaint(hwnd, &mut ps); let _ = EndPaint(hwnd, &mut ps); LRESULT(0) }
        WM_DESTROY => { let mut states = PROC_STATES.lock().unwrap(); if let Some(mut state) = states.remove(&(hwnd.0 as isize)) { state.cleanup(); } LRESULT(0) }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
