//! The Computer Control "assistant orb": a transparent, fullscreen, capture-excluded
//! overlay that hosts the living-orb canvas (`orb.html`) and reflects the agent's
//! state (icon + colour + motion) with Live-Translate-style captions.
//!
//! It is a `WS_EX_NOREDIRECTIONBITMAP` window composited via DirectComposition,
//! hosting WebView2 in *composition* mode — the only setup that gives BOTH true
//! per-pixel transparency over the desktop AND `WDA_EXCLUDEFROMCAPTURE` (so the agent
//! never sees its own UI). `WS_EX_LAYERED`/DWM-alpha is incompatible with capture
//! exclusion (the "white box"), which is why this can't use wry's windowed hosting.
//! See `dcomp.rs` for the host, `window.rs` for the thread/lifecycle.
//!
//! The window runs on its own thread with a message loop; other threads drive it via
//! `PostMessageW` custom messages (show / hide / run-script). `overlay.rs` is the
//! caller; it pushes the agent's state through `post_orb_script` (→ `ExecuteScript`).

mod dcomp;
mod html;
mod ipc;
mod window;
mod wnd_proc;

use std::cell::RefCell;
use std::sync::{Mutex, Once};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2CompositionController,
};
use windows061::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows061::Win32::UI::HiDpi::GetDpiForSystem;
use windows061::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, IsWindow, PostMessageW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WM_APP,
};

use window::create_orb_window;

static ORB_HWND: AtomicIsize = AtomicIsize::new(0);
static ORB_WARMED_UP: AtomicBool = AtomicBool::new(false);
static ORB_INITIALIZING: AtomicBool = AtomicBool::new(false);
static REGISTER_ORB_CLASS: Once = Once::new();

/// Whether the orb responds to the pointer (drag). Action states (Click/Type/…) set this
/// false so a synthetic agent click never grabs/moves the orb; the window proc reads it
/// live on each mouse message.
///
/// NOTE: on a DirectComposition window `SetWindowRgn` clips the VISUAL and INPUT together,
/// so the orb cannot be both visible AND click-through. It is kept visible (its small
/// corner footprint stays topmost) to show the agent's activity; this flag only gates
/// dragging. The agent's synthetic clicks land everywhere except the orb's own footprint.
static ORB_INTERACTIVE: AtomicBool = AtomicBool::new(true);

/// Set once the orb page has parsed + initialised (the `orbReady` IPC). `show_orb` waits
/// for this before revealing the window, so the first show never flashes the white default.
static ORB_PAGE_READY: AtomicBool = AtomicBool::new(false);

/// The orb's size factor — mirrors `mag` in `orb.html` (we never change it), used to size
/// the "is the orb in the way?" danger zone.
const ORB_MAG: f64 = 0.18;
/// Default placement when nothing is persisted (matches `orb.html`: bottom-right).
const ORB_HOME_DEFAULT: (f64, f64) = (0.9, 0.86);

/// `ORB_HOME` = the user's chosen resting spot (persisted); `ORB_CUR` = where the orb is now
/// (home, or a corner it dodged to so the agent's click lands instead of hitting the orb).
/// Both are screen fractions `(cxFrac, cyFrac)`.
static ORB_HOME: Mutex<(f64, f64)> = Mutex::new(ORB_HOME_DEFAULT);
static ORB_CUR: Mutex<(f64, f64)> = Mutex::new(ORB_HOME_DEFAULT);

const WM_APP_SHOW_ORB: u32 = WM_APP + 60;
const WM_APP_HIDE_ORB: u32 = WM_APP + 61;
const WM_APP_RUN_ORB_SCRIPT: u32 = WM_APP + 62;
const HIDE_TIMER_ID: usize = 1;

thread_local! {
    /// The composition-hosted WebView (owned by the orb thread) — the `ExecuteScript` target.
    static ORB_WEBVIEW: RefCell<Option<ICoreWebView2>> = const { RefCell::new(None) };
    /// The composition controller — the window proc forwards mouse input + queries the cursor
    /// through it (composition-hosted WebView2 receives no input automatically).
    static ORB_COMP: RefCell<Option<ICoreWebView2CompositionController>> =
        const { RefCell::new(None) };
}

