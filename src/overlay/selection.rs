use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetCapture, ReleaseCapture, VK_ESCAPE};
use windows::core::*;

use super::process::process_and_close;
use crate::{APP};

pub fn load_broom_cursor() -> HCURSOR {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        
        // Try to load from embedded resource (ID 101)
        if let Ok(hcursor) = LoadCursorW(instance, PCWSTR(101 as *const u16)) {
            if !hcursor.is_invalid() {
                return hcursor;
            }
        }
        
        // Fallback to standard crosshair cursor if resource not found
        LoadCursorW(None, IDC_CROSS).unwrap_or(HCURSOR(0))
    }
}

// --- DATA CRUNCH EFFECT STATE ---
struct GlitchParticle {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    life: f32,
    color: u32, // COLORREF format: 0x00BBGGRR
}

static mut PARTICLES: Vec<GlitchParticle> = Vec::new();
static mut RNG_SEED: u32 = 54321;

unsafe fn rand_range(min: i32, max: i32) -> i32 {
    if min >= max { return min; }
    RNG_SEED = RNG_SEED.wrapping_mul(1103515245).wrapping_add(12345);
    let val = (RNG_SEED / 65536) as i32;
    min + (val.abs() % (max - min))
}

unsafe fn rand_float() -> f32 {
    RNG_SEED = RNG_SEED.wrapping_mul(1103515245).wrapping_add(12345);
    (RNG_SEED as f32) / (u32::MAX as f32)
}

static mut START_POS: POINT = POINT { x: 0, y: 0 };
static mut CURR_POS: POINT = POINT { x: 0, y: 0 };
static mut IS_DRAGGING: bool = false;
static mut IS_PROCESSING: bool = false;
static mut SCAN_LINE_Y: i32 = 0;
static mut SCAN_DIR: i32 = 5;
static mut SELECTION_OVERLAY_ACTIVE: bool = false;
static mut SELECTION_OVERLAY_HWND: HWND = HWND(0);
static mut CURRENT_PRESET_IDX: usize = 0;

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

pub fn show_selection_overlay(preset_idx: usize) {
    unsafe {
        CURRENT_PRESET_IDX = preset_idx;
        SELECTION_OVERLAY_ACTIVE = true;
        PARTICLES.clear(); // Reset particles
        
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
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("Snipping"),
            WS_POPUP | WS_VISIBLE,
            x, y, w, h,
            None, None, instance, None
        );

        SELECTION_OVERLAY_HWND = hwnd;

        SetLayeredWindowAttributes(hwnd, COLORREF(0), 100, LWA_ALPHA);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if msg.message == WM_CLOSE { break; }
        }
        
        SELECTION_OVERLAY_ACTIVE = false;
        SELECTION_OVERLAY_HWND = HWND(0);
        PARTICLES.clear();
        
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
                    // Start timer for animation (30ms ~ 33fps)
                    SetTimer(hwnd, 1, 30, None);
                    
                    let app_clone = APP.clone();
                    let p_idx = CURRENT_PRESET_IDX;
                    std::thread::spawn(move || {
                        process_and_close(app_clone, rect, hwnd, p_idx);
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
                
                // Update Scan Line
                SCAN_LINE_Y += SCAN_DIR;
                if SCAN_LINE_Y > rect.bottom || SCAN_LINE_Y < rect.top {
                    SCAN_DIR = -SCAN_DIR;
                }

                // --- DATA CRUNCH PARTICLES ---
                // Spawn new
                let region_w = rect.right - rect.left;
                if region_w > 0 {
                    let count = rand_range(1, 4); // 1-3 particles per frame
                    for _ in 0..count {
                        let w = rand_range(2, 6);
                        let h = rand_range(5, 15);
                        let x = rand_range(rect.left, rect.right - w);
                        
                        // Mix of Matrix Green and Cyan
                        let is_cyan = rand_float() > 0.7;
                        let color = if is_cyan { 0x00FFFF00 } else { 0x0000FF00 }; // BGR

                        PARTICLES.push(GlitchParticle {
                            x,
                            y: SCAN_LINE_Y,
                            w,
                            h,
                            life: 1.0,
                            color,
                        });
                    }
                }

                // Update
                let mut keep = Vec::with_capacity(PARTICLES.len());
                for mut p in PARTICLES.drain(..) {
                    p.life -= 0.15; // Fast decay
                    if p.life > 0.0 {
                        keep.push(p);
                    }
                }
                PARTICLES = keep;

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

            // Clear Background
            let brush = CreateSolidBrush(COLORREF(0x00000000));
            let full_rect = RECT { left: 0, top: 0, right: width, bottom: height };
            FillRect(mem_dc, &full_rect, brush);
            DeleteObject(brush);

            if IS_DRAGGING || IS_PROCESSING {
                let rect_abs = RECT {
                    left: START_POS.x.min(CURR_POS.x),
                    top: START_POS.y.min(CURR_POS.y),
                    right: START_POS.x.max(CURR_POS.x),
                    bottom: START_POS.y.max(CURR_POS.y),
                };

                let screen_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
                let screen_y = GetSystemMetrics(SM_YVIRTUALSCREEN);

                let rect_rel = RECT {
                    left: rect_abs.left - screen_x,
                    top: rect_abs.top - screen_y,
                    right: rect_abs.right - screen_x,
                    bottom: rect_abs.bottom - screen_y,
                };
                
                // Draw Selection Frame
                let frame_brush = CreateSolidBrush(COLORREF(0x00FFFFFF));
                FrameRect(mem_dc, &rect_rel, frame_brush);
                DeleteObject(frame_brush);
                
                if IS_PROCESSING {
                    // Draw Glitch Particles (Behind the line)
                    for p in &PARTICLES {
                        // Simple opacity simulation: flip color if life is low, or just draw smaller
                        let draw_h = if p.life < 0.5 { p.h / 2 } else { p.h };
                        let p_rect = RECT {
                            left: p.x - screen_x,
                            top: p.y - screen_y,
                            right: p.x - screen_x + p.w,
                            bottom: p.y - screen_y + draw_h,
                        };
                        let p_brush = CreateSolidBrush(COLORREF(p.color));
                        FillRect(mem_dc, &p_rect, p_brush);
                        DeleteObject(p_brush);
                    }

                    // Draw Main Scan Line
                    let scan_y_rel = SCAN_LINE_Y - screen_y;
                    
                    // Glow/Outer (Green)
                    let line_outer = RECT {
                        left: rect_rel.left + 1,
                        top: scan_y_rel - 1,
                        right: rect_rel.right - 1,
                        bottom: scan_y_rel + 1
                    };
                    let scan_brush = CreateSolidBrush(COLORREF(0x0000FF00));
                    FillRect(mem_dc, &line_outer, scan_brush);
                    DeleteObject(scan_brush);
                    
                    // Core (White)
                    let line_core = RECT {
                        left: rect_rel.left + 1,
                        top: scan_y_rel,
                        right: rect_rel.right - 1,
                        bottom: scan_y_rel + 1
                    };
                    let white_brush = CreateSolidBrush(COLORREF(0x00FFFFFF));
                    FillRect(mem_dc, &line_core, white_brush);
                    DeleteObject(white_brush);
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
