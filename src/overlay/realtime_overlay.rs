//! Real-time audio transcription overlay
//! 
//! Displays streaming transcription text with optional translation panel

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::core::*;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex, Once};
use crate::APP;
use crate::api::realtime_audio::{
    start_realtime_transcription, RealtimeState, SharedRealtimeState,
    get_realtime_display_text, get_translation_display_text,
    WM_REALTIME_UPDATE, WM_TRANSLATION_UPDATE,
};

// Window dimensions
const OVERLAY_WIDTH: i32 = 500;
const OVERLAY_HEIGHT: i32 = 150;
const TRANSLATION_WIDTH: i32 = 500;
const TRANSLATION_HEIGHT: i32 = 150;
const GAP: i32 = 20;

// Stop signal for realtime transcription
lazy_static::lazy_static! {
    pub static ref REALTIME_STOP_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    static ref REALTIME_STATE: SharedRealtimeState = Arc::new(Mutex::new(RealtimeState::new()));
}

static mut REALTIME_HWND: HWND = HWND(0);
static mut TRANSLATION_HWND: HWND = HWND(0);
static mut IS_ACTIVE: bool = false;

// One-time class registration
static REGISTER_REALTIME_CLASS: Once = Once::new();
static REGISTER_TRANSLATION_CLASS: Once = Once::new();

pub fn is_realtime_overlay_active() -> bool {
    unsafe { IS_ACTIVE && REALTIME_HWND.0 != 0 }
}

pub fn show_realtime_overlay(preset_idx: usize) {
    unsafe {
        if IS_ACTIVE { return; }
        
        let preset = APP.lock().unwrap().config.presets[preset_idx].clone();
        
        // Reset state
        IS_ACTIVE = true;
        REALTIME_STOP_SIGNAL.store(false, Ordering::SeqCst);
        {
            let mut state = REALTIME_STATE.lock().unwrap();
            *state = RealtimeState::new();
        }
        
        let instance = GetModuleHandleW(None).unwrap();
        
        // --- Create Main Realtime Overlay ---
        let class_name = w!("RealtimeTranscriptOverlay");
        REGISTER_REALTIME_CLASS.call_once(|| {
            let mut wc = WNDCLASSW::default();
            wc.lpfnWndProc = Some(realtime_wnd_proc);
            wc.hInstance = instance;
            wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap();
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            let _ = RegisterClassW(&wc);
        });
        
        // Calculate positions
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        
        // Check if we have a translation block
        let has_translation = preset.blocks.len() > 1;
        
        let (main_x, main_y) = if has_translation {
            // Side by side
            let total_w = OVERLAY_WIDTH * 2 + GAP;
            ((screen_w - total_w) / 2, (screen_h - OVERLAY_HEIGHT) / 2)
        } else {
            // Centered
            ((screen_w - OVERLAY_WIDTH) / 2, (screen_h - OVERLAY_HEIGHT) / 2)
        };
        
        let main_hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("Realtime Transcription"),
            WS_POPUP,
            main_x, main_y, OVERLAY_WIDTH, OVERLAY_HEIGHT,
            None, None, instance, None
        );
        
        REALTIME_HWND = main_hwnd;
        paint_realtime_overlay(main_hwnd, OVERLAY_WIDTH, OVERLAY_HEIGHT, false);
        ShowWindow(main_hwnd, SW_SHOWNOACTIVATE);
        SetTimer(main_hwnd, 1, 16, None);
        
        // --- Create Translation Overlay if needed ---
        let translation_hwnd = if has_translation {
            let trans_class = w!("RealtimeTranslationOverlay");
            REGISTER_TRANSLATION_CLASS.call_once(|| {
                let mut wc = WNDCLASSW::default();
                wc.lpfnWndProc = Some(translation_wnd_proc);
                wc.hInstance = instance;
                wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap();
                wc.lpszClassName = trans_class;
                wc.style = CS_HREDRAW | CS_VREDRAW;
                let _ = RegisterClassW(&wc);
            });
            
            let trans_x = main_x + OVERLAY_WIDTH + GAP;
            let trans_hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                trans_class,
                w!("Translation"),
                WS_POPUP,
                trans_x, main_y, TRANSLATION_WIDTH, TRANSLATION_HEIGHT,
                None, None, instance, None
            );
            
            TRANSLATION_HWND = trans_hwnd;
            paint_translation_overlay(trans_hwnd, TRANSLATION_WIDTH, TRANSLATION_HEIGHT);
            ShowWindow(trans_hwnd, SW_SHOWNOACTIVATE);
            
            Some(trans_hwnd)
        } else {
            TRANSLATION_HWND = HWND(0);
            None
        };
        
        // Start realtime transcription
        start_realtime_transcription(
            preset,
            REALTIME_STOP_SIGNAL.clone(),
            main_hwnd,
            translation_hwnd,
            REALTIME_STATE.clone(),
        );
        
        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if msg.message == WM_QUIT { break; }
        }
        
        IS_ACTIVE = false;
        REALTIME_HWND = HWND(0);
        TRANSLATION_HWND = HWND(0);
    }
}

