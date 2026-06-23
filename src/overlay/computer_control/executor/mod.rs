//! Native Windows action executor for Computer Control: turns the model's tool
//! calls (`click`/`type_text`/`key_combination`/`scroll`/`drag`/…) into real
//! `SendInput` mouse/keyboard events.
//!
//! Coordinate space: the model targets the PIXEL SPACE OF THE FRAME IT WAS SHOWN
//! (verified by the probe), reported on a 0-1000 grid; the helpers here map that to
//! the 0..65535 virtual-desktop space used by `MOUSEEVENTF_ABSOLUTE |
//! MOUSEEVENTF_VIRTUALDESK`. The action handlers live in the `mouse` / `keyboard` /
//! `shell` submodules; this module holds the dispatch, the coordinate math, and the
//! raw `SendInput` builders they share.

use std::sync::atomic::AtomicBool;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MapVirtualKeyW,
    MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, SendInput, VIRTUAL_KEY, VK_DELETE,
    VK_DOWN, VK_END, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RIGHT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

use super::human_input::{self, HumanProfile, Outcome};

mod keyboard;
mod mouse;
mod shell;

/// Map a 0..1000 normalized model coordinate to the 0..65535 virtual-desktop
/// absolute space used by `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK`.
/// The model reports points on a 0-1000 grid over the screenshot (verified by
/// the coord-test harness) — independent of the frame's pixel size.
fn norm_to_absolute(x: f64, y: f64) -> (i32, i32) {
    let nx = (x / 1000.0 * 65535.0).round().clamp(0.0, 65535.0) as i32;
    let ny = (y / 1000.0 * 65535.0).round().clamp(0.0, 65535.0) as i32;
    (nx, ny)
}

/// Move the cursor to a 0-1000 normalized point (no click). Used by the
/// coordinate-accuracy debug harness.
pub(super) fn move_to(x: f64, y: f64) {
    let (nx, ny) = norm_to_absolute(x, y);
    move_abs(nx, ny);
}

/// Back-compat: instant, non-cancellable execution (coord-test / legacy callers).
pub fn execute(name: &str, args: &Value) -> Value {
    execute_ex(name, args, &HumanProfile::instant(), &AtomicBool::new(false))
}

