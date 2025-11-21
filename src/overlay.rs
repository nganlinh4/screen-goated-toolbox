use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::core::*;
use std::sync::{Arc, Mutex};
use std::mem::size_of;
use image::GenericImageView; 

use crate::{AppState, APP, api::translate_image_streaming};

static mut START_POS: POINT = POINT { x: 0, y: 0 };
static mut CURR_POS: POINT = POINT { x: 0, y: 0 };
static mut IS_DRAGGING: bool = false;
static mut IS_PROCESSING: bool = false;
static mut SCAN_LINE_Y: i32 = 0;
static mut SCAN_DIR: i32 = 5;
static mut SELECTION_OVERLAY_ACTIVE: bool = false;
static mut SELECTION_OVERLAY_HWND: HWND = HWND(0);

fn to_wstring(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// Helper to check if selection overlay is currently active and dismiss it
pub fn is_selection_overlay_active_and_dismiss() -> bool {
    unsafe {
        if SELECTION_OVERLAY_ACTIVE && SELECTION_OVERLAY_HWND.0 != 0 {
            PostMessageW(SELECTION_OVERLAY_HWND, WM_CLOSE, WPARAM(0), LPARAM(0));
            true
        } else {
            false
        }
    }
}


// --- CLIPBOARD SUPPORT ---
fn copy_to_clipboard(text: &str, hwnd: HWND) {
    unsafe {
        if OpenClipboard(hwnd).as_bool() {
            EmptyClipboard();
            
            // Convert text to UTF-16
            let wide_text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let mem_size = wide_text.len() * 2;
            
            // Allocate global memory
            if let Ok(h_mem) = GlobalAlloc(GMEM_MOVEABLE, mem_size) {
                let ptr = GlobalLock(h_mem) as *mut u16;
                std::ptr::copy_nonoverlapping(wide_text.as_ptr(), ptr, wide_text.len());
                GlobalUnlock(h_mem);
                
                // Set clipboard data (CF_UNICODETEXT = 13)
                let h_mem_handle = HANDLE(h_mem.0);
                let _ = SetClipboardData(13u32, h_mem_handle);
            }
            
            CloseClipboard();
        }
    }
}

// --- 1. SELECTION OVERLAY ---

pub fn show_selection_overlay() {
    unsafe {
        // Mark overlay as active
        SELECTION_OVERLAY_ACTIVE = true;
        
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("SnippingOverlay");
        
        let wc = WNDCLASSW {
            lpfnWndProc: Some(selection_wnd_proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap(),
            lpszClassName: class_name,
            hbrBackground: CreateSolidBrush(COLORREF(0x00000000)),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        
        let hwnd = CreateWindowExW(
            // WS_EX_TOOLWINDOW prevents taskbar appearance
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("Snipping"),
            WS_POPUP | WS_VISIBLE,
            x, y, w, h,
            None, None, instance, None
        );

        // Store the window handle
        SELECTION_OVERLAY_HWND = hwnd;

        SetLayeredWindowAttributes(hwnd, COLORREF(0), 100, LWA_ALPHA);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if msg.message == WM_CLOSE { break; }
        }
        
        // Mark overlay as inactive when it closes
        SELECTION_OVERLAY_ACTIVE = false;
        SELECTION_OVERLAY_HWND = HWND(0);
        
        UnregisterClassW(class_name, instance);
    }
}

unsafe extern "system" fn selection_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_KEYDOWN => {
            if wparam.0 == VK_ESCAPE.0 as usize {
                PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if !IS_PROCESSING {
                IS_DRAGGING = true;
                GetCursorPos(std::ptr::addr_of_mut!(START_POS));
                CURR_POS = START_POS;
                SetCapture(hwnd);
                InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if IS_DRAGGING {
                GetCursorPos(std::ptr::addr_of_mut!(CURR_POS));
                InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if IS_DRAGGING {
                IS_DRAGGING = false;
                ReleaseCapture();

                let rect = RECT {
                    left: START_POS.x.min(CURR_POS.x),
                    top: START_POS.y.min(CURR_POS.y),
                    right: START_POS.x.max(CURR_POS.x),
                    bottom: START_POS.y.max(CURR_POS.y),
                };

                if (rect.right - rect.left) > 10 && (rect.bottom - rect.top) > 10 {
                    IS_PROCESSING = true;
                    SCAN_LINE_Y = rect.top;
                    InvalidateRect(hwnd, None, false);
                    SetTimer(hwnd, 1, 30, None);
                    
                    let app_clone = APP.clone();
                    std::thread::spawn(move || {
                        process_and_close(app_clone, rect, hwnd);
                    });
                } else {
                    PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if IS_PROCESSING {
                let rect = RECT {
                    left: START_POS.x.min(CURR_POS.x),
                    top: START_POS.y.min(CURR_POS.y),
                    right: START_POS.x.max(CURR_POS.x),
                    bottom: START_POS.y.max(CURR_POS.y),
                };
                
                SCAN_LINE_Y += SCAN_DIR;
                if SCAN_LINE_Y > rect.bottom || SCAN_LINE_Y < rect.top {
                    SCAN_DIR = -SCAN_DIR;
                }
                InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            
            let mem_dc = CreateCompatibleDC(hdc);
            let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            let mem_bitmap = CreateCompatibleBitmap(hdc, width, height);
            SelectObject(mem_dc, mem_bitmap);

            let brush = CreateSolidBrush(COLORREF(0x00000000));
            let full_rect = RECT { left: 0, top: 0, right: width, bottom: height };
            FillRect(mem_dc, &full_rect, brush);
            DeleteObject(brush);

            if IS_DRAGGING || IS_PROCESSING {
                let rect = RECT {
                    left: (START_POS.x.min(CURR_POS.x)) - GetSystemMetrics(SM_XVIRTUALSCREEN),
                    top: (START_POS.y.min(CURR_POS.y)) - GetSystemMetrics(SM_YVIRTUALSCREEN),
                    right: (START_POS.x.max(CURR_POS.x)) - GetSystemMetrics(SM_XVIRTUALSCREEN),
                    bottom: (START_POS.y.max(CURR_POS.y)) - GetSystemMetrics(SM_YVIRTUALSCREEN),
                };
                
                let frame_brush = CreateSolidBrush(COLORREF(0x00FFFFFF));
                FrameRect(mem_dc, &rect, frame_brush);
                DeleteObject(frame_brush);
                
                if IS_PROCESSING {
                     let scan_y_rel = SCAN_LINE_Y - GetSystemMetrics(SM_YVIRTUALSCREEN);
                     let scan_rect = RECT {
                         left: rect.left + 2,
                         top: scan_y_rel,
                         right: rect.right - 2,
                         bottom: scan_y_rel + 2
                     };
                     let scan_brush = CreateSolidBrush(COLORREF(0x0000FF00));
                     FillRect(mem_dc, &scan_rect, scan_brush);
                     DeleteObject(scan_brush);
                }
            }

            BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY).ok().unwrap();
            DeleteObject(mem_bitmap);
            DeleteDC(mem_dc);
            EndPaint(hwnd, &mut ps);
            LRESULT(0)
        }
        WM_CLOSE => {
            KillTimer(hwnd, 1);
            IS_PROCESSING = false;
            DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn get_error_message(error: &str, lang: &str) -> String {
    match error {
        "NO_API_KEY" => {
            match lang {
                "vi" => "Bạn chưa nhập API key!".to_string(),
                _ => "You haven't entered an API key!".to_string(),
            }
        }
        "INVALID_API_KEY" => {
            match lang {
                "vi" => "API key không hợp lệ!".to_string(),
                _ => "Invalid API key!".to_string(),
            }
        }
        _ => {
            match lang {
                "vi" => format!("Lỗi: {}", error),
                _ => format!("Error: {}", error),
            }
        }
    }
}

fn process_and_close(app: Arc<Mutex<AppState>>, rect: RECT, overlay_hwnd: HWND) {
    let (img, config, model_name) = {
        let guard = app.lock().unwrap();
        let model = guard.model_selector.get_model();
        (guard.original_screenshot.clone().unwrap(), guard.config.clone(), model)
    };

    let x_virt = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let y_virt = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    
    let crop_x = (rect.left - x_virt).max(0) as u32;
    let crop_y = (rect.top - y_virt).max(0) as u32;
    let crop_w = (rect.right - rect.left).abs() as u32;
    let crop_h = (rect.bottom - rect.top).abs() as u32;
    
    let img_w = img.width();
    let img_h = img.height();
    let crop_w = crop_w.min(img_w.saturating_sub(crop_x));
    let crop_h = crop_h.min(img_h.saturating_sub(crop_y));

    if crop_w > 0 && crop_h > 0 {
        let cropped = img.view(crop_x, crop_y, crop_w, crop_h).to_image();
        
        // Store settings before config is moved
        let auto_copy = config.auto_copy;
        let api_key = config.api_key.clone();
        let ui_language = config.ui_language.clone();
        let target_lang = config.target_language.clone();
        let streaming_enabled = config.streaming_enabled;
        
        // NOTE: We do NOT close the overlay_hwnd here. We keep it open (and scanning)
        // until the first chunk of data arrives.
        
        // Spawn a dedicated UI thread for the result window
        std::thread::spawn(move || {
            // Create result window immediately but HIDDEN
            let result_hwnd = create_result_window(rect);
            
            // Spawn a worker thread for the blocking API call
            std::thread::spawn(move || {
                // Accumulate text for final result and auto-copy
                let accumulated = Arc::new(Mutex::new(String::new()));
                let accumulated_clone = accumulated.clone();
                let mut first_chunk_received = false;
                
                // Blocking call with callback for real-time updates
                let res = translate_image_streaming(&api_key, target_lang, model_name, cropped, streaming_enabled, |chunk| {
                    let mut text = accumulated_clone.lock().unwrap();
                    text.push_str(chunk);
                    
                    // On first chunk, switch windows
                    if !first_chunk_received {
                        first_chunk_received = true;
                        unsafe {
                            // Close the selection/scanning overlay
                            PostMessageW(overlay_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                            // Show the result window
                            ShowWindow(result_hwnd, SW_SHOW);
                        }
                    }
                    
                    // Update the window in real-time
                    update_result_window(&text);
                });

                match res {
                    Ok(text) => {
                        if !text.trim().is_empty() {
                            // Apply auto-copy if enabled
                            if auto_copy {
                                std::thread::spawn(move || {
                                    std::thread::sleep(std::time::Duration::from_millis(100));
                                    copy_to_clipboard(&text, HWND(0));
                                });
                            }
                        }
                    }
                    Err(e) => {
                        // If we error out before showing the window, show it now
                        if !first_chunk_received {
                            unsafe {
                                PostMessageW(overlay_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                                ShowWindow(result_hwnd, SW_SHOW);
                            }
                        }
                        let error_msg = get_error_message(&e.to_string(), &ui_language);
                        update_result_window(&error_msg);
                    }
                }
            });

            // Run message loop on this thread to keep the result window responsive
            unsafe {
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).into() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    if !IsWindow(result_hwnd).as_bool() { break; }
                }
            }
        });

    } else {
        unsafe { PostMessageW(overlay_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); }
    }
}

// --- 2. RESULT WINDOW (BLUR ACRYLIC) ---

static mut IS_DISMISSING: bool = false;
static mut DISMISS_ALPHA: u8 = 255;
static mut RESULT_HWND: HWND = HWND(0);
static mut RESULT_RECT: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };

pub fn create_result_window(target_rect: RECT) -> HWND {
    unsafe {
        IS_DISMISSING = false;
        DISMISS_ALPHA = 255;
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("TranslationResult");
        
        let mut wc = WNDCLASSW::default();
        if !GetClassInfoW(instance, class_name, &mut wc).as_bool() {
            wc.lpfnWndProc = Some(result_wnd_proc);
            wc.hInstance = instance;
            // Load custom broom cursor
            static BROOM_CURSOR_DATA: &[u8] = include_bytes!("../broom.cur");
            
            let temp_path = std::env::temp_dir().join("broom_cursor.cur");
            if let Ok(()) = std::fs::write(&temp_path, BROOM_CURSOR_DATA) {
                let path_wide: Vec<u16> = temp_path.to_string_lossy()
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let cursor_handle = LoadImageW(
                    None,
                    PCWSTR(path_wide.as_ptr()),
                    IMAGE_CURSOR,
                    0, 0,
                    LR_LOADFROMFILE | LR_DEFAULTSIZE
                );
                wc.hCursor = if let Ok(handle) = cursor_handle {
                    HCURSOR(handle.0)
                } else {
                    LoadCursorW(None, IDC_HAND).unwrap()
                };
            } else {
                wc.hCursor = LoadCursorW(None, IDC_HAND).unwrap();
            }
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            wc.hbrBackground = HBRUSH(0);
            RegisterClassW(&wc);
        }

        let width = (target_rect.right - target_rect.left).abs();
        let height = (target_rect.bottom - target_rect.top).abs();
        
        // Create window hidden (no WS_VISIBLE) to prevent white flash
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            class_name,
            w!(""),
            WS_POPUP,
            target_rect.left, target_rect.top, width, height,
            None, None, instance, None
        );

        // Set initial transparency
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 220, LWA_ALPHA);
        
        // Use DWM Rounded Corners (Windows 11 style)
        let corner_preference = 2u32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(33), // DWMWA_WINDOW_CORNER_PREFERENCE
            &corner_preference as *const _ as *const _,
            size_of::<u32>() as u32
        );
        
        // Force initial paint
        InvalidateRect(hwnd, None, false);
        UpdateWindow(hwnd);
        
        RESULT_HWND = hwnd;
        RESULT_RECT = target_rect;
        
        hwnd
    }
}

pub fn update_result_window(text: &str) {
    unsafe {
        if !IsWindow(RESULT_HWND).as_bool() {
            return;
        }
        
        // Update window text
        let wide_text = to_wstring(text);
        SetWindowTextW(RESULT_HWND, PCWSTR(wide_text.as_ptr()));
        
        // Redraw
        InvalidateRect(RESULT_HWND, None, false);
    }
}

pub fn show_result_window(target_rect: RECT, text: String) {
    unsafe {
        IS_DISMISSING = false;
        DISMISS_ALPHA = 255;
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("TranslationResult");
        
        let mut wc = WNDCLASSW::default();
        if !GetClassInfoW(instance, class_name, &mut wc).as_bool() {
            wc.lpfnWndProc = Some(result_wnd_proc);
            wc.hInstance = instance;
            // Load custom broom cursor
            static BROOM_CURSOR_DATA: &[u8] = include_bytes!("../broom.cur");
            
            let temp_path = std::env::temp_dir().join("broom_cursor.cur");
            if let Ok(()) = std::fs::write(&temp_path, BROOM_CURSOR_DATA) {
                let path_wide: Vec<u16> = temp_path.to_string_lossy()
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let cursor_handle = LoadImageW(
                    None,
                    PCWSTR(path_wide.as_ptr()),
                    IMAGE_CURSOR,
                    0, 0,
                    LR_LOADFROMFILE | LR_DEFAULTSIZE
                );
                wc.hCursor = if let Ok(handle) = cursor_handle {
                    HCURSOR(handle.0)
                } else {
                    LoadCursorW(None, IDC_HAND).unwrap()
                };
            } else {
                wc.hCursor = LoadCursorW(None, IDC_HAND).unwrap();
            }
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            // For layered windows, use NULL background - we paint everything in WM_PAINT
            wc.hbrBackground = HBRUSH(0);
            RegisterClassW(&wc);
        }

        let width = (target_rect.right - target_rect.left).abs();
        let height = (target_rect.bottom - target_rect.top).abs();
        
        // Create window hidden (no WS_VISIBLE) to prevent white flash
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            class_name,
            PCWSTR(to_wstring(&text).as_ptr()),
            WS_POPUP,
            target_rect.left, target_rect.top, width, height,
            None, None, instance, None
        );

        // Set initial transparency
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 220, LWA_ALPHA);
        
        // Use DWM Rounded Corners (Windows 11 style) instead of SetWindowRgn
        // 33 = DWMWA_WINDOW_CORNER_PREFERENCE, 2 = DWMWCP_ROUND
        let corner_preference = 2u32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(33), // DWMWA_WINDOW_CORNER_PREFERENCE
            &corner_preference as *const _ as *const _,
            size_of::<u32>() as u32
        );
        
        // Force initial paint before showing
        InvalidateRect(hwnd, None, false);
        UpdateWindow(hwnd);
        
        // NOW show the window with proper rendering
        ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if !IsWindow(hwnd).as_bool() { break; }
        }
    }
}

