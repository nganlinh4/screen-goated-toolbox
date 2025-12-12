use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::core::*;
use std::sync::Once;

const EM_SETSEL: u32 = 0x00B1;

static REGISTER_INPUT_CLASS: Once = Once::new();
static mut INPUT_HWND: HWND = HWND(0);
static mut ORIGINAL_EDIT_PROC: Option<WNDPROC> = None;

// Callback storage
type SubmitCallback = Box<dyn Fn(String, HWND) + Send>;
static mut ON_SUBMIT: Option<SubmitCallback> = None;

const WIN_W: i32 = 600;
const WIN_H: i32 = 250;

// Colors
const COL_DARK_BG: u32 = 0x202020; // RGB(32, 32, 32)
const COL_WHITE: u32 = 0xFFFFFF;

pub fn is_active() -> bool {
    unsafe { INPUT_HWND.0 != 0 }
}

pub fn cancel_input() {
    unsafe {
        if INPUT_HWND.0 != 0 {
            PostMessageW(INPUT_HWND, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn show(_prompt_guide: String, on_submit: impl Fn(String, HWND) + Send + 'static) {
    unsafe {
        if INPUT_HWND.0 != 0 {
            SetForegroundWindow(INPUT_HWND);
            return;
        }

        ON_SUBMIT = Some(Box::new(on_submit));

        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("SGT_TextInput");

        REGISTER_INPUT_CLASS.call_once(|| {
            let mut wc = WNDCLASSW::default();
            wc.lpfnWndProc = Some(input_wnd_proc);
            wc.hInstance = instance;
            wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap();
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            // Create a brush for the background
            wc.hbrBackground = CreateSolidBrush(COLORREF(COL_DARK_BG)); 
            let _ = RegisterClassW(&wc);
        });

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_w - WIN_W) / 2;
        let y = (screen_h - WIN_H) / 2;

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name,
            w!("Text Input"),
            WS_POPUP,
            x, y, WIN_W, WIN_H,
            None, None, instance, None
        );
        INPUT_HWND = hwnd;

        SetLayeredWindowAttributes(hwnd, COLORREF(0), 245, LWA_ALPHA);

        // Window Rounded Corners
        let rgn = CreateRoundRectRgn(0, 0, WIN_W, WIN_H, 16, 16);
        SetWindowRgn(hwnd, rgn, true);

        // Edit Box Dimensions (Inset from window)
        let edit_x = 20;
        let edit_y = 50;
        let edit_w = WIN_W - 40;
        let edit_h = WIN_H - 90;

        // Create Edit Control (Borderless, we draw the white bg manually)
        let edit_style = WS_CHILD | WS_VISIBLE | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32);
        let h_edit = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            w!(""),
            edit_style,
            edit_x + 5, edit_y + 5, edit_w - 10, edit_h - 10, // Padding inside the white box
            hwnd,
            HMENU(101),
            instance,
            None
        );

        // Set Font
        let h_font_edit = CreateFontW(16, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI"));
        SendMessageW(h_edit, WM_SETFONT, WPARAM(h_font_edit.0 as usize), LPARAM(1));

        // Subclass Edit Control for key handling
        let old_proc = SetWindowLongPtrW(h_edit, GWLP_WNDPROC, edit_subclass_proc as *const () as isize);
        ORIGINAL_EDIT_PROC = Some(std::mem::transmute(old_proc));

        SetFocus(h_edit);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        // Message Loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if !IsWindow(hwnd).as_bool() { break; }
        }

        DeleteObject(h_font_edit);
        INPUT_HWND = HWND(0);
        ON_SUBMIT = None;
        ORIGINAL_EDIT_PROC = None;
    }
}

