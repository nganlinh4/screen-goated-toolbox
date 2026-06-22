//! Native Windows action executor for Computer Control: turns the model's tool
//! calls (`click`/`type_text`/`key_combination`/`scroll`/`drag`/…) into real
//! `SendInput` mouse/keyboard events.
//!
//! Coordinate space: the model targets the PIXEL SPACE OF THE FRAME IT WAS SHOWN
//! (verified by the probe). [`FrameGeometry`] records that frame's dimensions and
//! maps frame-pixels → the normalized 0..65535 virtual-desktop space used by
//! `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK`. Because the captured frame
//! is the whole virtual desktop (just scaled), normalization is purely by the
//! frame dimensions — no per-monitor metrics needed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_SHOWNORMAL,
};
use windows::core::PCWSTR;

use super::human_input::{self, HumanProfile, Outcome};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MapVirtualKeyW,
    MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
    MOUSEINPUT, SendInput, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END,
    VK_ESCAPE, VK_F1, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11,
    VK_F12, VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN,
    VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};

const WHEEL_DELTA: i32 = 120;

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
        "click" => click(args, 1, profile, cancel),
        "double_click" => click(args, 2, profile, cancel),
        "drag" => drag(args, profile, cancel),
        "scroll" => scroll(args, profile, cancel),
        "type_text" => type_text(args, profile, cancel),
        "key_combination" => key_combination(args, cancel),
        "open_url" => open_url(args),
        "launch_app" => launch_app(args),
        "run_command" => run_command(args),
        "click_here" => click_here(args),
        "point" => point(args, profile, cancel),
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
fn move_humanized(x: f64, y: f64, target_w: f64, profile: &HumanProfile, cancel: &AtomicBool) -> Outcome {
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

fn aborted() -> Value {
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

fn click(args: &Value, times: u32, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    let (x, y) = xy(args)?;
    let target_w = args.get("target_w").and_then(Value::as_f64).unwrap_or(40.0);
    let (down, up) = button_flags(args);
    if move_humanized(x, y, target_w, profile, cancel) == Outcome::Aborted {
        return Ok(aborted());
    }
    // Confidence-gated hesitation: on an uncertain (vision/grid-located) click,
    // settle on the target before committing so the user can still barge in.
    if profile.humanized()
        && args.get("uncertain").and_then(Value::as_bool).unwrap_or(false)
        && human_input::sleep_cancellable(human_input::hesitation_ms(), cancel)
    {
        return Ok(aborted());
    }
    let dwell = if profile.humanized() { human_input::click_dwell_ms() } else { 20 };
    sleep(Duration::from_millis(20));
    for _ in 0..times {
        if cancel.load(Ordering::Relaxed) {
            return Ok(aborted());
        }
        // down -> dwell -> up always completes together (never leaves a held button).
        send(&[mouse(0, 0, 0, down)]);
        sleep(Duration::from_millis(dwell));
        send(&[mouse(0, 0, 0, up)]);
        sleep(Duration::from_millis(20));
    }
    Ok(json!({"ok": true, "clicked": [x, y], "times": times}))
}

fn drag(args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    let (x, y) = xy(args)?;
    let dx = args.get("dest_x").and_then(Value::as_f64).ok_or_else(|| anyhow!("missing dest_x"))?;
    let dy = args.get("dest_y").and_then(Value::as_f64).ok_or_else(|| anyhow!("missing dest_y"))?;
    if move_humanized(x, y, 40.0, profile, cancel) == Outcome::Aborted {
        return Ok(aborted());
    }
    sleep(Duration::from_millis(30));
    send(&[mouse(0, 0, 0, MOUSEEVENTF_LEFTDOWN)]);
    sleep(Duration::from_millis(40));
    if move_humanized(dx, dy, 40.0, profile, cancel) == Outcome::Aborted {
        send(&[mouse(0, 0, 0, MOUSEEVENTF_LEFTUP)]); // release the held button
        return Ok(aborted());
    }
    sleep(Duration::from_millis(40));
    send(&[mouse(0, 0, 0, MOUSEEVENTF_LEFTUP)]);
    Ok(json!({"ok": true, "drag": [[x, y], [dx, dy]]}))
}

/// Glide the cursor to a 0-1000 point and STOP there - a point/hover, NO click.
/// For "point at / show me X" (indicate without acting) or to hover and reveal a
/// tooltip / hover-menu. `dwell_ms` lingers on the target so that reveal can happen
/// before the next frame is captured. Pollable by `cancel` like every motion.
fn point(args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    let (x, y) = xy(args)?;
    if move_humanized(x, y, 40.0, profile, cancel) == Outcome::Aborted {
        return Ok(aborted());
    }
    let dwell = args.get("dwell_ms").and_then(Value::as_u64).unwrap_or(0).min(10_000);
    if dwell > 0 && human_input::sleep_cancellable(dwell, cancel) {
        return Ok(aborted());
    }
    Ok(json!({"ok": true, "pointed": [x, y]}))
}

/// Click (or right/middle-click) at the CURRENT cursor position WITHOUT moving
/// the mouse - for "this / the one I'm hovering on", where the user's pointer is
/// already on the target (so we don't have to guess it by description).
fn click_here(args: &Value) -> Result<Value> {
    super::uia::focus_foreground();
    let (down, up) = button_flags(args);
    send(&[mouse(0, 0, 0, down), mouse(0, 0, 0, up)]);
    Ok(json!({"ok": true, "clicked": "at the current cursor position"}))
}

fn scroll(args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    let (x, y) = xy(args)?;
    if move_humanized(x, y, 40.0, profile, cancel) == Outcome::Aborted {
        return Ok(aborted());
    }
    let magnitude = args.get("magnitude").and_then(Value::as_f64).unwrap_or(3.0).max(0.0);
    let ticks = (magnitude * WHEEL_DELTA as f64) as i32;
    let dir = args.get("direction").and_then(Value::as_str).unwrap_or("down");
    let (flag, data) = match dir {
        "up" => (MOUSEEVENTF_WHEEL, ticks),
        "down" => (MOUSEEVENTF_WHEEL, -ticks),
        "right" => (MOUSEEVENTF_HWHEEL, ticks),
        "left" => (MOUSEEVENTF_HWHEEL, -ticks),
        other => return Err(anyhow!("bad scroll direction: {other}")),
    };
    send(&[mouse(0, 0, data, flag)]);
    Ok(json!({"ok": true, "scroll": dir, "magnitude": magnitude}))
}

fn type_text(args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    super::uia::focus_foreground(); // text must land on the on-screen window
    let raw = args.get("text").and_then(Value::as_str).ok_or_else(|| anyhow!("missing text"))?;
    // Submit handling: models routinely append a submit token ("…{enter}", a
    // trailing newline) or pass press_enter. Honor all of them — type the literal
    // text, then press Enter. Without this the field just gets
    // "chrome://extensions{enter}" typed verbatim (what stalled browser setup).
    let mut text = raw.to_string();
    let mut enter = args.get("press_enter").and_then(Value::as_bool).unwrap_or(false);
    let lower = text.to_lowercase();
    for tok in ["{enter}", "{return}", "\n"] {
        if lower.ends_with(tok) {
            text.truncate(text.len() - tok.len());
            enter = true;
            break;
        }
    }
    let n = text.chars().count();
    let press_enter = || {
        if enter && let Some(vk) = token_to_vk("enter") {
            send(&[key_vk(vk, false), key_vk(vk, true)]);
        }
    };

    // PASTE longer text via the clipboard instead of a keystroke per character
    // (slow for paragraphs, mangles non-ASCII). Save/restore the user's clipboard.
    // Short inputs still type, to leave the clipboard alone and play nice with
    // type-as-you-search fields.
    let saved = super::clipboard::get_text();
    // Only take the paste fast-path when the clipboard holds NO non-text data we
    // can't restore (an image, copied files). If any such format is present we type
    // instead - even when text ALSO exists, because re-setting our saved text would
    // EmptyClipboard the rich formats and silently downgrade them to plain text.
    let would_clobber = super::clipboard::has_nontext();
    if n > 12 && !would_clobber {
        super::clipboard::set_text(&text);
        sleep(Duration::from_millis(30));
        send_ctrl_v();
        sleep(Duration::from_millis(140));
        if saved.is_empty() {
            super::clipboard::clear(); // don't leave our text on a previously-empty clipboard
        } else {
            super::clipboard::set_text(&saved);
        }
        press_enter();
        return Ok(json!({"ok": true, "typed_chars": n, "method": "paste", "submitted": enter}));
    }
    // Slow, human-paced per-key typing ONLY when explicitly asked for (a rare field
    // that demands paced keystrokes). It is NOT tied to the cursor profile: a
    // humanized cursor should still type instantly - pacing a 66-char path to 20s is
    // pointless. Default falls through to the instant batch below.
    let slow = args.get("slow").and_then(Value::as_bool).unwrap_or(false);
    if slow && n > 0 {
        let r = human_input::human_type(
            &text,
            profile,
            cancel,
            &|unit| send(&[key_unicode(unit, false)]),
            &|unit| send(&[key_unicode(unit, true)]),
        );
        if r == Outcome::Aborted {
            return Ok(json!({"ok": false, "status": "aborted_by_user", "typed_partial": true}));
        }
        press_enter();
        return Ok(json!({"ok": true, "typed_chars": n, "submitted": enter}));
    }
    let mut inputs = Vec::new();
    for unit in text.encode_utf16() {
        inputs.push(key_unicode(unit, false));
        inputs.push(key_unicode(unit, true));
    }
    // Send in chunks so very long strings don't overflow a single call.
    for chunk in inputs.chunks(64) {
        send(chunk);
        sleep(Duration::from_millis(2));
    }
    press_enter();
    Ok(json!({"ok": true, "typed_chars": n, "submitted": enter}))
}

/// Run a PowerShell command (non-interactive, no profile) and capture its text
/// output — the agent's GENERAL escape hatch for anything without a dedicated
/// tool (files, processes, volume, system info). Inherits THIS process's
/// (non-elevated) privileges. `CREATE_NO_WINDOW` avoids a console flash.
fn run_command(args: &Value) -> Result<Value> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let command = args.get("command").and_then(Value::as_str).ok_or_else(|| anyhow!("missing command"))?;
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", command])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| anyhow!("failed to launch powershell: {e}"))?;
    let clip = |b: &[u8]| -> String { String::from_utf8_lossy(b).trim().chars().take(4000).collect() };
    Ok(json!({
        "ok": output.status.success(),
        "exit_code": output.status.code(),
        "stdout": clip(&output.stdout),
        "stderr": clip(&output.stderr),
    }))
}

/// Press Ctrl+V (paste) — Ctrl down, V down, V up, Ctrl up.
fn send_ctrl_v() {
    let v = VIRTUAL_KEY(0x56); // 'V'
    send(&[
        key_vk(VK_CONTROL, false),
        key_vk(v, false),
        key_vk(v, true),
        key_vk(VK_CONTROL, true),
    ]);
}

fn key_combination(args: &Value, cancel: &AtomicBool) -> Result<Value> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(aborted());
    }
    super::uia::focus_foreground(); // keys must land on the on-screen window
    let combo = args.get("keys").and_then(Value::as_str).ok_or_else(|| anyhow!("missing keys"))?;
    let mut vks = Vec::new();
    for token in combo.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        vks.push(token_to_vk(token).ok_or_else(|| anyhow!("unknown key: {token}"))?);
    }
    if vks.is_empty() {
        return Err(anyhow!("empty key combination"));
    }
    // Press all down in order, release in reverse (so modifiers wrap the key).
    let mut inputs: Vec<INPUT> = vks.iter().map(|&vk| key_vk(vk, false)).collect();
    inputs.extend(vks.iter().rev().map(|&vk| key_vk(vk, true)));
    send(&inputs);
    Ok(json!({"ok": true, "keys": combo}))
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `ShellExecuteW "open"` on a file/app/URL, optionally with command-line
/// arguments (e.g. open a file in an app). Returns Ok if the shell accepted it
/// (HINSTANCE > 32 per the Win32 contract).
fn shell_open(file: &str, params: Option<&str>) -> Result<()> {
    let op = to_wide("open");
    let file_w = to_wide(file);
    let params_w = params.filter(|p| !p.is_empty()).map(to_wide);
    let params_ptr = params_w.as_ref().map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr()));
    let r = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(file_w.as_ptr()),
            params_ptr,
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = r.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(anyhow!("ShellExecuteW failed (code {code})"))
    }
}

