use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub mod misc;
pub mod timer_tasks;

pub const MIN_WINDOW_WIDTH: i32 = 40;
pub const MIN_WINDOW_HEIGHT: i32 = 30;

pub unsafe extern "system" fn result_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => misc::handle_paint(hwnd),
            WM_NCHITTEST => handle_hit_test(hwnd, lparam),
            WM_TIMER => timer_tasks::handle_timer(hwnd, wparam),
            WM_DESTROY => misc::handle_destroy(hwnd),
            WM_DISPLAYCHANGE => misc::handle_display_change(hwnd),
            WM_SHOWWINDOW => misc::handle_show_window(hwnd, wparam, lparam),
            WM_GETMINMAXINFO => {
                let mmi = lparam.0 as *mut MINMAXINFO;
                if !mmi.is_null() {
                    (*mmi).ptMinTrackSize.x = MIN_WINDOW_WIDTH;
                    (*mmi).ptMinTrackSize.y = MIN_WINDOW_HEIGHT;
                }
                LRESULT(0)
            }
            msg if msg == misc::WM_UNDO_CLICK => {
                crate::overlay::result::trigger_undo(hwnd);
                LRESULT(0)
            }
            msg if msg == misc::WM_REDO_CLICK => {
                crate::overlay::result::trigger_redo(hwnd);
                LRESULT(0)
            }
            msg if msg == misc::WM_COPY_CLICK => {
                crate::overlay::result::trigger_copy(hwnd);
                LRESULT(0)
            }
            msg if msg == misc::WM_EDIT_CLICK => {
                crate::overlay::result::trigger_edit(hwnd);
                LRESULT(0)
            }
            msg if msg == misc::WM_BACK_CLICK => misc::handle_back_click(hwnd),
            msg if msg == misc::WM_FORWARD_CLICK => misc::handle_forward_click(hwnd),
            msg if msg == misc::WM_SPEAKER_CLICK => {
                crate::overlay::result::trigger_speaker(hwnd);
                LRESULT(0)
            }
            msg if msg == misc::WM_DOWNLOAD_CLICK => misc::handle_download_click(hwnd),
            msg if msg == misc::WM_CLOSE_GROUP_CLICK => misc::handle_close_group_click(hwnd),
            WM_WINDOWPOSCHANGED => {
                crate::overlay::result::scene_compositor::sync_geometry(
                    hwnd,
                    IsWindowVisible(hwnd).as_bool(),
                );
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_ENTERSIZEMOVE => {
                crate::overlay::result::raise_window(hwnd);
                crate::overlay::result::button_canvas::set_drag_mode(true);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_EXITSIZEMOVE => {
                save_window_geometry(hwnd, "WM_EXITSIZEMOVE");
                crate::overlay::result::button_canvas::update_window_position(hwnd);
                crate::overlay::result::button_canvas::set_drag_mode(false);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn handle_hit_test(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    unsafe {
        let x = (lparam.0 & 0xffff) as i16 as i32;
        let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let left = x < rect.left + 6;
        let right = x >= rect.right - 6;
        let top = y < rect.top + 4;
        let bottom = y >= rect.bottom - 4;
        let hit = match (left, right, top, bottom) {
            (true, _, true, _) => HTTOPLEFT,
            (_, true, true, _) => HTTOPRIGHT,
            (true, _, _, true) => HTBOTTOMLEFT,
            (_, true, _, true) => HTBOTTOMRIGHT,
            (true, _, _, _) => HTLEFT,
            (_, true, _, _) => HTRIGHT,
            (_, _, true, _) => HTTOP,
            (_, _, _, true) => HTBOTTOM,
            _ => HTCLIENT,
        };
        LRESULT(hit as isize)
    }
}

pub fn save_window_geometry(hwnd: HWND, _source: &str) {
    unsafe {
        let (preset_id, rect, is_root) = {
            let states = crate::overlay::result::state::WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get(&(hwnd.0 as isize)) {
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                (state.preset_id.clone(), rect, state.is_chain_root)
            } else {
                (None, RECT::default(), false)
            }
        };
        let Some(preset_id) = preset_id else { return };
        if !is_root {
            return;
        }
        let mut app = crate::APP.lock().unwrap();
        if let Some(preset) = app
            .config
            .presets
            .iter_mut()
            .find(|preset| preset.id == preset_id)
            && preset.preset_type != "image"
        {
            preset.window_geometry = Some(crate::config::preset::WindowGeometry {
                x: rect.left,
                y: rect.top,
                width: rect.right - rect.left,
                height: rect.bottom - rect.top,
            });
            crate::config::save_config(&app.config);
        }
    }
}
