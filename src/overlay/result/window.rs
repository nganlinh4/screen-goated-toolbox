use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::System::LibraryLoader::*;
use windows::core::*;
use std::mem::size_of;
use std::sync::Once;

use super::state::{WINDOW_STATES, WindowState, CursorPhysics, InteractionMode, ResizeEdge, RefineContext, WindowType};
use super::event_handler::result_wnd_proc;

static mut CURRENT_BG_COLOR: u32 = 0x00222222;
static REGISTER_RESULT_CLASS: Once = Once::new();

// Helper to apply rounded corners to the edit control
unsafe fn set_rounded_edit_region(h_edit: HWND, w: i32, h: i32) {
    // radius (12, 12) matches the overlay style
    let rgn = CreateRoundRectRgn(0, 0, w, h, 12, 12);
    SetWindowRgn(h_edit, rgn, true);
}

pub fn create_result_window(
    target_rect: RECT,
    win_type: WindowType,
    context: RefineContext,
    model_id: String,
    provider: String,
    streaming_enabled: bool
) -> HWND {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("TranslationResult");
        
        REGISTER_RESULT_CLASS.call_once(|| {
            let mut wc = WNDCLASSW::default();
            wc.lpfnWndProc = Some(result_wnd_proc);
            wc.hInstance = instance;
            wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap(); 
            wc.lpszClassName = class_name;
            wc.style = CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS; 
            wc.hbrBackground = HBRUSH(0);
            let _ = RegisterClassW(&wc);
        });

        let width = (target_rect.right - target_rect.left).abs();
        let height = (target_rect.bottom - target_rect.top).abs();
        
        let (x, y, color) = match win_type {
            WindowType::Primary => {
                CURRENT_BG_COLOR = 0x00222222; 
                (target_rect.left, target_rect.top, 0x00222222)
            },
            WindowType::SecondaryExplicit => {
                CURRENT_BG_COLOR = 0x002d4a22; 
                (target_rect.left, target_rect.top, 0x002d4a22)
            },
            WindowType::Secondary => {
                let padding = 10;
                let hmonitor = MonitorFromRect(&target_rect, MONITOR_DEFAULTTONEAREST);
                let mut mi = MONITORINFO::default();
                mi.cbSize = size_of::<MONITORINFO>() as u32;
                GetMonitorInfoW(hmonitor, &mut mi);
                let work_rect = mi.rcWork;

                let pos_right_x = target_rect.right + padding;
                let pos_left_x  = target_rect.left - width - padding;
                let pos_bottom_y = target_rect.bottom + padding;
                let pos_top_y    = target_rect.top - height - padding;

                let space_right  = work_rect.right - pos_right_x;
                let space_left   = (target_rect.left - padding) - work_rect.left;
                let space_bottom = work_rect.bottom - pos_bottom_y;
                let space_top    = (target_rect.top - padding) - work_rect.top;

                let (mut best_x, mut best_y) = if space_right >= width {
                    (pos_right_x, target_rect.top)
                } else if space_bottom >= height {
                    (target_rect.left, pos_bottom_y)
                } else if space_left >= width {
                    (pos_left_x, target_rect.top)
                } else if space_top >= height {
                    (target_rect.left, pos_top_y)
                } else {
                    let max_space = space_right.max(space_left).max(space_bottom).max(space_top);
                    if max_space == space_right { (pos_right_x, target_rect.top) } 
                    else if max_space == space_left { (pos_left_x, target_rect.top) } 
                    else if max_space == space_bottom { (target_rect.left, pos_bottom_y) } 
                    else { (target_rect.left, pos_top_y) }
                };
                
                let safe_w = width.min(work_rect.right - work_rect.left);
                let safe_h = height.min(work_rect.bottom - work_rect.top);
                best_x = best_x.clamp(work_rect.left, work_rect.right - safe_w);
                best_y = best_y.clamp(work_rect.top, work_rect.bottom - safe_h);

                CURRENT_BG_COLOR = 0x002d4a22; 
                (best_x, best_y, 0x002d4a22)
            }
        };

        // WS_CLIPCHILDREN prevents parent from drawing over child (Fixes Blinking)
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            class_name,
            w!(""),
            WS_POPUP | WS_CLIPCHILDREN, 
            x, y, width, height,
            None, None, instance, None
        );

        let edit_style = WINDOW_STYLE(
            WS_CHILD.0 | 
            WS_BORDER.0 | 
            WS_CLIPSIBLINGS.0 |
            (ES_MULTILINE as u32) |
            (ES_AUTOVSCROLL as u32)
        );
        
        let h_edit = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("EDIT"),
            w!(""),
            edit_style,
            0, 0, 0, 0, // Sized dynamically
            hwnd,
            HMENU(101),
            instance,
            None
        );
        
        let hfont = CreateFontW(14, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32, (VARIABLE_PITCH.0 | FF_SWISS.0) as u32, w!("Segoe UI"));
        SendMessageW(h_edit, WM_SETFONT, WPARAM(hfont.0 as usize), LPARAM(1));

        let mut physics = CursorPhysics::default();
        physics.initialized = true;

        {
            let mut states = WINDOW_STATES.lock().unwrap();
            states.insert(hwnd.0 as isize, WindowState {
                alpha: 220,
                is_hovered: false,
                on_copy_btn: false,
                copy_success: false,
                on_edit_btn: false,
                on_undo_btn: false,
                is_editing: false,
                edit_hwnd: h_edit,
                context_data: context,
                full_text: String::new(),
                text_history: Vec::new(),
                is_refining: false,
                animation_offset: 0.0,
                model_id,
                provider,
                streaming_enabled,
                bg_color: color,
                linked_window: None,
                physics,
                interaction_mode: InteractionMode::None,
                current_resize_edge: ResizeEdge::None,
                drag_start_mouse: POINT { x: 0, y: 0 },
                drag_start_window_rect: RECT::default(),
                has_moved_significantly: false,
                font_cache_dirty: true,
                cached_font_size: 72,
                content_bitmap: HBITMAP(0),
                last_w: 0,
                last_h: 0,
                pending_text: None,
                last_text_update_time: 0,
                bg_bitmap: HBITMAP(0),
                bg_bits: std::ptr::null_mut(),
                bg_w: 0,
                bg_h: 0,
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
        
        SetTimer(hwnd, 3, 16, None);
        
        InvalidateRect(hwnd, None, false);
        UpdateWindow(hwnd);
        
        hwnd
    }
}

pub fn update_window_text(hwnd: HWND, text: &str) {
    if !unsafe { IsWindow(hwnd).as_bool() } { return; }
    
    let mut states = WINDOW_STATES.lock().unwrap();
    if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
        state.pending_text = Some(text.to_string());
        state.full_text = text.to_string();
    }
}
