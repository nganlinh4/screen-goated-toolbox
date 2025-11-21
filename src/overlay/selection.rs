use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetCapture, ReleaseCapture, VK_ESCAPE};
use windows::core::*;

use super::process::process_and_close;
use crate::{APP};

static mut START_POS: POINT = POINT { x: 0, y: 0 };
static mut CURR_POS: POINT = POINT { x: 0, y: 0 };
static mut IS_DRAGGING: bool = false;
static mut IS_PROCESSING: bool = false;
static mut SCAN_LINE_Y: i32 = 0;
static mut SCAN_DIR: i32 = 5;
static mut SELECTION_OVERLAY_ACTIVE: bool = false;
static mut SELECTION_OVERLAY_HWND: HWND = HWND(0);

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
