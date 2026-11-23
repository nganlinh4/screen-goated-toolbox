//! Window procedure for the DComp orb. Receives cross-thread requests (show / hide /
//! run-script) posted from the runtime thread and drives the composition-hosted WebView2,
//! which it owns on this (the message-loop) thread: `ExecuteScript` for state, and mouse +
//! cursor forwarding (composition hosting delivers neither automatically).

use std::sync::atomic::Ordering;

use webview2_com::ExecuteScriptCompletedHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MOUSE_EVENT_KIND, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS, ICoreWebView2Controller,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::SetWindowRgn;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, GetForegroundWindow, HCURSOR, HWND_TOPMOST, IsWindow, KillTimer,
    MA_ACTIVATE, MA_NOACTIVATE, PostQuitMessage, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOZORDER, SetCursor, SetForegroundWindow, SetTimer, SetWindowPos, ShowWindow, WM_CLOSE,
    WM_DESTROY, WM_DISPLAYCHANGE, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_TIMER,
};
use windows::core::{HSTRING, Interface, PCWSTR};

use super::{
    HIDE_TIMER_ID, LEAVE_TIMER_ID, ORB_COMP, ORB_TEXT_MODE, ORB_WEBVIEW, WM_APP_HIDE_ORB,
    WM_APP_ORB_PROCESS_FAILED, WM_APP_RUN_ORB_SCRIPT, WM_APP_SHOW_ORB,
    visibility_message_is_current,
};

pub(super) unsafe extern "system" fn orb_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_RUN_ORB_SCRIPT => {
                let p = lparam.0 as *mut String;
                if !p.is_null() {
                    let script = Box::from_raw(p);
                    exec_script(&script);
                }
                LRESULT(0)
            }

            WM_APP_SHOW_ORB => {
                if visibility_message_is_current(wparam.0, true) {
                    handle_show(hwnd);
                }
                LRESULT(0)
            }

            WM_APP_HIDE_ORB => {
                if visibility_message_is_current(wparam.0, false) {
                    exec_script("window.cc&&window.cc.hide();");
                    // Let the fly-out animation play, then hide the window.
                    let _ = SetTimer(Some(hwnd), HIDE_TIMER_ID, 900, None);
                }
                LRESULT(0)
            }

            WM_TIMER => {
                if wparam.0 == HIDE_TIMER_ID {
                    let _ = KillTimer(Some(hwnd), HIDE_TIMER_ID);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                } else if wparam.0 == LEAVE_TIMER_ID && super::ipc::cursor_left_orb(hwnd) {
                    // Cursor left the orb's footprint → auto-dismiss the command box (visual + focus).
                    exec_script("window.cc&&window.cc.closeCmd&&window.cc.closeCmd();");
                    super::ipc::end_text_mode(hwnd);
                }
                LRESULT(0)
            }

            // Composition-hosted WebView2 receives no input automatically — forward it so the orb's
            // pointer/drag handlers fire. Only reaches here inside the window region (the orb box); the
            // orb is always grabbable (the dodge keeps its footprint clear of the agent's clicks).
            WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP => {
                forward_mouse(msg, wparam, lparam);
                LRESULT(0)
            }

            // Apply the WebView's own cursor (grab / grabbing) — composition hosting won't do it for us.
            WM_SETCURSOR => {
                let mut handled = false;
                ORB_COMP.with(|c| {
                    if let Some(comp) = c.borrow().as_ref() {
                        let mut cur = HCURSOR::default();
                        if comp.Cursor(&mut cur).is_ok() && !cur.is_invalid() {
                            let _ = SetCursor(Some(cur));
                            handled = true;
                        }
                    }
                });
                if handled {
                    return LRESULT(1);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }

            // Normally never activate (dragging mustn't steal focus); but while the command box is
            // open we WANT activation so the WebView's text input keeps real keyboard focus.
            WM_MOUSEACTIVATE => LRESULT(if ORB_TEXT_MODE.load(Ordering::SeqCst) {
                MA_ACTIVATE
            } else {
                MA_NOACTIVATE
            } as isize),

            WM_DISPLAYCHANGE => {
                handle_display_change(hwnd);
                LRESULT(0)
            }

            // A user close only hides the orb; the host stays warm for the next show.
            WM_CLOSE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }

            // A dead WebView cannot recover in place: end this host so the window
            // thread's cleanup clears the warm state and restarts on demand.
            WM_APP_ORB_PROCESS_FAILED => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }

            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }

            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// Reveal the orb: full region first (so the fly-in is visible from the real screen edge —
