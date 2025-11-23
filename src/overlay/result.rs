use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, TRACKMOUSEEVENT, TrackMouseEvent, TME_LEAVE};
use windows::core::*;
use std::mem::size_of;
use std::collections::HashMap;
use std::sync::Mutex;

use super::utils::to_wstring;
use super::selection::load_broom_cursor;

// State for each window
struct WindowState {
    alpha: u8,
    is_hovered: bool,
    copy_success: bool,
    bg_color: u32,
    linked_window: Option<HWND>,
}

// Global map to track state of all active overlay windows
// Using Mutex for safety, though we are mostly single-threaded in UI loop
lazy_static::lazy_static! {
    static ref WINDOW_STATES: Mutex<HashMap<isize, WindowState>> = Mutex::new(HashMap::new());
}

// Configuration for the window being created (passed from process.rs usually)
static mut CURRENT_BG_COLOR: u32 = 0x00222222;

pub enum WindowType {
    Primary,
    Secondary,
}

pub fn link_windows(hwnd1: HWND, hwnd2: HWND) {
    let mut states = WINDOW_STATES.lock().unwrap();
    if let Some(s1) = states.get_mut(&(hwnd1.0 as isize)) {
        s1.linked_window = Some(hwnd2);
    }
    if let Some(s2) = states.get_mut(&(hwnd2.0 as isize)) {
        s2.linked_window = Some(hwnd1);
    }
}

pub fn create_result_window(target_rect: RECT, win_type: WindowType) -> HWND {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("TranslationResult");
        
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
                let padding = 10;
                let screen_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
                let screen_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
                let screen_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
                let screen_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
                let screen_right = screen_x + screen_w;
                let screen_bottom = screen_y + screen_h;

                let right_x = target_rect.right + padding;
                let bottom_y = target_rect.bottom + padding;
                let left_x = target_rect.left - width - padding;
                let top_y = target_rect.top - height - padding;

                let (new_x, new_y) = if right_x + width <= screen_right {
                    (right_x, target_rect.top)
                } else if bottom_y + height <= screen_bottom {
                    (target_rect.left, bottom_y)
                } else if left_x >= screen_x {
                    (left_x, target_rect.top)
                } else if top_y >= screen_y {
                    (target_rect.left, top_y)
                } else {
                    // Fallback to right
                    (right_x, target_rect.top)
                };

                CURRENT_BG_COLOR = 0x002d4a22; // Dark Green-ish
                (new_x, new_y, 0x002d4a22)
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

        // Initialize State
        {
            let mut states = WINDOW_STATES.lock().unwrap();
            states.insert(hwnd.0 as isize, WindowState {
                alpha: 220, // Start alpha
                is_hovered: false,
                copy_success: false,
                bg_color: color,
                linked_window: None,
            });
        }

        SetLayeredWindowAttributes(hwnd, COLORREF(0), 220, LWA_ALPHA);
        
        let corner_preference = 2u32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(33),
            &corner_preference as *const _ as *const _,
            size_of::<u32>() as u32
        );
        
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