unsafe extern "system" fn result_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => {
            // Prevent white flash by not letting Windows erase the background
            LRESULT(1)
        }
        WM_LBUTTONUP => {
            IS_DISMISSING = true;
            SetTimer(hwnd, 2, 8, None); 
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let text_len = GetWindowTextLengthW(hwnd) + 1;
            let mut buf = vec![0u16; text_len as usize];
            GetWindowTextW(hwnd, &mut buf);
            let text = String::from_utf16_lossy(&buf[..text_len as usize - 1]).to_string();
            copy_to_clipboard(&text, hwnd);
            IS_DISMISSING = true;
            SetTimer(hwnd, 2, 8, None);
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == 2 && IS_DISMISSING {
                if DISMISS_ALPHA > 15 {
                    DISMISS_ALPHA = DISMISS_ALPHA.saturating_sub(15);
                    SetLayeredWindowAttributes(hwnd, COLORREF(0), DISMISS_ALPHA, LWA_ALPHA);
                } else {
                    KillTimer(hwnd, 2);
                    DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => { 
            if wparam.0 == VK_ESCAPE.0 as usize { 
                DestroyWindow(hwnd); 
            } else if wparam.0 == 'C' as usize {
                let text_len = GetWindowTextLengthW(hwnd) + 1;
                let mut buf = vec![0u16; text_len as usize];
                GetWindowTextW(hwnd, &mut buf);
                let text = String::from_utf16_lossy(&buf[..text_len as usize - 1]).to_string();
                copy_to_clipboard(&text, hwnd);
            }
            LRESULT(0) 
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect);
            
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            // Double buffering setup
            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bitmap = CreateCompatibleBitmap(hdc, width, height);
            let old_bitmap = SelectObject(mem_dc, mem_bitmap);

            // Paint to memory DC
            let dark_brush = CreateSolidBrush(COLORREF(0x00222222)); // Dark background
            FillRect(mem_dc, &rect, dark_brush);
            DeleteObject(dark_brush);
            
            SetBkMode(mem_dc, TRANSPARENT);
            SetTextColor(mem_dc, COLORREF(0x00FFFFFF)); // White text
            
            let text_len = GetWindowTextLengthW(hwnd) + 1;
            let mut buf = vec![0u16; text_len as usize];
            GetWindowTextW(hwnd, &mut buf);
            
            let padding = 4; 
            let available_w = (width - (padding * 2)).max(1); 
            let available_h = (height - (padding * 2)).max(1);

            // Binary search for optimal font size
            let mut low = 10;
            let mut high = 72;
            let mut optimal_size = 10; 
            let mut text_h = 0;

            while low <= high {
                let mid = (low + high) / 2;
                let hfont = CreateFontW(mid, 0, 0, 0, FW_MEDIUM.0 as i32, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI"));
                let old_font = SelectObject(mem_dc, hfont);
                
                let mut calc_rect = RECT { left: 0, top: 0, right: available_w, bottom: 0 };
                let h = DrawTextW(mem_dc, &mut buf, &mut calc_rect, DT_CALCRECT | DT_WORDBREAK);
                
                SelectObject(mem_dc, old_font);
                DeleteObject(hfont);

                if h <= available_h {
                    optimal_size = mid;
                    text_h = h;
                    low = mid + 1; 
                } else {
                    high = mid - 1; 
                }
            }

            // Draw text
            let hfont = CreateFontW(optimal_size, 0, 0, 0, FW_MEDIUM.0 as i32, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI"));
            let old_font = SelectObject(mem_dc, hfont);

            let offset_y = (available_h - text_h) / 2;
            let mut draw_rect = rect;
            draw_rect.left += padding; 
            draw_rect.right -= padding;
            draw_rect.top += padding + offset_y;
            
            DrawTextW(mem_dc, &mut buf, &mut draw_rect as *mut _, DT_LEFT | DT_WORDBREAK);
            
            SelectObject(mem_dc, old_font);
            DeleteObject(hfont);

            // Copy to screen
            BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY).ok().unwrap();

            // Cleanup
            SelectObject(mem_dc, old_bitmap);
            DeleteObject(mem_bitmap);
            DeleteDC(mem_dc);
            
            EndPaint(hwnd, &mut ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