/// the page tightens it to the orb box once it settles), then `show()`. Restores the prior
/// foreground window so we never steal focus.
unsafe fn handle_show(hwnd: HWND) {
    unsafe {
        let foreground = GetForegroundWindow();
        let (vx, vy, vw, vh) = super::virtual_screen();

        let _ = KillTimer(Some(hwnd), HIDE_TIMER_ID);
        let _ = SetWindowRgn(hwnd, None, true);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), vx, vy, vw, vh, SWP_NOACTIVATE);

        exec_script(&format!(
            "{}window.cc&&window.cc.show();",
            super::placement_script()
        ));

        if !foreground.0.is_null() && IsWindow(Some(foreground)).as_bool() {
            let _ = SetForegroundWindow(foreground);
        }
    }
}

/// Keep the window covering the (possibly resized / rearranged) virtual desktop.
unsafe fn handle_display_change(hwnd: HWND) {
    unsafe {
        let (vx, vy, vw, vh) = super::virtual_screen();
        let _ = SetWindowPos(hwnd, None, vx, vy, vw, vh, SWP_NOZORDER | SWP_NOACTIVATE);
        ORB_COMP.with(|c| {
            if let Some(comp) = c.borrow().as_ref()
                && let Ok(controller) = comp.cast::<ICoreWebView2Controller>()
            {
                let _ = controller.SetBounds(RECT {
                    left: 0,
                    top: 0,
                    right: vw,
                    bottom: vh,
                });
            }
        });
    }
}

/// Run a JS snippet on the orb WebView (no result needed → a no-op completion handler).
fn exec_script(js: &str) {
    ORB_WEBVIEW.with(|cell| {
        if let Some(webview) = cell.borrow().as_ref() {
            let hs = HSTRING::from(js);
            let handler = ExecuteScriptCompletedHandler::create(Box::new(|_code, _result| Ok(())));
            unsafe {
                let _ = webview.ExecuteScript(PCWSTR(hs.as_ptr()), &handler);
            }
        }
    });
}

/// Forward a Win32 mouse message to the composition controller. The WebView2 mouse-event-kind
/// constants are defined to equal the WM_* values, so the message id maps straight across.
fn forward_mouse(msg: u32, wparam: WPARAM, lparam: LPARAM) {
    let x = (lparam.0 & 0xFFFF) as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
    let vkeys = (wparam.0 & 0xFFFF) as i32;
    ORB_COMP.with(|c| {
        if let Some(comp) = c.borrow().as_ref() {
            unsafe {
                let _ = comp.SendMouseInput(
                    COREWEBVIEW2_MOUSE_EVENT_KIND(msg as i32),
                    COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(vkeys),
                    0,
                    POINT { x, y },
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, PostMessageW,
        RegisterClassW, SendMessageW, WINDOW_EX_STYLE, WM_QUIT, WNDCLASSW, WS_POPUP,
    };
    use windows::core::w;

    #[test]
    fn process_failure_ends_the_host_loop_while_user_close_only_hides() {
        std::thread::spawn(|| unsafe {
            let instance = GetModuleHandleW(None).unwrap();
            let class = w!("SGTComputerControlOrbProcFailureTest");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(orb_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();

            SendMessageW(hwnd, WM_CLOSE, None, None);
            assert!(
                IsWindow(Some(hwnd)).as_bool(),
                "a user close must keep the host"
            );

            PostMessageW(Some(hwnd), WM_APP_ORB_PROCESS_FAILED, WPARAM(0), LPARAM(0)).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut quit = false;
            while !quit && Instant::now() < deadline {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        quit = true;
                        break;
                    }
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(
                quit,
                "the host message loop must end so cleanup can restart it"
            );
            assert!(!IsWindow(Some(hwnd)).as_bool());
        })
        .join()
        .unwrap();
    }
}