fn get_dpi_scale() -> f64 {
    unsafe { GetDpiForSystem() as f64 / 96.0 }
}

/// The bounding box of the whole virtual desktop `(x, y, w, h)` in physical px.
fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

fn valid_orb_hwnd() -> Option<HWND> {
    let v = ORB_HWND.load(Ordering::SeqCst);
    if v == 0 {
        return None;
    }
    let hwnd = HWND(v as *mut std::ffi::c_void);
    unsafe {
        if IsWindow(Some(hwnd)).as_bool() {
            Some(hwnd)
        } else {
            ORB_HWND.store(0, Ordering::SeqCst);
            ORB_WARMED_UP.store(false, Ordering::SeqCst);
            ORB_PAGE_READY.store(false, Ordering::SeqCst);
            None
        }
    }
}

/// Spawn the orb window thread if it is not already up (idempotent).
pub(super) fn ensure_started() {
    if ORB_WARMED_UP.load(Ordering::SeqCst) && valid_orb_hwnd().is_some() {
        return;
    }
    if !ORB_INITIALIZING.swap(true, Ordering::SeqCst) {
        std::thread::spawn(create_orb_window);
    }
}

/// Show the orb (it flies in from the nearest screen edge). Waits for the window AND the
/// page's readiness first, so the first reveal paints the transparent canvas, never the
/// WebView's white default. Times out at ~4s as a fallback.
pub(super) fn show_orb() {
    init_position();
    std::thread::spawn(|| {
        for _ in 0..80 {
            if let Some(hwnd) = valid_orb_hwnd()
                && ORB_PAGE_READY.load(Ordering::SeqCst)
            {
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_APP_SHOW_ORB, WPARAM(0), LPARAM(0));
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if let Some(hwnd) = valid_orb_hwnd() {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_APP_SHOW_ORB, WPARAM(0), LPARAM(0));
            }
        }
    });
}

/// Hide the orb (it flies out, then the window is hidden after the animation).
pub(super) fn hide_orb() {
    if let Some(hwnd) = valid_orb_hwnd() {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_HIDE_ORB, WPARAM(0), LPARAM(0));
        }
    }
}

/// Run a JS snippet on the orb WebView from any thread (boxed string handed to the
/// window's message loop, which owns the WebView). Dropped silently if the orb is not up
/// yet — state is re-pushed on the next event.
pub(super) fn post_orb_script(script: String) {
    let Some(hwnd) = valid_orb_hwnd() else {
        return;
    };
    let boxed = Box::into_raw(Box::new(script));
    unsafe {
        if PostMessageW(
            Some(hwnd),
            WM_APP_RUN_ORB_SCRIPT,
            WPARAM(0),
            LPARAM(boxed as isize),
        )
        .is_err()
        {
            drop(Box::from_raw(boxed));
        }
    }
}

/// Allow (true) or block (false) dragging the orb with the pointer. Action states pass
/// `false` so the agent's synthetic clicks never grab the orb. The window proc reads this
/// live on each mouse message, so no window message is needed.
pub(super) fn set_interactive(on: bool) {
    ORB_INTERACTIVE.store(on, Ordering::SeqCst);
}

// --- intelligent dodging: keep the orb out from under the agent's clicks ---
//
// On a DComp window the orb's footprint is topmost + opaque to input, so a synthetic click
// landing on it would hit the orb instead of the app. Rather than hide the orb during actions,
// we slide it to the corner diagonally opposite the action so it's never in the way — then
// glide it back to the user's spot when the agent rests.

/// Load the persisted placement into both home + current (called when a session shows the orb).
fn init_position() {
    let pos = persisted_orb_pos().unwrap_or(ORB_HOME_DEFAULT);
    *ORB_HOME.lock().unwrap() = pos;
    *ORB_CUR.lock().unwrap() = pos;
}

