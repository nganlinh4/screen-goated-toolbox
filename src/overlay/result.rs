use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::core::*;
use std::mem::size_of;

use super::utils::to_wstring;
use super::selection::load_broom_cursor;

// We support up to 2 windows: Primary and Secondary
static mut PRIMARY_HWND: HWND = HWND(0);
static mut SECONDARY_HWND: HWND = HWND(0);

// State tracking
static mut IS_DISMISSING: bool = false;
static mut DISMISS_ALPHA: u8 = 255;

// Configuration for the window being created
static mut CURRENT_BG_COLOR: u32 = 0x00222222;

pub enum WindowType {
    Primary,
    Secondary,
}

pub fn create_result_window(target_rect: RECT, win_type: WindowType) -> HWND {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("TranslationResult");
        
        // Reset dismiss state when creating new primary
        if matches!(win_type, WindowType::Primary) {
            IS_DISMISSING = false;
            DISMISS_ALPHA = 255;
            // Close existing secondary if any
            if IsWindow(SECONDARY_HWND).as_bool() { DestroyWindow(SECONDARY_HWND); }
            if IsWindow(PRIMARY_HWND).as_bool() { DestroyWindow(PRIMARY_HWND); }
        }

        let mut wc = WNDCLASSW::default();
        if !GetClassInfoW(instance, class_name, &mut wc).as_bool() {
            wc.lpfnWndProc = Some(result_wnd_proc);
            wc.hInstance = instance;
            wc.hCursor = load_broom_cursor();
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            wc.hbrBackground = HBRUSH(0);
            RegisterClassW(&wc);
        }

        let width = (target_rect.right - target_rect.left).abs();
        let height = (target_rect.bottom - target_rect.top).abs();
        
        // Determine position and color
        let (x, y, color) = match win_type {
            WindowType::Primary => {
                CURRENT_BG_COLOR = 0x00222222; // Dark Gray
                (target_rect.left, target_rect.top, 0x00222222)
            },
            WindowType::Secondary => {
                // Position to the right of Primary by default
                // TODO: Check screen bounds. For now, just +width + 10 padding
                let padding = 10;
                let new_x = target_rect.right + padding;
                CURRENT_BG_COLOR = 0x002d4a22; // Dark Green-ish for distinction
                (new_x, target_rect.top, 0x002d4a22)
            }
        };

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            class_name,
            w!(""),
            WS_POPUP,
            x, y, width, height,
            None, None, instance, None
        );

        // Store color in UserData or a map? 
        // Simpler: We only paint in the wnd_proc. 
        // We need to know WHICH window acts which way.
        // Let's use SetWindowLongPtr with GWLP_USERDATA to store the color.
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, color as isize);

        SetLayeredWindowAttributes(hwnd, COLORREF(0), 220, LWA_ALPHA);
        
        let corner_preference = 2u32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(33),
            &corner_preference as *const _ as *const _,
            size_of::<u32>() as u32
        );
        
        match win_type {
            WindowType::Primary => PRIMARY_HWND = hwnd,
            WindowType::Secondary => SECONDARY_HWND = hwnd,
        }
        
        InvalidateRect(hwnd, None, false);
        UpdateWindow(hwnd);
        
        hwnd
    }
}

pub fn update_window_text(hwnd: HWND, text: &str) {
    unsafe {
        if !IsWindow(hwnd).as_bool() { return; }
        let wide_text = to_wstring(text);
        SetWindowTextW(hwnd, PCWSTR(wide_text.as_ptr()));
        InvalidateRect(hwnd, None, false);
    }
}

// Helper for single-shot error/status windows (always Primary)
pub fn show_result_window(target_rect: RECT, text: String) {
    let hwnd = create_result_window(target_rect, WindowType::Primary);
    update_window_text(hwnd, &text);
    unsafe { ShowWindow(hwnd, SW_SHOW); }
    
    unsafe {
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
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONUP | WM_RBUTTONUP => {
            // Dismiss BOTH windows on click
            IS_DISMISSING = true;
            SetTimer(PRIMARY_HWND, 2, 8, None); 
            
            // Copy text if Right Click or 'C' (handled in logic)
            if msg == WM_RBUTTONUP {
                let text_len = GetWindowTextLengthW(hwnd) + 1;
                let mut buf = vec![0u16; text_len as usize];
                GetWindowTextW(hwnd, &mut buf);
                let text = String::from_utf16_lossy(&buf[..text_len as usize - 1]).to_string();
                super::utils::copy_to_clipboard(&text, hwnd);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == 2 && IS_DISMISSING {
                if DISMISS_ALPHA > 15 {
                    DISMISS_ALPHA = DISMISS_ALPHA.saturating_sub(15);
                    // Fade both
                    if IsWindow(PRIMARY_HWND).as_bool() {
                        SetLayeredWindowAttributes(PRIMARY_HWND, COLORREF(0), DISMISS_ALPHA, LWA_ALPHA);
                    }
                    if IsWindow(SECONDARY_HWND).as_bool() {
                        SetLayeredWindowAttributes(SECONDARY_HWND, COLORREF(0), DISMISS_ALPHA, LWA_ALPHA);
                    }
                } else {
                    KillTimer(hwnd, 2);
                    if IsWindow(SECONDARY_HWND).as_bool() { DestroyWindow(SECONDARY_HWND); }
                    if IsWindow(PRIMARY_HWND).as_bool() { DestroyWindow(PRIMARY_HWND); }
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => { 
            if wparam.0 == VK_ESCAPE.0 as usize { 
                // Destroy both
                if IsWindow(SECONDARY_HWND).as_bool() { DestroyWindow(SECONDARY_HWND); }
                if IsWindow(PRIMARY_HWND).as_bool() { DestroyWindow(PRIMARY_HWND); }
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

            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bitmap = CreateCompatibleBitmap(hdc, width, height);
            let old_bitmap = SelectObject(mem_dc, mem_bitmap);

            // Retrieve stored color
            let bg_color_val = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as u32;
            let dark_brush = CreateSolidBrush(COLORREF(bg_color_val));
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

            // Simple Font Auto-size logic (Simplified for brevity)
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
            BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY).ok().unwrap();
            SelectObject(mem_dc, old_bitmap);
            DeleteObject(mem_bitmap);
            DeleteDC(mem_dc);
            
            EndPaint(hwnd, &mut ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
