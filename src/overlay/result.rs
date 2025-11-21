use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::core::*;
use std::mem::size_of;

use super::utils::to_wstring;

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
            static BROOM_CURSOR_DATA: &[u8] = include_bytes!("../../broom.cur");
            
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
            static BROOM_CURSOR_DATA: &[u8] = include_bytes!("../../broom.cur");
            
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
            super::utils::copy_to_clipboard(&text, hwnd);
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
                super::utils::copy_to_clipboard(&text, hwnd);
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