unsafe extern "system" fn result_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        
        WM_SETCURSOR => {
            // Check if over copy button
            let cursor_pos = GetMessagePos();
            let mut pt = POINT { x: (cursor_pos & 0xFFFF) as i32, y: ((cursor_pos >> 16) & 0xFFFF) as i32 };
            ScreenToClient(hwnd, &mut pt);

            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            
            let btn_size = 24;
            let btn_rect = RECT { 
                left: width - btn_size, 
                top: height - btn_size, 
                right: width, 
                bottom: height 
            };

            let is_over_btn = pt.x >= btn_rect.left && pt.x <= btn_rect.right && pt.y >= btn_rect.top && pt.y <= btn_rect.bottom;
            
            // Only check if window is hovered (to match paint logic)
            let is_hovered = {
                let states = WINDOW_STATES.lock().unwrap();
                if let Some(state) = states.get(&(hwnd.0 as isize)) {
                    state.is_hovered
                } else {
                    false
                }
            };

            if is_hovered && is_over_btn {
                SetCursor(LoadCursorW(None, IDC_HAND).unwrap());
                return LRESULT(1);
            }
            
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_MOUSEMOVE => {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                if !state.is_hovered {
                    state.is_hovered = true;
                    
                    // Track mouse leave
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut tme);
                    
                    InvalidateRect(hwnd, None, false);
                }
            }
            LRESULT(0)
        }

        0x02A3 => { // WM_MOUSELEAVE
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                state.is_hovered = false;
                InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }

        WM_LBUTTONUP | WM_RBUTTONUP => {
            // Check for Copy Button Click
            let x = (lparam.0 & 0xFFFF) as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i32;
            
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            
            // Button area: bottom-right 24x24
            let btn_size = 24;
            let btn_rect = RECT { 
                left: width - btn_size, 
                top: height - btn_size, 
                right: width, 
                bottom: height 
            };

            let is_copy_click = x >= btn_rect.left && x <= btn_rect.right && y >= btn_rect.top && y <= btn_rect.bottom;

            if is_copy_click || msg == WM_RBUTTONUP {
                let text_len = GetWindowTextLengthW(hwnd) + 1;
                let mut buf = vec![0u16; text_len as usize];
                GetWindowTextW(hwnd, &mut buf);
                let text = String::from_utf16_lossy(&buf[..text_len as usize - 1]).to_string();
                super::utils::copy_to_clipboard(&text, hwnd);
                
                // Show success feedback
                {
                    let mut states = WINDOW_STATES.lock().unwrap();
                    if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                        state.copy_success = true;
                    }
                }
                InvalidateRect(hwnd, None, false);
                SetTimer(hwnd, 1, 1500, None); // 1.5s timer to revert icon

                // If left click on button, don't dismiss
                if is_copy_click && msg == WM_LBUTTONUP {
                    return LRESULT(0);
                }
            }

            // Dismiss THIS window
            SetTimer(hwnd, 2, 8, None); 
            
            // Dismiss LINKED window
            let linked_hwnd = {
                let states = WINDOW_STATES.lock().unwrap();
                if let Some(state) = states.get(&(hwnd.0 as isize)) {
                    state.linked_window
                } else {
                    None
                }
            };

            if let Some(linked) = linked_hwnd {
                if IsWindow(linked).as_bool() {
                    SetTimer(linked, 2, 8, None);
                }
            }

            LRESULT(0)
        }

        WM_TIMER => {
            if wparam.0 == 1 {
                // Revert copy success
                KillTimer(hwnd, 1);
                {
                    let mut states = WINDOW_STATES.lock().unwrap();
                    if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                        state.copy_success = false;
                    }
                }
                InvalidateRect(hwnd, None, false);
            } else if wparam.0 == 2 {
                // Fade out
                let mut should_destroy = false;
                {
                    let mut states = WINDOW_STATES.lock().unwrap();
                    if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                        if state.alpha > 15 {
                            state.alpha = state.alpha.saturating_sub(15);
                            SetLayeredWindowAttributes(hwnd, COLORREF(0), state.alpha, LWA_ALPHA);
                        } else {
                            should_destroy = true;
                        }
                    }
                }

                if should_destroy {
                    KillTimer(hwnd, 2);
                    PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            LRESULT(0)
        }

        WM_KEYDOWN => { 
            if wparam.0 == VK_ESCAPE.0 as usize { 
                PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                 // Dismiss LINKED window
                let linked_hwnd = {
                    let states = WINDOW_STATES.lock().unwrap();
                    if let Some(state) = states.get(&(hwnd.0 as isize)) {
                        state.linked_window
                    } else {
                        None
                    }
                };
                if let Some(linked) = linked_hwnd {
                    if IsWindow(linked).as_bool() {
                        PostMessageW(linked, WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                }

            } else if wparam.0 == 'C' as usize {
                let text_len = GetWindowTextLengthW(hwnd) + 1;
                let mut buf = vec![0u16; text_len as usize];
                GetWindowTextW(hwnd, &mut buf);
                let text = String::from_utf16_lossy(&buf[..text_len as usize - 1]).to_string();
                super::utils::copy_to_clipboard(&text, hwnd);
                
                 // Show success feedback
                {
                    let mut states = WINDOW_STATES.lock().unwrap();
                    if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                        state.copy_success = true;
                    }
                }
                InvalidateRect(hwnd, None, false);
                SetTimer(hwnd, 1, 1500, None);
            }
            LRESULT(0) 
        }

        WM_DESTROY => {
            let mut states = WINDOW_STATES.lock().unwrap();
            states.remove(&(hwnd.0 as isize));
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

            // Retrieve state
            let (bg_color, is_hovered, copy_success) = {
                let states = WINDOW_STATES.lock().unwrap();
                if let Some(state) = states.get(&(hwnd.0 as isize)) {
                    (state.bg_color, state.is_hovered, state.copy_success)
                } else {
                    (0x00222222, false, false)
                }
            };

            let dark_brush = CreateSolidBrush(COLORREF(bg_color));
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

            // Simple Font Auto-size logic
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

            // --- DRAW COPY BUTTON (Only if hovered) ---
            if is_hovered {
                let btn_size = 24;
                let btn_rect = RECT { 
                    left: width - btn_size, 
                    top: height - btn_size, 
                    right: width, 
                    bottom: height 
                };
                
                // Button BG
                let btn_brush = CreateSolidBrush(COLORREF(0x00444444));
                FillRect(mem_dc, &btn_rect, btn_brush);
                DeleteObject(btn_brush);

                // Icon Color
                let icon_pen = if copy_success {
                    CreatePen(PS_SOLID, 2, COLORREF(0x0000FF00)) // Green
                } else {
                    CreatePen(PS_SOLID, 2, COLORREF(0x00AAAAAA)) // Gray
                };
                let old_pen = SelectObject(mem_dc, icon_pen);

                if copy_success {
                    // Draw Checkmark
                    MoveToEx(mem_dc, btn_rect.left + 6, btn_rect.top + 12, None);
                    LineTo(mem_dc, btn_rect.left + 10, btn_rect.top + 16);
                    LineTo(mem_dc, btn_rect.left + 18, btn_rect.top + 8);
                } else {
                    // Draw Clipboard Icon
                    // Main board
                    Rectangle(mem_dc, btn_rect.left + 6, btn_rect.top + 6, btn_rect.right - 6, btn_rect.bottom - 4);
                    // Clip (top small rect)
                    Rectangle(mem_dc, btn_rect.left + 9, btn_rect.top + 4, btn_rect.right - 9, btn_rect.top + 8);
                    // Lines inside
                    MoveToEx(mem_dc, btn_rect.left + 9, btn_rect.top + 10, None);
                    LineTo(mem_dc, btn_rect.right - 9, btn_rect.top + 10);
                    MoveToEx(mem_dc, btn_rect.left + 9, btn_rect.top + 14, None);
                    LineTo(mem_dc, btn_rect.right - 9, btn_rect.top + 14);
                }

                SelectObject(mem_dc, old_pen);
                DeleteObject(icon_pen);
            }

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