/// Open an http(s) URL in the default browser (a new, foreground tab). Far more
/// reliable than driving the address bar by keystrokes.
fn open_url(args: &Value) -> Result<Value> {
    let url = args.get("url").and_then(Value::as_str).ok_or_else(|| anyhow!("missing url"))?;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(anyhow!("url must start with http:// or https://"));
    }
    shell_open(url, None)?;
    Ok(json!({"ok": true, "opened_url": url}))
}

/// Launch (or focus) an application by name/path via the shell, e.g. "chrome",
/// "notepad", "explorer", with optional arguments (e.g. open a file in an app:
/// name="notepad", args="C:\path\file.txt"). More reliable than the Win+type
/// Start-menu dance.
fn launch_app(args: &Value) -> Result<Value> {
    let name = args.get("name").and_then(Value::as_str).ok_or_else(|| anyhow!("missing name"))?;
    let app_args = args.get("args").and_then(Value::as_str);
    shell_open(name, app_args)?;
    Ok(json!({"ok": true, "launched": name, "args": app_args}))
}

fn token_to_vk(token: &str) -> Option<VIRTUAL_KEY> {
    let lower = token.to_ascii_lowercase();
    let vk = match lower.as_str() {
        "ctrl" | "control" => VK_CONTROL,
        "alt" | "menu" => VK_MENU,
        "shift" => VK_SHIFT,
        "win" | "super" | "meta" | "cmd" => VK_LWIN,
        "enter" | "return" => VK_RETURN,
        "tab" => VK_TAB,
        "esc" | "escape" => VK_ESCAPE,
        "space" | "spacebar" => VK_SPACE,
        "backspace" | "back" => VK_BACK,
        "delete" | "del" => VK_DELETE,
        "up" => VK_UP,
        "down" => VK_DOWN,
        "left" => VK_LEFT,
        "right" => VK_RIGHT,
        "home" => VK_HOME,
        "end" => VK_END,
        "pageup" | "pgup" => VK_PRIOR,
        "pagedown" | "pgdn" => VK_NEXT,
        "f1" => VK_F1,
        "f2" => VK_F2,
        "f3" => VK_F3,
        "f4" => VK_F4,
        "f5" => VK_F5,
        "f6" => VK_F6,
        "f7" => VK_F7,
        "f8" => VK_F8,
        "f9" => VK_F9,
        "f10" => VK_F10,
        "f11" => VK_F11,
        "f12" => VK_F12,
        _ => {
            let bytes = lower.as_bytes();
            if bytes.len() == 1 {
                let c = bytes[0];
                if c.is_ascii_lowercase() {
                    return Some(VIRTUAL_KEY((c.to_ascii_uppercase()) as u16));
                }
                if c.is_ascii_digit() {
                    return Some(VIRTUAL_KEY(c as u16));
                }
            }
            return None;
        }
    };
    Some(vk)
}

// --- raw SendInput helpers ---

fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn move_abs(nx: i32, ny: i32) {
    send(&[mouse(
        nx,
        ny,
        0,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )]);
}

fn mouse(dx: i32, dy: i32, data: i32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
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

    #[test]
    fn maps_letter_and_named_keys() {
        assert_eq!(token_to_vk("c"), Some(VIRTUAL_KEY(b'C' as u16)));
        assert_eq!(token_to_vk("Ctrl"), Some(VK_CONTROL));
        assert_eq!(token_to_vk("enter"), Some(VK_RETURN));
        assert_eq!(token_to_vk("F5"), Some(VK_F5));
        assert!(token_to_vk("nope").is_none());
    }
}