// Subclass procedure for the Edit control
unsafe extern "system" fn edit_subclass_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_KEYDOWN => {
            let vk = wparam.0 as i32;
            
            // ENTER: Submit
            if vk == VK_RETURN.0 as i32 {
                let shift = (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
                if !shift {
                    // Get text and submit
                    let len = GetWindowTextLengthW(hwnd) + 1;
                    let mut buf = vec![0u16; len as usize];
                    GetWindowTextW(hwnd, &mut buf);
                    let text = String::from_utf16_lossy(&buf[..len as usize - 1]);
                    
                    if !text.trim().is_empty() {
                        if let Some(cb) = ON_SUBMIT.as_ref() {
                            let parent = GetParent(hwnd);
                            DestroyWindow(parent); // Close UI first
                            cb(text, parent); // Callback
                        }
                    }
                    return LRESULT(0); // Swallow key
                }
            }
            
            // ESCAPE: Cancel
            if vk == VK_ESCAPE.0 as i32 {
                let parent = GetParent(hwnd);
                DestroyWindow(parent);
                return LRESULT(0); // Don't PostQuitMessage, let the main loop detect window closure
            }

            // CTRL+A: Select All
            if vk == 0x41 { // 'A'
                if (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 {
                    SendMessageW(hwnd, EM_SETSEL, WPARAM(0), LPARAM(-1));
                    return LRESULT(0);
                }
            }
        }
        WM_CHAR => {
            // Swallow the Enter character to prevent newline/beep if Shift not held
            if wparam.0 == VK_RETURN.0 as usize && (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) == 0 {
                return LRESULT(0);
            }
        }
        _ => {}
    }

    // Call original proc for everything else (typing, arrows, etc.)
    if let Some(proc) = ORIGINAL_EDIT_PROC {
        CallWindowProcW(proc, hwnd, msg, wparam, lparam)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

unsafe extern "system" fn input_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam.0 as isize);
            SetBkMode(hdc, OPAQUE);
            SetBkColor(hdc, COLORREF(COL_WHITE)); // White bg for text
            SetTextColor(hdc, COLORREF(0x000000)); // Black text
            // Return handle to white brush for edit control background
            let hbrush = GetStockObject(WHITE_BRUSH);
            LRESULT(hbrush.0 as isize)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect);
            let w = rect.right;
            let h = rect.bottom;

            // Use double buffering
            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bmp = CreateCompatibleBitmap(hdc, w, h);
            let old_bmp = SelectObject(mem_dc, mem_bmp);

            // 1. Draw Window Background (Dark)
            let brush_bg = CreateSolidBrush(COLORREF(COL_DARK_BG));
            FillRect(mem_dc, &rect, brush_bg);
            DeleteObject(brush_bg);

            // 2. Draw Rounded White Input Area
            // Coordinates matching the edit control padding
            let edit_x = 20;
            let edit_y = 50;
            let edit_w = w - 40;
            let edit_h = h - 90;
            
            let brush_white = CreateSolidBrush(COLORREF(COL_WHITE));
            let old_brush = SelectObject(mem_dc, brush_white);
            let pen_null = GetStockObject(NULL_PEN);
            let old_pen = SelectObject(mem_dc, pen_null);
            
            RoundRect(mem_dc, edit_x, edit_y, edit_x + edit_w, edit_y + edit_h, 12, 12);
            
            SelectObject(mem_dc, old_pen);
            SelectObject(mem_dc, old_brush);
            DeleteObject(brush_white);

            // 3. Draw Text Labels
            SetBkMode(mem_dc, TRANSPARENT);
            SetTextColor(mem_dc, COLORREF(0x00FFFFFF)); // White Text
            
            let h_font = CreateFontW(19, 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI"));
            let old_font = SelectObject(mem_dc, h_font);
            
            let title = "Enter text to process:";
            let mut title_w = crate::overlay::utils::to_wstring(title);
            let mut r_title = RECT { left: 20, top: 15, right: w - 20, bottom: 45 };
            DrawTextW(mem_dc, &mut title_w, &mut r_title, DT_LEFT | DT_SINGLELINE);
            
            let h_font_small = CreateFontW(13, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI"));
            SelectObject(mem_dc, h_font_small);
            SetTextColor(mem_dc, COLORREF(0x00AAAAAA)); // Light Grey
            
            let hint = "Enter to Submit  |  Shift+Enter for New Line  |  Esc to Cancel";
            let mut hint_w = crate::overlay::utils::to_wstring(hint);
            let mut r_hint = RECT { left: 20, top: h - 30, right: w - 20, bottom: h - 5 };
            DrawTextW(mem_dc, &mut hint_w, &mut r_hint, DT_CENTER | DT_SINGLELINE);

            SelectObject(mem_dc, old_font);
            DeleteObject(h_font);
            DeleteObject(h_font_small);

            BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
            SelectObject(mem_dc, old_bmp);
            DeleteObject(mem_bmp);
            DeleteDC(mem_dc);
            
            EndPaint(hwnd, &mut ps);
            LRESULT(0)
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
