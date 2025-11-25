use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::core::*;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use crate::APP;

static mut RECORDING_HWND: HWND = HWND(0);
static mut IS_RECORDING: bool = false;
static mut IS_PAUSED: bool = false;
static mut ANIMATION_OFFSET: f32 = 0.0;
static mut CURRENT_PRESET_IDX: usize = 0;
static mut CURRENT_ALPHA: i32 = 0; // For fade-in

// Shared flag for the audio thread
lazy_static::lazy_static! {
    pub static ref AUDIO_STOP_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    pub static ref AUDIO_PAUSE_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

pub fn is_recording_overlay_active() -> bool {
    unsafe { IS_RECORDING && RECORDING_HWND.0 != 0 }
}

pub fn stop_recording_and_submit() {
    unsafe {
        if IS_RECORDING && RECORDING_HWND.0 != 0 {
            AUDIO_STOP_SIGNAL.store(true, Ordering::SeqCst);
            // Force immediate update to show "Processing"
            PostMessageW(RECORDING_HWND, WM_TIMER, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn show_recording_overlay(preset_idx: usize) {
    unsafe {
        if IS_RECORDING { return; }
        
        let preset = APP.lock().unwrap().config.presets[preset_idx].clone();
        
        IS_RECORDING = true;
        IS_PAUSED = false;
        CURRENT_PRESET_IDX = preset_idx;
        ANIMATION_OFFSET = 0.0;
        CURRENT_ALPHA = 0; // Start invisible
        AUDIO_STOP_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_PAUSE_SIGNAL.store(false, Ordering::SeqCst);

        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("RecordingOverlay");

        let mut wc = WNDCLASSW::default();
        if !GetClassInfoW(instance, class_name, &mut wc).as_bool() {
            wc.lpfnWndProc = Some(recording_wnd_proc);
            wc.hInstance = instance;
            wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap(); 
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            RegisterClassW(&wc);
        }

        let w = 420; // Slightly wider to accommodate text comfortably
        let h = 100; // Taller for sub-text
        let screen_x = GetSystemMetrics(SM_CXSCREEN);
        let screen_y = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_x - w) / 2;
        let y = (screen_y - h) / 2;

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("SGT Recording"),
            WS_POPUP,
            x, y, w, h,
            None, None, instance, None
        );

        RECORDING_HWND = hwnd;
        
        SetTimer(hwnd, 1, 16, None); 

        if !preset.hide_recording_ui {
            // Initially 0 alpha, will fade in via timer
            paint_layered_window(hwnd, w, h, 0);
            ShowWindow(hwnd, SW_SHOW);
        }

        std::thread::spawn(move || {
            crate::api::record_audio_and_transcribe(preset, AUDIO_STOP_SIGNAL.clone(), AUDIO_PAUSE_SIGNAL.clone(), hwnd);
        });

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if msg.message == WM_QUIT { break; }
        }

        IS_RECORDING = false;
        RECORDING_HWND = HWND(0);
    }
}

unsafe fn paint_layered_window(hwnd: HWND, width: i32, height: i32, alpha: u8) {
    let screen_dc = GetDC(None);
    
    let bmi = windows::Win32::Graphics::Gdi::BITMAPINFO {
        bmiHeader: windows::Win32::Graphics::Gdi::BITMAPINFOHEADER {
            biSize: std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: windows::Win32::Graphics::Gdi::BI_RGB.0 as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    
    let mut p_bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bitmap = CreateDIBSection(screen_dc, &bmi, windows::Win32::Graphics::Gdi::DIB_RGB_COLORS, &mut p_bits, None, 0).unwrap();
    
    let mem_dc = CreateCompatibleDC(screen_dc);
    let old_bitmap = SelectObject(mem_dc, bitmap);

    let is_waiting = AUDIO_STOP_SIGNAL.load(Ordering::SeqCst);
    let should_animate = !IS_PAUSED || is_waiting;
    
    if !p_bits.is_null() {
        let pixels = std::slice::from_raw_parts_mut(p_bits as *mut u32, (width * height) as usize);
        
        let bx = (width as f32) / 2.0;
        let by = (height as f32) / 2.0;
        let center_x = bx;
        let center_y = by;
        
        let time_rad = ANIMATION_OFFSET.to_radians();
        
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let px = (x as f32) - center_x;
                let py = (y as f32) - center_y;
                
                // Rounded Box SDF
                let d = super::paint_utils::sd_rounded_box(px, py, bx - 2.0, by - 2.0, 16.0);
                
                let mut final_col = 0x000000;
                let mut final_alpha = 0.0f32;

                if should_animate {
                    if d <= 0.0 {
                         final_alpha = 0.40; 
                         final_col = 0x00050505;
                    } else {
                        let angle = py.atan2(px);
                        let noise = (angle * 2.0 + time_rad * 3.0).sin() * 0.2;
                        let glow_width = 8.0 + (noise * 5.0);
                        
                        let t = (d / glow_width).clamp(0.0, 1.0);
                        let glow_intensity = (1.0 - t).powi(2);
                        
                        if glow_intensity > 0.01 {
                            let hue = (angle.to_degrees() + ANIMATION_OFFSET * 2.0) % 360.0;
                            let rgb = super::paint_utils::hsv_to_rgb(hue, 0.85, 1.0);
                            final_col = rgb;
                            final_alpha = glow_intensity;
                        }
                    }
                } else {
                     if d <= 0.0 {
                        final_alpha = 0.40;
                        final_col = 0x00050505;
                     } else if d < 2.0 {
                        final_alpha = 0.8;
                        final_col = 0x00AAAAAA;
                     }
                }

                let a = (final_alpha * 255.0) as u32;
                let r = ((final_col >> 16) & 0xFF) * a / 255;
                let g = ((final_col >> 8) & 0xFF) * a / 255;
                let b = (final_col & 0xFF) * a / 255;
                
                pixels[idx] = (a << 24) | (r << 16) | (g << 8) | b;
            }
        }
    }

    SetBkMode(mem_dc, TRANSPARENT);
    SetTextColor(mem_dc, COLORREF(0x00FFFFFF));

    // --- MAIN STATUS TEXT ---
    let hfont_main = CreateFontW(20, 0, 0, 0, FW_BOLD.0 as i32, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI"));
    let old_font = SelectObject(mem_dc, hfont_main);

    let src_text = if is_waiting {
        "Đang xử lý..."
    } else {
        if CURRENT_PRESET_IDX < APP.lock().unwrap().config.presets.len() {
             let p = &APP.lock().unwrap().config.presets[CURRENT_PRESET_IDX];
             if IS_PAUSED { "Tạm dừng" } 
             else if p.audio_source == "device" { "Ghi âm máy..." } 
             else { "Ghi âm mic..." }
        } else { "Recording..." }
    };

    let mut text_w = crate::overlay::utils::to_wstring(src_text);
    // Move main text up slightly (bottom=60)
    let mut tr = RECT { left: 0, top: 0, right: width, bottom: 65 };
    DrawTextW(mem_dc, &mut text_w, &mut tr, DT_CENTER | DT_BOTTOM | DT_SINGLELINE);

    SelectObject(mem_dc, old_font);
    DeleteObject(hfont_main);

    // --- SUB INSTRUCTION TEXT ---
    let hfont_sub = CreateFontW(15, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI"));
    SelectObject(mem_dc, hfont_sub);
    
    // Greyish color for subtext (0xBBBBBB)
    SetTextColor(mem_dc, COLORREF(0x00DDDDDD)); 

    let sub_text = "Bấm hotkey lần nữa để xử lý âm thanh";
    let mut sub_text_w = crate::overlay::utils::to_wstring(sub_text);
    let mut tr_sub = RECT { left: 0, top: 68, right: width, bottom: height };
    DrawTextW(mem_dc, &mut sub_text_w, &mut tr_sub, DT_CENTER | DT_TOP | DT_SINGLELINE);

    SelectObject(mem_dc, old_font);
    DeleteObject(hfont_sub);


    // DRAW BIG BUTTONS
    let pen = CreatePen(PS_SOLID, 3, COLORREF(0x00FFFFFF)); 
    let old_pen = SelectObject(mem_dc, pen);
    let brush_white = CreateSolidBrush(COLORREF(0x00FFFFFF));
    let brush_none = GetStockObject(NULL_BRUSH);

    // Pause Button
    // Vertical Center
    let p_cx = 45; // Moved slightly inward
    let p_cy = height / 2;
    if IS_PAUSED {
        SelectObject(mem_dc, brush_white);
        let pts = [POINT{x: p_cx - 6, y: p_cy - 10}, POINT{x: p_cx - 6, y: p_cy + 10}, POINT{x: p_cx + 10, y: p_cy}];
        Polygon(mem_dc, &pts);
    } else {
        SelectObject(mem_dc, brush_white);
        Rectangle(mem_dc, p_cx - 8, p_cy - 10, p_cx - 2, p_cy + 10);
        Rectangle(mem_dc, p_cx + 2, p_cy - 10, p_cx + 8, p_cy + 10);
    }

    // Close Button
    let c_cx = width - 45;
    let c_cy = height / 2;
    SelectObject(mem_dc, brush_none);
    MoveToEx(mem_dc, c_cx - 8, c_cy - 8, None); LineTo(mem_dc, c_cx + 8, c_cy + 8);
    MoveToEx(mem_dc, c_cx + 8, c_cy - 8, None); LineTo(mem_dc, c_cx - 8, c_cy + 8);

    SelectObject(mem_dc, old_pen);
    DeleteObject(pen);
    DeleteObject(brush_white);

    let pt_src = POINT { x: 0, y: 0 };
    let size = SIZE { cx: width, cy: height };
    let mut blend = BLENDFUNCTION::default();
    blend.BlendOp = AC_SRC_OVER as u8;
    blend.SourceConstantAlpha = alpha; // Use the fading alpha
    blend.AlphaFormat = AC_SRC_ALPHA as u8;

    UpdateLayeredWindow(hwnd, HDC(0), None, Some(&size), mem_dc, Some(&pt_src), COLORREF(0), Some(&blend), ULW_ALPHA);

    SelectObject(mem_dc, old_bitmap);
    DeleteObject(bitmap);
    DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
}

unsafe extern "system" fn recording_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_SETCURSOR => {
            // Robust Hit Test Check using lParam (Low Word = HitTest Result)
            let hit_test = (lparam.0 & 0xFFFF) as i16 as i32;
            
            if hit_test == HTCLIENT as i32 {
                SetCursor(LoadCursorW(None, IDC_HAND).unwrap());
                LRESULT(1)
            } else {
                 // Delegate to default (will likely be IDC_ARROW due to HTCAPTION)
                 DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_NCHITTEST => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect);
            let local_x = x - rect.left;
            
            // Buttons Areas -> HTCLIENT (Trigger Hand Cursor)
            if local_x < 90 { return LRESULT(HTCLIENT as isize); } 
            let w = rect.right - rect.left;
            if local_x > w - 90 { return LRESULT(HTCLIENT as isize); } 

            // Center Area -> HTCAPTION (Trigger Arrow + Drag)
            LRESULT(HTCAPTION as isize)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            // Note: lparam coords are relative to client area (top-left 0,0)
            let w = 420; 
            
            if x < 90 {
                IS_PAUSED = !IS_PAUSED;
                AUDIO_PAUSE_SIGNAL.store(IS_PAUSED, Ordering::SeqCst);
                paint_layered_window(hwnd, w, 100, CURRENT_ALPHA as u8);
            } else if x > w - 90 {
                AUDIO_STOP_SIGNAL.store(true, Ordering::SeqCst);
                PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if AUDIO_STOP_SIGNAL.load(Ordering::SeqCst) {
                 ANIMATION_OFFSET += 3.0;
            } else if !IS_PAUSED {
                ANIMATION_OFFSET += 5.0;
            }
            if ANIMATION_OFFSET > 360.0 { ANIMATION_OFFSET -= 360.0; }
            
            // Fade In Logic
            if CURRENT_ALPHA < 255 {
                CURRENT_ALPHA += 15; // Fade speed
                if CURRENT_ALPHA > 255 { CURRENT_ALPHA = 255; }
            }

            paint_layered_window(hwnd, 420, 100, CURRENT_ALPHA as u8);
            LRESULT(0)
        }
        WM_CLOSE => {
            AUDIO_STOP_SIGNAL.store(true, Ordering::SeqCst);
            DestroyWindow(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
