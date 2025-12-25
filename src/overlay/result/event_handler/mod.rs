use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub mod misc;
pub mod timer_tasks;
pub mod mouse_input;
pub mod click_actions;

pub unsafe extern "system" fn result_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => misc::handle_erase_bkgnd(hwnd, wparam),
        
        WM_CTLCOLOREDIT => misc::handle_ctl_color_edit(wparam),
        
        WM_SETCURSOR => mouse_input::handle_set_cursor(hwnd),

        WM_LBUTTONDOWN => mouse_input::handle_lbutton_down(hwnd, lparam),

        WM_RBUTTONDOWN => mouse_input::handle_rbutton_down(hwnd, lparam),

        WM_MOUSEMOVE => mouse_input::handle_mouse_move(hwnd, lparam),

        0x02A3 => mouse_input::handle_mouse_leave(hwnd), // WM_MOUSELEAVE

        WM_LBUTTONUP => click_actions::handle_lbutton_up(hwnd),
        
        WM_RBUTTONUP => click_actions::handle_rbutton_up(hwnd),

        WM_MBUTTONUP => click_actions::handle_mbutton_up(),

        WM_TIMER => timer_tasks::handle_timer(hwnd, wparam),

        WM_DESTROY => misc::handle_destroy(hwnd),

        WM_PAINT => misc::handle_paint(hwnd),
        
        WM_KEYDOWN => misc::handle_keydown(),
        
        // Deferred WebView2 creation - handles the WM_CREATE_WEBVIEW we posted
        msg if msg == misc::WM_CREATE_WEBVIEW => misc::handle_create_webview(hwnd),
        
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
