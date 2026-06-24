//! IPC from the orb WebView. On a DirectComposition window `SetWindowRgn` clips the VISUAL
//! and INPUT together, so the region IS the orb's only visible + clickable area — the rest of
//! the desktop passes clicks through. The page reports:
//! - `orbRegion` — the orb's on-screen bbox → the tight region (covers the glow, nothing cut);
//! - `orbDrag` — full window while flying in/out or being dragged (visible across its whole path);
//! - `orbMoved` — the new placement after a drag (persisted across sessions);
//! - `orbReady` — first-paint readiness, so `show_orb` reveals the window without a white flash.

use std::sync::atomic::Ordering;

use windows061::Win32::Foundation::HWND;
use windows061::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};

use super::{ORB_PAGE_READY, get_dpi_scale, note_user_move, virtual_screen};

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
        _ => {}
    }
}
