//! Mouse action handlers for the Computer Control executor: click / double-click,
//! drag, point (hover, no click), click-at-cursor, and wheel scroll. The
//! coordinate math and the raw `SendInput` builders live in the parent module and
//! are reached via `super::`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_WHEEL,
};

use super::super::human_input::{self, HumanProfile, Outcome};

const WHEEL_DELTA: i32 = 120;

pub(super) fn click(args: &Value, times: u32, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    let (x, y) = super::xy(args)?;
    let target_w = args.get("target_w").and_then(Value::as_f64).unwrap_or(40.0);
    let (down, up) = super::button_flags(args);
    if super::move_humanized(x, y, target_w, profile, cancel) == Outcome::Aborted {
        return Ok(super::aborted());
    }
    // Confidence-gated hesitation: on an uncertain (vision/grid-located) click,
    // settle on the target before committing so the user can still barge in.
    if profile.humanized()
        && args.get("uncertain").and_then(Value::as_bool).unwrap_or(false)
        && human_input::sleep_cancellable(human_input::hesitation_ms(), cancel)
    {
        return Ok(super::aborted());
    }
    let dwell = if profile.humanized() { human_input::click_dwell_ms() } else { 20 };
    sleep(Duration::from_millis(20));
    for _ in 0..times {
        if cancel.load(Ordering::Relaxed) {
            return Ok(super::aborted());
        }
        // down -> dwell -> up always completes together (never leaves a held button).
        super::send(&[super::mouse_input(0, 0, 0, down)]);
        sleep(Duration::from_millis(dwell));
        super::send(&[super::mouse_input(0, 0, 0, up)]);
        sleep(Duration::from_millis(20));
    }
    Ok(json!({"ok": true, "clicked": [x, y], "times": times}))
}

pub(super) fn drag(args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    let (x, y) = super::xy(args)?;
    let dx = args.get("dest_x").and_then(Value::as_f64).ok_or_else(|| anyhow!("missing dest_x"))?;
    let dy = args.get("dest_y").and_then(Value::as_f64).ok_or_else(|| anyhow!("missing dest_y"))?;
    if super::move_humanized(x, y, 40.0, profile, cancel) == Outcome::Aborted {
        return Ok(super::aborted());
    }
    sleep(Duration::from_millis(30));
    super::send(&[super::mouse_input(0, 0, 0, MOUSEEVENTF_LEFTDOWN)]);
    sleep(Duration::from_millis(40));
    if super::move_humanized(dx, dy, 40.0, profile, cancel) == Outcome::Aborted {
        super::send(&[super::mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP)]); // release the held button
        return Ok(super::aborted());
    }
    sleep(Duration::from_millis(40));
    super::send(&[super::mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP)]);
    Ok(json!({"ok": true, "drag": [[x, y], [dx, dy]]}))
}

/// Glide the cursor to a 0-1000 point and STOP there - a point/hover, NO click.
/// For "point at / show me X" (indicate without acting) or to hover and reveal a
/// tooltip / hover-menu. `dwell_ms` lingers on the target so that reveal can happen
/// before the next frame is captured. Pollable by `cancel` like every motion.
pub(super) fn point(args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    let (x, y) = super::xy(args)?;
    if super::move_humanized(x, y, 40.0, profile, cancel) == Outcome::Aborted {
        return Ok(super::aborted());
    }
    let dwell = args.get("dwell_ms").and_then(Value::as_u64).unwrap_or(0).min(10_000);
    if dwell > 0 && human_input::sleep_cancellable(dwell, cancel) {
        return Ok(super::aborted());
    }
    Ok(json!({"ok": true, "pointed": [x, y]}))
}

/// Click (or right/middle-click) at the CURRENT cursor position WITHOUT moving
/// the mouse - for "this / the one I'm hovering on", where the user's pointer is
/// already on the target (so we don't have to guess it by description).
pub(super) fn click_here(args: &Value) -> Result<Value> {
    super::super::uia::focus_foreground();
    let (down, up) = super::button_flags(args);
    super::send(&[super::mouse_input(0, 0, 0, down), super::mouse_input(0, 0, 0, up)]);
    Ok(json!({"ok": true, "clicked": "at the current cursor position"}))
}

pub(super) fn scroll(args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    let (x, y) = super::xy(args)?;
    if super::move_humanized(x, y, 40.0, profile, cancel) == Outcome::Aborted {
        return Ok(super::aborted());
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
    super::send(&[super::mouse_input(0, 0, data, flag)]);
    Ok(json!({"ok": true, "scroll": dir, "magnitude": magnitude}))
}
