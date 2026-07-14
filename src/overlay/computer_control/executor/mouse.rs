//! Mouse action handlers for the Computer Control executor: click / double-click,
//! drag, point (hover, no click), click-at-cursor, and wheel scroll. The
//! coordinate math and the raw `SendInput` builders live in the parent module and
//! are reached via `super::`.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOUSE_EVENT_FLAGS, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_WHEEL,
};

use super::super::human_input::{self, HumanProfile, Outcome};

const WHEEL_DELTA: i32 = 120;

pub(super) fn click(
    args: &Value,
    times: u32,
    profile: &HumanProfile,
    cancel: &AtomicBool,
) -> Result<Value> {
    let (x, y) = super::xy(args)?;
    let target_w = args.get("target_w").and_then(Value::as_f64).unwrap_or(40.0);
    let (down, up) = super::button_flags(args);
    super::verify_expected_input_target(args)?;
    if super::move_humanized(x, y, target_w, profile, cancel)? == Outcome::Aborted {
        return Ok(super::aborted());
    }
    // Confidence-gated hesitation: on an uncertain (vision/grid-located) click,
    // settle on the target before committing so the user can still barge in.
    if profile.humanized()
        && args
            .get("uncertain")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && human_input::sleep_cancellable(human_input::hesitation_ms(), cancel)
    {
        return Ok(super::aborted());
    }
    let dwell = if profile.humanized() {
        human_input::click_dwell_ms()
    } else {
        20
    };
    if human_input::sleep_cancellable(20, cancel) {
        return Ok(super::aborted());
    }
    let mut completed = 0;
    for _ in 0..times {
        if cancel.load(Ordering::Relaxed) {
            return Ok(aborted_click(completed));
        }
        super::verify_expected_pointer_target(args)?;
        press_button(down, up)?;
        let interrupted = human_input::sleep_cancellable(dwell, cancel);
        release_button(up)?;
        completed += 1;
        if interrupted {
            return Ok(aborted_click(completed));
        }
        if human_input::sleep_cancellable(20, cancel) {
            return Ok(aborted_click(completed));
        }
    }
    Ok(json!({"ok": true, "clicked": [x, y], "times": times}))
}

pub(super) fn drag(args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    let (x, y) = super::xy(args)?;
    let dx = args
        .get("dest_x")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("missing dest_x"))?;
    let dy = args
        .get("dest_y")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("missing dest_y"))?;
    super::verify_expected_input_target(args)?;
    if super::move_humanized(x, y, 40.0, profile, cancel)? == Outcome::Aborted {
        return Ok(super::aborted());
    }
    if human_input::sleep_cancellable(30, cancel) {
        return Ok(super::aborted());
    }
    super::verify_expected_input_target(args)?;
    press_button(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)?;
    if human_input::sleep_cancellable(40, cancel) {
        release_button(MOUSEEVENTF_LEFTUP)?;
        return Ok(super::aborted());
    }
    match super::move_humanized(dx, dy, 40.0, profile, cancel) {
        Ok(Outcome::Done) => {}
        Ok(Outcome::Aborted) => {
            release_button(MOUSEEVENTF_LEFTUP)?;
            return Ok(super::aborted());
        }
        Err(move_error) => {
            release_button(MOUSEEVENTF_LEFTUP)?;
            return Err(move_error);
        }
    }
    let interrupted = human_input::sleep_cancellable(40, cancel);
    if let Err(error) = super::verify_expected_input_target(args) {
        release_button(MOUSEEVENTF_LEFTUP)?;
        return Err(error);
    }
    release_button(MOUSEEVENTF_LEFTUP)?;
    if interrupted {
        return Ok(super::aborted());
    }
    Ok(json!({"ok": true, "drag": [[x, y], [dx, dy]]}))
}

/// Glide the cursor to a 0-1000 point and STOP there - a point/hover, NO click.
/// For "point at / show me X" (indicate without acting) or to hover and reveal a
/// tooltip / hover-menu. `dwell_ms` lingers on the target so that reveal can happen
/// before the next frame is captured. Pollable by `cancel` like every motion.
pub(super) fn point(args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    let (x, y) = super::xy(args)?;
    super::verify_expected_input_target(args)?;
    if super::move_humanized(x, y, 40.0, profile, cancel)? == Outcome::Aborted {
        return Ok(super::aborted());
    }
    super::verify_expected_input_target(args)?;
    let dwell = args
        .get("dwell_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(10_000);
    if dwell > 0 && human_input::sleep_cancellable(dwell, cancel) {
        return Ok(super::aborted());
    }
    Ok(json!({"ok": true, "pointed": [x, y]}))
}

/// Click (or right/middle-click) at the CURRENT cursor position WITHOUT moving
/// the mouse - for "this / the one I'm hovering on", where the user's pointer is
/// already on the target (so we don't have to guess it by description).
pub(super) fn click_here(args: &Value, cancel: &AtomicBool) -> Result<Value> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(super::aborted());
    }
    super::super::uia::focus_foreground();
    super::verify_expected_input_target(args)?;
    let (down, up) = super::button_flags(args);
    press_button(down, up)?;
    release_button(up)?;
    Ok(json!({"ok": true, "clicked": "at the current cursor position"}))
}

pub(super) fn scroll(args: &Value, profile: &HumanProfile, cancel: &AtomicBool) -> Result<Value> {
    let (x, y) = super::xy(args)?;
    super::verify_expected_input_target(args)?;
    if super::move_humanized(x, y, 40.0, profile, cancel)? == Outcome::Aborted {
        return Ok(super::aborted());
    }
    let magnitude = args
        .get("magnitude")
        .and_then(Value::as_f64)
        .unwrap_or(3.0)
        .max(0.0);
    let ticks = (magnitude * WHEEL_DELTA as f64) as i32;
    let dir = args
        .get("direction")
        .and_then(Value::as_str)
        .unwrap_or("down");
    let (flag, data) = match dir {
        "up" => (MOUSEEVENTF_WHEEL, ticks),
        "down" => (MOUSEEVENTF_WHEEL, -ticks),
        "right" => (MOUSEEVENTF_HWHEEL, ticks),
        "left" => (MOUSEEVENTF_HWHEEL, -ticks),
        other => return Err(anyhow!("bad scroll direction: {other}")),
    };
    if cancel.load(Ordering::Relaxed) {
        return Ok(super::aborted());
    }
    super::verify_expected_input_target(args)?;
    super::send(&[super::mouse_input(0, 0, data, flag)])?;
    Ok(json!({"ok": true, "scroll": dir, "magnitude": magnitude}))
}

fn press_button(down: MOUSE_EVENT_FLAGS, up: MOUSE_EVENT_FLAGS) -> Result<()> {
    if let Err(error) = super::send(&[super::mouse_input(0, 0, 0, down)]) {
        // A one-event dispatch should be all-or-nothing, but an unconditional
        // up is the safe response if a driver reports anything unexpected.
        let _ = super::release(&[super::mouse_input(0, 0, 0, up)]);
        return Err(error.into());
    }
    Ok(())
}

fn release_button(up: MOUSE_EVENT_FLAGS) -> Result<()> {
    let release = super::mouse_input(0, 0, 0, up);
    if let Err(error) = super::send(std::slice::from_ref(&release)) {
        let _ = super::release(std::slice::from_ref(&release));
        return Err(error.into());
    }
    Ok(())
}

fn aborted_click(completed: u32) -> Value {
    json!({
        "ok": false,
        "status": "aborted_by_user",
        "completed_clicks": completed,
    })
}
