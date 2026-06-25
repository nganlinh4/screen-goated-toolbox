//! IPC from the orb WebView. On a DirectComposition window `SetWindowRgn` clips the VISUAL
//! and INPUT together, so the region IS the orb's only visible + clickable area — the rest of
//! the desktop passes clicks through. The page reports:
//! - `orbRegion` — the orb's on-screen bbox → the tight region (covers the glow, nothing cut);
//! - `orbDrag` — full window while flying in/out or being dragged (visible across its whole path);
//! - `orbMoved` — the new placement after a drag (persisted across sessions);
//! - `orbReady` — first-paint readiness, so `show_orb` reveals the window without a white flash.

use std::sync::atomic::Ordering;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC, ICoreWebView2Controller,
};
use windows061::Win32::Foundation::{HWND, POINT, RECT};
use windows061::Win32::Graphics::Gdi::{CreateRectRgn, GetWindowRgnBox, SetWindowRgn};
use windows061::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, IsWindow, KillTimer,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, WS_EX_NOACTIVATE,
};
use windows061::core::Interface;

use super::{
    LEAVE_TIMER_ID, ORB_COMP, ORB_PAGE_READY, ORB_PREV_FG, ORB_TEXT_MODE, get_dpi_scale,
    note_user_move, virtual_screen,
};

pub(super) fn handle_orb_ipc(hwnd: HWND, body: &str) {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        return;
    };
    let g = |k: &str| json.get(k).and_then(serde_json::Value::as_f64);
    match json.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        // JS reports logical (CSS) px; the window region is physical px.
        "orbRegion" => {
            let s = get_dpi_scale();
            let p = |k: &str| (g(k).unwrap_or(0.0) * s) as i32;
            let x1 = p("x");
            let y1 = p("y");
            let x2 = ((g("x").unwrap_or(0.0) + g("w").unwrap_or(0.0)) * s) as i32;
            let y2 = ((g("y").unwrap_or(0.0) + g("h").unwrap_or(0.0)) * s) as i32;
            let (_, _, vw, vh) = virtual_screen();
            if x2 - x1 < 30 || y2 - y1 < 30 || x1 >= vw || y1 >= vh || x2 <= 0 || y2 <= 0 {
                return;
            }
            unsafe {
                let _ = SetWindowRgn(hwnd, Some(CreateRectRgn(x1, y1, x2, y2)), true);
            }
        }
        // Flying in/out or being dragged: full window so the orb shows across its whole path and the
        // cursor never leaves it. Restored to the tight box by the next orbRegion (on settle / drop).
        "orbDrag" => unsafe {
            let _ = SetWindowRgn(hwnd, None, true);
        },
        "orbMoved" => {
            if let (Some(cx), Some(cy)) = (g("cxFrac"), g("cyFrac")) {
                note_user_move(cx, cy);
            }
        }
        "orbReady" => ORB_PAGE_READY.store(true, Ordering::SeqCst),
        // The command box opened (orb clicked) → take keyboard focus so typing (incl. IME) works.
        "openCommand" => begin_text_mode(hwnd),
        // A typed command was submitted (Enter) → inject it into the live session.
        "command" => {
            if let Some(t) = json.get("text").and_then(serde_json::Value::as_str) {
                let t = t.trim();
                if !t.is_empty() {
                    super::super::runtime::submit_text_command(t.to_string());
                }
            }
        }
        // The box dismissed (Enter / Esc) → hand focus back to the user's window.
        "closeCommand" => end_text_mode(hwnd),
        _ => {}
    }
}

/// Open the command box: remember the focused window, make the orb briefly activatable, and move
/// keyboard focus into the WebView so the `<input>` receives keys. REAL focus is required for IME
/// (a key hook can't compose Korean/Vietnamese). Runs on the orb thread (the IPC callback), so it
/// drives the window + composition controller directly.
pub(super) fn begin_text_mode(hwnd: HWND) {
    unsafe {
        ORB_PREV_FG.store(GetForegroundWindow().0 as isize, Ordering::SeqCst);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
        ORB_TEXT_MODE.store(true, Ordering::SeqCst);
        let _ = SetForegroundWindow(hwnd);
        ORB_COMP.with(|c| {
            if let Some(comp) = c.borrow().as_ref()
                && let Ok(ctl) = comp.cast::<ICoreWebView2Controller>()
            {
                let _ = ctl.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
            }
        });
        let _ = SetTimer(Some(hwnd), LEAVE_TIMER_ID, 120, None);
    }
}

/// Close the command box: restore `WS_EX_NOACTIVATE` (non-stealing again) and hand focus back to
/// whoever had it. Idempotent — the leave-poll and the Esc/Enter IPC can both fire.
pub(super) fn end_text_mode(hwnd: HWND) {
    if !ORB_TEXT_MODE.swap(false, Ordering::SeqCst) {
        return;
    }
    unsafe {
        let _ = KillTimer(Some(hwnd), LEAVE_TIMER_ID);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 as isize));
        let prev = ORB_PREV_FG.swap(0, Ordering::SeqCst);
        if prev != 0 {
            let h = HWND(prev as *mut std::ffi::c_void);
            if IsWindow(Some(h)).as_bool() {
                let _ = SetForegroundWindow(h);
            }
        }
    }
}

/// True once the cursor has left the orb's current footprint (the `SetWindowRgn` box) — the
/// auto-dismiss trigger. An empty/absent region ⇒ false, so we never dismiss spuriously.
pub(super) fn cursor_left_orb(hwnd: HWND) -> bool {
    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_err() {
            return false;
        }
        let mut rb = RECT::default();
        let _ = GetWindowRgnBox(hwnd, &mut rb);
        if rb.right <= rb.left || rb.bottom <= rb.top {
            return false;
        }
        let (vx, vy, _, _) = virtual_screen();
        pt.x < vx + rb.left || pt.x > vx + rb.right || pt.y < vy + rb.top || pt.y > vy + rb.bottom
    }
}