/// If the orb currently sits on/near the screen point the agent is about to act on `(sx, sy)`,
/// glide it to the opposite corner and give the region a moment to clear before the click.
/// Called from the executor right before it moves the cursor to a target (physical screen px).
pub(super) fn avoid_point(sx: i32, sy: i32) {
    if valid_orb_hwnd().is_none() {
        return;
    }
    let (vx, vy, vw, vh) = virtual_screen();
    if vw <= 0 || vh <= 0 {
        return;
    }
    let (cx, cy) = *ORB_CUR.lock().unwrap();
    let ox = vx as f64 + cx * vw as f64;
    let oy = vy as f64 + cy * vh as f64;
    // The orb's glow footprint radius (matches orb.html: min(W,H)*0.16*mag*2.4) + a margin. The
    // dpi factor cancels (the page reports logical px, the region scales them back to physical).
    let danger = vw.min(vh) as f64 * 0.16 * ORB_MAG * 2.4 + 90.0;
    if (sx as f64 - ox).hypot(sy as f64 - oy) >= danger {
        return; // not in the way — leave it where the user put it
    }
    // Dodge to the corner diagonally opposite the action point.
    let nx = if (sx as f64) < vx as f64 + vw as f64 / 2.0 { 0.90 } else { 0.10 };
    let ny = if (sy as f64) < vy as f64 + vh as f64 / 2.0 { 0.86 } else { 0.14 };
    {
        let mut curr = ORB_CUR.lock().unwrap();
        if (curr.0 - nx).abs() < 0.01 && (curr.1 - ny).abs() < 0.01 {
            return; // already dodged to that corner
        }
        *curr = (nx, ny);
    }
    move_orb_to(nx, ny);
    // Let the glide carry the orb's region clear of the click point before the executor proceeds.
    std::thread::sleep(std::time::Duration::from_millis(200));
}

/// Glide the orb back to the user's resting spot (called when the agent stops acting).
pub(super) fn restore_home() {
    let home = *ORB_HOME.lock().unwrap();
    {
        let mut curr = ORB_CUR.lock().unwrap();
        if (curr.0 - home.0).abs() < 0.01 && (curr.1 - home.1).abs() < 0.01 {
            return;
        }
        *curr = home;
    }
    move_orb_to(home.0, home.1);
}

/// The user dragged the orb → that becomes the new home (and current); remember it.
pub(super) fn note_user_move(cx: f64, cy: f64) {
    *ORB_HOME.lock().unwrap() = (cx, cy);
    *ORB_CUR.lock().unwrap() = (cx, cy);
    persist_orb_pos(cx, cy);
}

fn move_orb_to(cx: f64, cy: f64) {
    post_orb_script(format!("window.cc&&window.cc.moveTo({cx:.4},{cy:.4});"));
}

// --- remembered placement (fraction of the screen the user dragged the orb to) ---

fn orb_pos_path() -> std::path::PathBuf {
    crate::paths::app_sgt_dir().join("cc_orb_pos.json")
}

/// Remember the orb's placement after the user drags it.
pub(super) fn persist_orb_pos(cx: f64, cy: f64) {
    let _ = std::fs::write(orb_pos_path(), format!("{{\"cx\":{cx:.5},\"cy\":{cy:.5}}}"));
}

fn persisted_orb_pos() -> Option<(f64, f64)> {
    let s = std::fs::read_to_string(orb_pos_path()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    Some((v.get("cx")?.as_f64()?, v.get("cy")?.as_f64()?))
}

/// JS that restores the persisted placement; empty when none (HTML default = bottom-right).
pub(super) fn placement_script() -> String {
    match persisted_orb_pos() {
        Some((cx, cy)) => {
            format!("window.cc&&window.cc.configurePlacement({{cxFrac:{cx:.5},cyFrac:{cy:.5}}});")
        }
        None => String::new(),
    }
}
