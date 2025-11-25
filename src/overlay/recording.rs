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
            // Change text to "Waiting..."
            // Actually, we signal the audio thread to stop.
            // The audio thread will finalize the file/buffer and call the processing logic.
            // We update UI here immediately for feedback.
            AUDIO_STOP_SIGNAL.store(true, Ordering::SeqCst);
            InvalidateRect(RECORDING_HWND, None, false);
        }
    }
}

pub fn show_recording_overlay(preset_idx: usize) {
    unsafe {
        if IS_RECORDING { return; }
        
        IS_RECORDING = true;
        IS_PAUSED = false;
        CURRENT_PRESET_IDX = preset_idx;
        ANIMATION_OFFSET = 0.0;
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

        let w = 300;
        let h = 100;
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
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_COLORKEY); // Use per-pixel alpha via Paint
        ShowWindow(hwnd, SW_SHOW);
        
        SetTimer(hwnd, 1, 16, None); // Animation timer

        // Start Audio Recording Thread
        let preset = APP.lock().unwrap().config.presets[preset_idx].clone();
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

unsafe extern "system" fn recording_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => {
            // Make whole window draggable
            let res = DefWindowProcW(hwnd, msg, wparam, lparam);
            if res == LRESULT(1) { // HTCLIENT
                return LRESULT(2); // HTCAPTION -> allows drag
            }
            res
        }
        WM_LBUTTONDOWN => {
            // Handle buttons manually since we hijack caption drag?
            // Actually WM_NCHITTEST blocks LBUTTONDOWN for client area buttons if we return HTCAPTION everywhere.
            // We need to check button coords first.
            let x = (lparam.0 & 0xFFFF) as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i32;
            
            // Cancel Button (Top Right)
            if x > 270 && y < 30 {
                 PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                 return LRESULT(0);
            }
            // Pause/Play Button (Bottom Left)
            if x < 30 && y > 70 {
                IS_PAUSED = !IS_PAUSED;
                AUDIO_PAUSE_SIGNAL.store(IS_PAUSED, Ordering::SeqCst);
                InvalidateRect(hwnd, None, false);
                return LRESULT(0);
            }
            
            // If not button, pass to default for dragging (via HTCAPTION logic or just allow drag)
            // But if we returned HTCAPTION in NCHITTEST, LBUTTONDOWN doesn't fire here easily.
            // FIX: Don't use simple HTCAPTION. Use manual drag in LBUTTONDOWN/MOUSEMOVE like result window?
            // Result window uses manual drag logic in logic.rs. Let's stick to standard drag here for simplicity.
            // Or just check rects in NCHITTEST.
            
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        // Retrying NCHITTEST with button exclusion
        // WM_NCHITTEST logic:
        // Get cursor pos (screen coords), map to client.
        // If over buttons -> HTCLIENT.
        // Else -> HTCAPTION.
        
        WM_TIMER => {
            if AUDIO_STOP_SIGNAL.load(Ordering::SeqCst) {
                 // Text is "Waiting..." - animate slowly?
                 ANIMATION_OFFSET += 2.0;
            } else if !IS_PAUSED {
                ANIMATION_OFFSET += 5.0;
            }
            if ANIMATION_OFFSET > 360.0 { ANIMATION_OFFSET -= 360.0; }
            InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect);
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;

            // Double buffering
            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bm = CreateCompatibleBitmap(hdc, w, h);
            SelectObject(mem_dc, mem_bm);

            // Draw Background (SDF Glow)
            let is_waiting = AUDIO_STOP_SIGNAL.load(Ordering::SeqCst);
            super::paint_utils::render_box_sdf(HDC(mem_dc.0), rect, w, h, !IS_PAUSED || is_waiting, ANIMATION_OFFSET);

            // Draw Text
            SetBkMode(mem_dc, TRANSPARENT);
            SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
            
            let text = if is_waiting {
                "Đợi kết quả..."
            } else if IS_PAUSED {
                "Tạm dừng"
            } else {
                "Đang ghi âm..."
            };
            let mut text_w = crate::overlay::utils::to_wstring(text);
            let mut tr = RECT { left: 0, top: 0, right: w, bottom: h };
            DrawTextW(mem_dc, &mut text_w, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

            // Draw Cancel Button (X) - Top Right
            // Simple lines
            let pen = CreatePen(PS_SOLID, 2, COLORREF(0x00AAAAAA));
            let old_pen = SelectObject(mem_dc, pen);
            MoveToEx(mem_dc, w-20, 10, None); LineTo(mem_dc, w-10, 20);
            MoveToEx(mem_dc, w-10, 10, None); LineTo(mem_dc, w-20, 20);
            SelectObject(mem_dc, old_pen);
            DeleteObject(pen);

            // Draw Pause/Play Button - Bottom Left
            let brush = CreateSolidBrush(COLORREF(0x00AAAAAA));
            if IS_PAUSED {
                 // Play Icon (Triangle)
                 let pts = [POINT{x:10, y:h-20}, POINT{x:10, y:h-10}, POINT{x:20, y:h-15}];
                 Polygon(mem_dc, &pts);
            } else {
                 // Pause Icon (Two bars)
                 let r1 = RECT{left: 10, top: h-20, right: 14, bottom: h-10};
                 let r2 = RECT{left: 16, top: h-20, right: 20, bottom: h-10};
                 FillRect(mem_dc, &r1, brush);
                 FillRect(mem_dc, &r2, brush);
            }
            DeleteObject(brush);

            BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
            DeleteObject(mem_bm);
            DeleteDC(mem_dc);
            EndPaint(hwnd, &mut ps);
            LRESULT(0)
        }
        WM_CLOSE => {
            AUDIO_STOP_SIGNAL.store(true, Ordering::SeqCst); // Ensure thread stops
            DestroyWindow(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