/// Dispatch one tool call to a real OS action. `profile` selects humanization
/// (instant by default); `cancel` lets a spoken "stop" abort mid-action — it is
/// polled between micro-steps (every cursor segment / keystroke). Returns a JSON
/// result suitable for the `toolResponse`; never panics on bad args.
pub fn execute_ex(name: &str, args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Value {
    let result: Result<Value> = match name {
        "click" => mouse::click(args, 1, profile, cancel),
        "double_click" => mouse::click(args, 2, profile, cancel),
        "drag" => mouse::drag(args, profile, cancel),
        "scroll" => mouse::scroll(args, profile, cancel),
        "type_text" => keyboard::type_text(args, profile, cancel),
        "key_combination" => keyboard::key_combination(args, cancel),
        "open_url" => shell::open_url(args),
        "launch_app" => shell::launch_app(args),
        "run_command" => shell::run_command(args),
        "click_here" => mouse::click_here(args),
        "point" => mouse::point(args, profile, cancel),
        other => Err(anyhow!("unknown action: {other}")),
    };
    match result {
        Ok(v) => v,
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

// --- screen/coordinate helpers for humanized motion ---

fn virtual_desktop() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

/// 0-1000 normalized -> absolute screen pixel.
fn norm_to_screen(x: f64, y: f64) -> (i32, i32) {
    let (vx, vy, vw, vh) = virtual_desktop();
    (
        vx + (x / 1000.0 * vw as f64).round() as i32,
        vy + (y / 1000.0 * vh as f64).round() as i32,
    )
}

/// Absolute screen pixel -> 0..1000 normalized (the space `click`/`scroll`/`drag`
/// take). Inverse of `norm_to_screen`; lets Brain-side screen px feed those tools.
pub(super) fn screen_to_norm(sx: i32, sy: i32) -> (f64, f64) {
    let (vx, vy, vw, vh) = virtual_desktop();
    (
        (sx - vx) as f64 / vw.max(1) as f64 * 1000.0,
        (sy - vy) as f64 / vh.max(1) as f64 * 1000.0,
    )
}

/// Absolute screen pixel -> 0..65535 virtual-desktop space (for `SendInput`).
fn screen_to_abs(sx: i32, sy: i32) -> (i32, i32) {
    let (vx, vy, vw, vh) = virtual_desktop();
    let nx = ((sx - vx) as f64 / vw.max(1) as f64 * 65535.0).round().clamp(0.0, 65535.0) as i32;
    let ny = ((sy - vy) as f64 / vh.max(1) as f64 * 65535.0).round().clamp(0.0, 65535.0) as i32;
    (nx, ny)
}

fn cursor_pos() -> (f64, f64) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x as f64, p.y as f64)
}

/// Move to a 0-1000 point: a humanized path when the profile asks, else instant.
/// Returns `Aborted` if `cancel` tripped mid-move.
pub(super) fn move_humanized(
    x: f64,
    y: f64,
    target_w: f64,
    profile: &HumanProfile,
    cancel: &AtomicBool,
) -> Outcome {
    if profile.humanized() {
        let (tx, ty) = norm_to_screen(x, y);
        let from = cursor_pos();
        human_input::human_move(
            from,
            (tx as f64, ty as f64),
            target_w,
            profile,
            cancel,
            &|sx, sy| {
                let (ax, ay) = screen_to_abs(sx, sy);
                move_abs(ax, ay);
            },
        )
    } else {
        let (nx, ny) = norm_to_absolute(x, y);
        move_abs(nx, ny);
        Outcome::Done
    }
}

pub(super) fn aborted() -> Value {
    json!({"ok": false, "status": "aborted_by_user"})
}

/// Visual demo (no model): glide the real cursor on a tour of screen points so
/// the WindMouse path + overshoot are visible. Honors `CC_HUMANIZE`; falls back
/// to a realistic persona so the motion always shows.
pub fn cursor_demo() {
    let mut profile = HumanProfile::from_env();
    if !profile.humanized() {
        profile = HumanProfile::realistic();
    }
    let cancel = AtomicBool::new(false);
    let (vx, vy, vw, vh) = virtual_desktop();
    let p = |fx: f64, fy: f64| (vx + (fx * vw as f64) as i32, vy + (fy * vh as f64) as i32);
    // Center, then the four corners via long diagonals (triggers overshoot), back.
    let tour = [
        p(0.5, 0.5),
        p(0.12, 0.14),
        p(0.88, 0.85),
        p(0.86, 0.14),
        p(0.14, 0.85),
        p(0.5, 0.5),
    ];
    let mut max_err = 0.0f64;
    for &(tx, ty) in &tour {
        let from = cursor_pos();
        human_input::human_move(from, (tx as f64, ty as f64), 40.0, &profile, &cancel, &|sx, sy| {
            let (ax, ay) = screen_to_abs(sx, sy);
            move_abs(ax, ay);
        });
        // Harness-accuracy probe: where did the cursor ACTUALLY land vs intended?
        let after = cursor_pos();
        let err = (after.0 - tx as f64).hypot(after.1 - ty as f64);
        max_err = max_err.max(err);
        eprintln!(
            "[cursor-demo] target ({tx},{ty}) landed ({},{}) Δ={err:.1}px",
            after.0 as i32, after.1 as i32
        );
        sleep(Duration::from_millis(250));
    }
    eprintln!("[cursor-demo] done — max landing error {max_err:.1}px (harness fidelity)");
}

fn xy(args: &Value) -> Result<(f64, f64)> {
    let x = args.get("x").and_then(Value::as_f64).ok_or_else(|| anyhow!("missing x"))?;
    let y = args.get("y").and_then(Value::as_f64).ok_or_else(|| anyhow!("missing y"))?;
    Ok((x, y))
}

fn button_flags(args: &Value) -> (MOUSE_EVENT_FLAGS, MOUSE_EVENT_FLAGS) {
    match args.get("button").and_then(Value::as_str).unwrap_or("left") {
        "right" => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        "middle" => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        _ => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
    }
}

// --- raw SendInput helpers (shared by the action submodules via `super::`) ---

fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn move_abs(nx: i32, ny: i32) {
    send(&[mouse_input(
        nx,
        ny,
        0,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )]);
}

fn mouse_input(dx: i32, dy: i32, data: i32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                // mouseData is u32; wheel deltas are signed and wrap correctly.
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key_unicode(scan: u16, up: bool) -> INPUT {
    let flags = if up {
        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
    } else {
        KEYEVENTF_UNICODE
    };
    keybd(VIRTUAL_KEY(0), scan, flags)
}

/// Build a virtual-key event WITH its hardware scan code. A real keypress carries
/// the scan code, which browsers turn into `KeyboardEvent.code` ("KeyW",
/// "ArrowDown") — games that read `.code` ignore a scan-codeless synthetic key.
/// Setting both wVk and wScan makes the injected key indistinguishable from a
/// physical one. Extended keys (arrows, nav cluster) need the EXTENDEDKEY flag.
fn key_vk(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let scan = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) } as u16;
    let mut flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
    if is_extended_key(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    keybd(vk, scan, flags)
}

/// Keys in the "extended" set send an extra 0xE0 prefix on real hardware; the
/// EXTENDEDKEY flag reproduces that so `KeyboardEvent.code` resolves correctly.
fn is_extended_key(vk: VIRTUAL_KEY) -> bool {
    matches!(
        vk,
        VK_LEFT | VK_UP | VK_RIGHT | VK_DOWN | VK_HOME | VK_END | VK_PRIOR | VK_NEXT | VK_INSERT | VK_DELETE
    )
}

fn keybd(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_0_1000_to_normalized_absolute() {
        assert_eq!(norm_to_absolute(0.0, 0.0), (0, 0));
        assert_eq!(norm_to_absolute(1000.0, 1000.0), (65535, 65535));
        // Centre (500,500) maps to ~half of 65535.
        let (cx, cy) = norm_to_absolute(500.0, 500.0);
        assert!((cx - 32767).abs() <= 1);
        assert!((cy - 32767).abs() <= 1);
    }

    #[test]
    fn out_of_range_coords_clamp() {
        assert_eq!(norm_to_absolute(-50.0, 9999.0), (0, 65535));
    }
}