unsafe fn paint_realtime_overlay(hwnd: HWND, width: i32, height: i32, is_closing: bool) {
    let screen_dc = GetDC(None);
    
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    
    let mut p_bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bitmap = CreateDIBSection(screen_dc, &bmi, DIB_RGB_COLORS, &mut p_bits, None, 0).unwrap();
    
    let mem_dc = CreateCompatibleDC(screen_dc);
    let old_bitmap = SelectObject(mem_dc, bitmap);
    
    if !p_bits.is_null() {
        let pixels = std::slice::from_raw_parts_mut(p_bits as *mut u32, (width * height) as usize);
        
        // Draw rounded rectangle background
        let corner_radius = 12.0;
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let px = x as f32 - cx;
                let py = y as f32 - cy;
                
                let d = crate::overlay::paint_utils::sd_rounded_box(px, py, cx - 4.0, cy - 4.0, corner_radius);
                
                if d < 0.0 {
                    // Inside - dark semi-transparent background
                    let alpha = 0.85;
                    let a = (alpha * 255.0) as u32;
                    let c = 0x1A; // Dark gray
                    pixels[idx] = (a << 24) | (c << 16) | (c << 8) | c;
                } else if d < 1.5 {
                    // Edge with anti-aliasing
                    let t = 1.0 - (d / 1.5);
                    let alpha = 0.85 * t;
                    let a = (alpha * 255.0) as u32;
                    let c = 0x1A;
                    pixels[idx] = (a << 24) | (c << 16) | (c << 8) | c;
                } else {
                    pixels[idx] = 0;
                }
            }
        }
        
        // Add subtle border glow (cyan for transcription)
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let px = x as f32 - cx;
                let py = y as f32 - cy;
                
                let d = crate::overlay::paint_utils::sd_rounded_box(px, py, cx - 4.0, cy - 4.0, corner_radius);
                
                if d >= -2.0 && d < 4.0 {
                    let glow_t = ((d + 2.0) / 6.0).clamp(0.0, 1.0);
                    let glow_intensity = 1.0 - glow_t;
                    
                    if glow_intensity > 0.01 {
                        let existing = pixels[idx];
                        let existing_a = (existing >> 24) & 0xFF;
                        
                        let glow_a = (glow_intensity * 0.5 * 255.0) as u32;
                        // Cyan glow
                        let glow_r = (0 as f32 * glow_intensity) as u32;
                        let glow_g = (200 as f32 * glow_intensity) as u32;
                        let glow_b = (255 as f32 * glow_intensity) as u32;
                        
                        let final_a = (existing_a + glow_a).min(255);
                        let final_r = (((existing >> 16) & 0xFF) + glow_r).min(255);
                        let final_g = (((existing >> 8) & 0xFF) + glow_g).min(255);
                        let final_b = ((existing & 0xFF) + glow_b).min(255);
                        
                        pixels[idx] = (final_a << 24) | (final_r << 16) | (final_g << 8) | final_b;
                    }
                }
            }
        }
    }
    
    // Draw text
    SetBkMode(mem_dc, TRANSPARENT);
    SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
    
    // Title
    let hfont_title = CreateFontW(
        14, 0, 0, 0, FW_BOLD.0 as i32, 0, 0, 0,
        DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32,
        (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI")
    );
    SelectObject(mem_dc, hfont_title);
    
    let title = "🎤 Đang nghe...";
    let mut title_w = crate::overlay::utils::to_wstring(title);
    let mut title_rect = RECT { left: 15, top: 10, right: width - 15, bottom: 30 };
    DrawTextW(mem_dc, &mut title_w, &mut title_rect, DT_LEFT | DT_TOP | DT_SINGLELINE);
    
    DeleteObject(hfont_title);
    
    // Transcript text
    let hfont_text = CreateFontW(
        16, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
        DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32,
        (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI")
    );
    SelectObject(mem_dc, hfont_text);
    
    let display_text = get_realtime_display_text();
    let text_to_show = if display_text.is_empty() {
        if is_closing { "Đã dừng." } else { "Chờ giọng nói..." }
    } else {
        &display_text
    };
    
    let mut text_w = crate::overlay::utils::to_wstring(text_to_show);
    let mut text_rect = RECT { left: 15, top: 35, right: width - 15, bottom: height - 35 };
    DrawTextW(mem_dc, &mut text_w, &mut text_rect, DT_LEFT | DT_TOP | DT_WORDBREAK);
    
    DeleteObject(hfont_text);
    
    // Close hint
    let hfont_hint = CreateFontW(
        12, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
        DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32,
        (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI")
    );
    SelectObject(mem_dc, hfont_hint);
    SetTextColor(mem_dc, COLORREF(0x00AAAAAA));
    
    let hint = "Bấm × để dừng";
    let mut hint_w = crate::overlay::utils::to_wstring(hint);
    let mut hint_rect = RECT { left: 15, top: height - 25, right: width - 40, bottom: height - 5 };
    DrawTextW(mem_dc, &mut hint_w, &mut hint_rect, DT_LEFT | DT_BOTTOM | DT_SINGLELINE);
    
    DeleteObject(hfont_hint);
    
    // Draw close button (X) in top-right
    SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
    let hfont_close = CreateFontW(
        20, 0, 0, 0, FW_BOLD.0 as i32, 0, 0, 0,
        DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32,
        (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI")
    );
    SelectObject(mem_dc, hfont_close);
    
    let close_btn = "×";
    let mut close_w = crate::overlay::utils::to_wstring(close_btn);
    let mut close_rect = RECT { left: width - 30, top: 5, right: width - 5, bottom: 30 };
    DrawTextW(mem_dc, &mut close_w, &mut close_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    
    DeleteObject(hfont_close);
    
    // Fix text alpha
    if !p_bits.is_null() {
        GdiFlush();
        let pixels = std::slice::from_raw_parts_mut(p_bits as *mut u32, (width * height) as usize);
        for px in pixels.iter_mut() {
            let val = *px;
            let a = (val >> 24) & 0xFF;
            let r = (val >> 16) & 0xFF;
            let g = (val >> 8) & 0xFF;
            let b = val & 0xFF;
            let max_c = r.max(g).max(b);
            if max_c > a {
                *px = (max_c << 24) | (r << 16) | (g << 8) | b;
            }
        }
    }
    
    let size = SIZE { cx: width, cy: height };
    let pt_src = POINT { x: 0, y: 0 };
    let mut blend = BLENDFUNCTION::default();
    blend.BlendOp = AC_SRC_OVER as u8;
    blend.SourceConstantAlpha = 255;
    blend.AlphaFormat = AC_SRC_ALPHA as u8;
    
    UpdateLayeredWindow(hwnd, HDC(0), None, Some(&size), mem_dc, Some(&pt_src), COLORREF(0), Some(&blend), ULW_ALPHA);
    
    SelectObject(mem_dc, old_bitmap);
    DeleteObject(bitmap);
    DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
}

unsafe fn paint_translation_overlay(hwnd: HWND, width: i32, height: i32) {
    let screen_dc = GetDC(None);
    
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    
    let mut p_bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bitmap = CreateDIBSection(screen_dc, &bmi, DIB_RGB_COLORS, &mut p_bits, None, 0).unwrap();
    
    let mem_dc = CreateCompatibleDC(screen_dc);
    let old_bitmap = SelectObject(mem_dc, bitmap);
    
    if !p_bits.is_null() {
        let pixels = std::slice::from_raw_parts_mut(p_bits as *mut u32, (width * height) as usize);
        
        // Draw rounded rectangle background
        let corner_radius = 12.0;
        let cx = width as f32 / 2.0;
        let cy = height as f32 / 2.0;
        
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let px = x as f32 - cx;
                let py = y as f32 - cy;
                
                let d = crate::overlay::paint_utils::sd_rounded_box(px, py, cx - 4.0, cy - 4.0, corner_radius);
                
                if d < 0.0 {
                    let alpha = 0.85;
                    let a = (alpha * 255.0) as u32;
                    let c = 0x1A;
                    pixels[idx] = (a << 24) | (c << 16) | (c << 8) | c;
                } else if d < 1.5 {
                    let t = 1.0 - (d / 1.5);
                    let alpha = 0.85 * t;
                    let a = (alpha * 255.0) as u32;
                    let c = 0x1A;
                    pixels[idx] = (a << 24) | (c << 16) | (c << 8) | c;
                } else {
                    pixels[idx] = 0;
                }
            }
        }
        
        // Add subtle border glow (orange for translation)
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let px = x as f32 - cx;
                let py = y as f32 - cy;
                
                let d = crate::overlay::paint_utils::sd_rounded_box(px, py, cx - 4.0, cy - 4.0, corner_radius);
                
                if d >= -2.0 && d < 4.0 {
                    let glow_t = ((d + 2.0) / 6.0).clamp(0.0, 1.0);
                    let glow_intensity = 1.0 - glow_t;
                    
                    if glow_intensity > 0.01 {
                        let existing = pixels[idx];
                        let existing_a = (existing >> 24) & 0xFF;
                        
                        let glow_a = (glow_intensity * 0.5 * 255.0) as u32;
                        // Orange glow
                        let glow_r = (255 as f32 * glow_intensity) as u32;
                        let glow_g = (150 as f32 * glow_intensity) as u32;
                        let glow_b = (50 as f32 * glow_intensity) as u32;
                        
                        let final_a = (existing_a + glow_a).min(255);
                        let final_r = (((existing >> 16) & 0xFF) + glow_r).min(255);
                        let final_g = (((existing >> 8) & 0xFF) + glow_g).min(255);
                        let final_b = ((existing & 0xFF) + glow_b).min(255);
                        
                        pixels[idx] = (final_a << 24) | (final_r << 16) | (final_g << 8) | final_b;
                    }
                }
            }
        }
    }
    
    // Draw text
    SetBkMode(mem_dc, TRANSPARENT);
    SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
    
    // Title
    let hfont_title = CreateFontW(
        14, 0, 0, 0, FW_BOLD.0 as i32, 0, 0, 0,
        DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32,
        (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI")
    );
    SelectObject(mem_dc, hfont_title);
    
    let title = "🌐 Bản dịch";
    let mut title_w = crate::overlay::utils::to_wstring(title);
    let mut title_rect = RECT { left: 15, top: 10, right: width - 15, bottom: 30 };
    DrawTextW(mem_dc, &mut title_w, &mut title_rect, DT_LEFT | DT_TOP | DT_SINGLELINE);
    
    DeleteObject(hfont_title);
    
    // Translation text
    let hfont_text = CreateFontW(
        16, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
        DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32,
        (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI")
    );
    SelectObject(mem_dc, hfont_text);
    
    let display_text = get_translation_display_text();
    let text_to_show = if display_text.is_empty() {
        "Đang chờ câu hoàn chỉnh..."
    } else {
        &display_text
    };
    
    let mut text_w = crate::overlay::utils::to_wstring(text_to_show);
    let mut text_rect = RECT { left: 15, top: 35, right: width - 15, bottom: height - 10 };
    DrawTextW(mem_dc, &mut text_w, &mut text_rect, DT_LEFT | DT_TOP | DT_WORDBREAK);
    
    DeleteObject(hfont_text);
    
    // Fix text alpha
    if !p_bits.is_null() {
        GdiFlush();
        let pixels = std::slice::from_raw_parts_mut(p_bits as *mut u32, (width * height) as usize);
        for px in pixels.iter_mut() {
            let val = *px;
            let a = (val >> 24) & 0xFF;
            let r = (val >> 16) & 0xFF;
            let g = (val >> 8) & 0xFF;
            let b = val & 0xFF;
            let max_c = r.max(g).max(b);
            if max_c > a {
                *px = (max_c << 24) | (r << 16) | (g << 8) | b;
            }
        }
    }
    
    let size = SIZE { cx: width, cy: height };
    let pt_src = POINT { x: 0, y: 0 };
    let mut blend = BLENDFUNCTION::default();
    blend.BlendOp = AC_SRC_OVER as u8;
    blend.SourceConstantAlpha = 255;
    blend.AlphaFormat = AC_SRC_ALPHA as u8;
    
    UpdateLayeredWindow(hwnd, HDC(0), None, Some(&size), mem_dc, Some(&pt_src), COLORREF(0), Some(&blend), ULW_ALPHA);
    
    SelectObject(mem_dc, old_bitmap);
    DeleteObject(bitmap);
    DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
}

unsafe extern "system" fn realtime_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_REALTIME_UPDATE => {
            paint_realtime_overlay(hwnd, OVERLAY_WIDTH, OVERLAY_HEIGHT, false);
            LRESULT(0)
        }
        WM_TIMER => {
            // Periodic repaint for animation (if needed)
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            
            // Check if click is on close button (top-right area)
            if x > OVERLAY_WIDTH - 35 && y < 30 {
                // Stop and close
                REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
                PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            
            LRESULT(0)
        }
        WM_NCHITTEST => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect);
            let local_x = x - rect.left;
            let local_y = y - rect.top;
            
            // Close button area
            if local_x > OVERLAY_WIDTH - 35 && local_y < 30 {
                return LRESULT(HTCLIENT as isize);
            }
            
            // Rest is draggable
            LRESULT(HTCAPTION as isize)
        }
        WM_CLOSE => {
            REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
            DestroyWindow(hwnd);
            
            // Also close translation window if exists
            if TRANSLATION_HWND.0 != 0 {
                DestroyWindow(TRANSLATION_HWND);
            }
            
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn translation_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRANSLATION_UPDATE => {
            paint_translation_overlay(hwnd, TRANSLATION_WIDTH, TRANSLATION_HEIGHT);
            LRESULT(0)
        }
        WM_NCHITTEST => {
            // Entire window is draggable
            LRESULT(HTCAPTION as isize)
        }
        WM_CLOSE => {
            // If translation closes, stop everything
            REALTIME_STOP_SIGNAL.store(true, Ordering::SeqCst);
            DestroyWindow(hwnd);
            
            // Also close main window
            if REALTIME_HWND.0 != 0 {
                DestroyWindow(REALTIME_HWND);
            }
            
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
