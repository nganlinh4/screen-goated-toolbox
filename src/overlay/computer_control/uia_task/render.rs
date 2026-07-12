//! View capture/rendering, click-coordinate mapping, and the structured
//! element-state formatting — split out of `uia_task.rs` to keep it within the
//! file-size limit. `use super::*` pulls in the shared imports/types/consts.

use super::*;

/// The view the model is grounded on. DEFAULT = the ACTIVE window (precise clicks,
/// no whole-screen ambiguity) - the target window if `target` is set, else the
/// foreground one. When `whole_screen` is set (the model called see_whole_screen for
/// awareness / to find another window) it's the WHOLE virtual desktop instead.
pub(super) fn window_view(target: Option<&str>, whole_screen: bool) -> View {
    if !whole_screen && let Some((x, y, w, h)) = uia::target_window_rect(target) {
        return View { x, y, w, h };
    }
    let (x, y, w, h) = uia::virtual_desktop();
    if target.is_some() && !whole_screen {
        super::super::telemetry::typed_error(
            "ERR_TARGET_VIEW_FALLBACK",
            "capture",
            "target window could not be resolved; capture fell back to the virtual desktop",
            json!({"target": target, "fallback_view": [x, y, w, h]}),
        );
    }
    View { x, y, w, h }
}

/// Capture + overlay + save the current view; return (base64 JPEG, clamped view)
/// WITHOUT sending. Callers send the frame AFTER answering any pending tool call:
/// pushing realtimeInput while a synchronous-FC tool is unanswered can trip an
/// intermittent INVALID_ARGUMENT abort.
/// Half-width (screen px) of the box compared before/after a click for the "did
/// it register?" signal.
pub(super) const VC_HALF: i32 = 90;

/// Minimum changed fingerprint cells (of 1024) to count as a real on-screen change
/// in that box. Above the cursor's own footprint, below a placed mark / reveal.
/// Tunable via CC_VC_MIN.
pub(super) fn vc_min() -> u32 {
    std::env::var("CC_VC_MIN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(14)
}

pub(super) fn render_view(
    dir: &str,
    step: usize,
    view: View,
    grid: Grid,
    marker: Option<(i32, i32)>,
    reason: &str,
    action: Option<super::super::telemetry::ActionTrace>,
) -> Result<(String, View, Vec<u8>, u64)> {
    use super::super::telemetry;
    let frame_id = telemetry::next_frame_for(reason, action);
    let capture_t0 = Instant::now();
    let cap = match session::capture_virtual() {
        Ok(cap) => cap,
        Err(error) => {
            frame_event(
                "frame_failed",
                action,
                json!({"frame_id": frame_id, "reason": reason, "phase": "capture", "error": error.to_string()}),
            );
            return Err(error);
        }
    };
    let capture_ms = capture_t0.elapsed().as_millis();
    let encode_t0 = Instant::now();
    let (jpeg, shown) = match session::encode_view(&cap, view, VIEW_SHORT, Some(grid), marker) {
        Ok(encoded) => encoded,
        Err(error) => {
            frame_event(
                "frame_failed",
                action,
                json!({"frame_id": frame_id, "reason": reason, "phase": "encode", "error": error.to_string()}),
            );
            return Err(error);
        }
    };
    let encode_ms = encode_t0.elapsed().as_millis();
    // Fingerprint the CLEAN region around the click (no grid/marker overlay), so we
    // can tell whether the click changed ITS OWN cell - ignoring a timer/animation
    // elsewhere. With no marker (turn 0 / keyboard) fall back to the whole view.
    let fp = match marker {
        Some((mx, my)) => session::region_fingerprint(&cap, mx, my, VC_HALF),
        None => session::view_fingerprint(&cap, shown),
    };
    let turn_id = action.map_or_else(telemetry::current_turn, |trace| trace.turn_id);
    let file_name = frame_file_name(frame_id, turn_id, action.map(|trace| trace.action_id), step);
    let path = std::path::Path::new(dir).join(&file_name);
    let artifact_write_ok = match std::fs::write(&path, &jpeg) {
        Ok(()) => true,
        Err(error) => {
            telemetry::artifact_write_failed("frame", &path, action, &error);
            false
        }
    };
    frame_event(
        "frame_ready",
        action,
        json!({
            "frame_id": frame_id,
            "reason": reason,
            "step": step,
            "capture_ms": capture_ms,
            "encode_ms": encode_ms,
            "byte_count": jpeg.len(),
            "artifact_path": file_name,
            "artifact_write_ok": artifact_write_ok,
            "view": [shown.x, shown.y, shown.w, shown.h],
            "marker_screen_px": marker.map(|(x, y)| [x, y]),
        }),
    );
    eprintln!(
        "[cc] step {step:02} frame {frame_id} {} KB -> {}",
        jpeg.len() / 1024,
        path.display()
    );
    Ok((general_purpose::STANDARD.encode(&jpeg), shown, fp, frame_id))
}

fn frame_event(event: &str, action: Option<super::super::telemetry::ActionTrace>, fields: Value) {
    use super::super::telemetry::{self, Privacy};
    match action {
        Some(trace) => telemetry::event_for_action(event, "capture", Privacy::Safe, trace, fields),
        None => telemetry::event(event, "capture", Privacy::Safe, fields),
    }
}

fn frame_file_name(frame_id: u64, turn_id: u64, action_id: Option<u64>, step: usize) -> String {
    let action = action_id.map_or_else(|| "none".to_string(), |id| format!("{id:06}"));
    format!("frame-{frame_id:06}-turn-{turn_id:04}-action-{action}-step-{step:04}.jpg")
}

/// Click at an absolute screen pixel (maps to 0-1000 over the virtual desktop,
/// which the executor turns into the SendInput absolute coordinate). `button` is
/// "left" or "right" (right is for context menus, e.g. "Save image as").
pub(super) fn click_screen(
    sx: i32,
    sy: i32,
    dry: bool,
    button: &str,
    profile: &HumanProfile,
    cancel: &AtomicBool,
) -> Value {
    let (vx, vy, vw, vh) = uia::virtual_desktop();
    let nx = (sx - vx) as f64 / vw.max(1) as f64 * 1000.0;
    let ny = (sy - vy) as f64 / vh.max(1) as f64 * 1000.0;
    if dry {
        return json!({"ok": true, "note": "dry", "screen_px": [sx, sy], "button": button});
    }
    // Grid/vision-located clicks are "uncertain" → humanized executor hesitates
    // on the target so the user can barge in before it commits.
    executor::execute_ex(
        "click",
        &json!({"x": nx, "y": ny, "button": button, "uncertain": true}),
        profile,
        cancel,
    )
}

/// Wrap a pointer executor result with both coordinate spaces used to derive it.
/// Keeping these fields at the top level makes truncated human logs and JSONL
/// telemetry auditable without reverse-engineering the desktop/view transform.
pub(super) fn pointer_result(
    input_result: Value,
    view: View,
    view_norm: (f64, f64),
    screen_px: (i32, i32),
    extra: Value,
) -> Value {
    let ok = input_result
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut fields = match extra {
        Value::Object(fields) => fields,
        _ => serde_json::Map::new(),
    };
    fields.insert("ok".to_string(), json!(ok));
    fields.insert("view_norm".to_string(), json!([view_norm.0, view_norm.1]));
    fields.insert("screen_px".to_string(), json!([screen_px.0, screen_px.1]));
    fields.insert(
        "view_rect".to_string(),
        json!([view.x, view.y, view.w, view.h]),
    );
    fields.insert(
        "coordinate_spaces".to_string(),
        json!({
            "view_norm": "0..1000 relative to view_rect",
            "screen_px": "virtual-desktop pixels",
            "view_rect": "screen pixels [x,y,width,height]",
        }),
    );
    fields.insert("input_result".to_string(), input_result);
    Value::Object(fields)
}

pub(super) fn screen_to_view_norm(view: View, sx: i32, sy: i32) -> (f64, f64) {
    (
        (sx - view.x) as f64 / view.w.max(1) as f64 * 1000.0,
        (sy - view.y) as f64 / view.h.max(1) as f64 * 1000.0,
    )
}

/// Glide the cursor to an absolute screen pixel WITHOUT clicking (point/hover);
/// `dwell_ms` lingers there so a hover tooltip / menu can surface. Mirrors
/// `click_screen` but runs the executor's `point` action (move only).
pub(super) fn point_screen(
    sx: i32,
    sy: i32,
    dwell_ms: u64,
    dry: bool,
    profile: &HumanProfile,
    cancel: &AtomicBool,
) -> Value {
    let (vx, vy, vw, vh) = uia::virtual_desktop();
    let nx = (sx - vx) as f64 / vw.max(1) as f64 * 1000.0;
    let ny = (sy - vy) as f64 / vh.max(1) as f64 * 1000.0;
    if dry {
        return json!({"ok": true, "note": "dry", "screen_px": [sx, sy]});
    }
    executor::execute_ex(
        "point",
        &json!({"x": nx, "y": ny, "dwell_ms": dwell_ms}),
        profile,
        cancel,
    )
}

/// Drag from one absolute screen pixel to another (press, glide, release) - the
/// precise drag for canvas drag-and-drop (place a card on a slot, move a slider).
/// The executor's `drag` takes 0-1000 normalized, so convert screen px back.
pub(super) fn drag_screen(
    fx: i32,
    fy: i32,
    tx: i32,
    ty: i32,
    dry: bool,
    profile: &HumanProfile,
    cancel: &AtomicBool,
) -> Value {
    if dry {
        return json!({"ok": true, "note": "dry", "from_px": [fx, fy], "to_px": [tx, ty]});
    }
    let (fnx, fny) = executor::screen_to_norm(fx, fy);
    let (tnx, tny) = executor::screen_to_norm(tx, ty);
    executor::execute_ex(
        "drag",
        &json!({"x": fnx, "y": fny, "dest_x": tnx, "dest_y": tny}),
        profile,
        cancel,
    )
}

/// A new view magnified to the labeled grid cell (plus a little context), in
/// screen pixels. Returns None if the label is out of range.
pub(super) fn zoom_to_cell(view: View, grid: &Grid, label: u32) -> Option<View> {
    let (fx0, fy0, fx1, fy1) = grid.frac_rect(label, 0.25)?;
    let x0 = view.x + (fx0 * view.w as f64).round() as i32;
    let y0 = view.y + (fy0 * view.h as f64).round() as i32;
    let x1 = view.x + (fx1 * view.w as f64).round() as i32;
    let y1 = view.y + (fy1 * view.h as f64).round() as i32;
    Some(View {
        x: x0,
        y: y0,
        w: (x1 - x0).max(8),
        h: (y1 - y0).max(8),
    })
}

/// Control types we treat as clickable targets.
pub(super) fn is_clickable(ct: &str) -> bool {
    matches!(
        ct,
        "Button"
            | "MenuItem"
            | "TabItem"
            | "ListItem"
            | "CheckBox"
            | "RadioButton"
            | "Edit"
            | "ComboBox"
            | "Hyperlink"
            | "SplitButton"
            | "TreeItem"
            | "Slider"
            | "Tab"
    )
}

/// Inline summary of the read-only text elements (the live "values"), for logging.
pub(super) fn readouts_inline(elements: &[UiElement]) -> String {
    elements
        .iter()
        .filter(|e| e.control_type == "Text" && !e.name.trim().is_empty())
        .map(|e| e.name.as_str())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Order-independent signature of the accessible UI (readout + clickable names),
/// for detecting whether an action changed anything (#2 state-delta).
pub(super) fn state_signature(elements: &[UiElement]) -> String {
    let mut names: Vec<&str> = elements
        .iter()
        .filter(|e| {
            !e.name.trim().is_empty() && (e.control_type == "Text" || is_clickable(e.control_type))
        })
        .map(|e| e.name.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    names.join("|")
}

/// A truncated action signature for the stuck-loop detector (#1).
pub(super) fn compact_args(args: &Value) -> String {
    args.to_string().chars().take(60).collect()
}

/// The structured state the model sees each turn: window title, live READOUTS
/// (Text values), and CLICKABLE elements by exact name. Each element is tagged
/// with the grid cell its center falls in (when inside the current view), giving
/// the model ground-truth spatial anchors in the SAME coordinate system it
/// clicks with — so it can locate canvas/board targets (which have no UIA
/// element) by reasoning relative to the named anchors instead of guessing.
pub(super) fn format_state(
    elements: &[UiElement],
    target: Option<&str>,
    view: View,
    grid: Grid,
) -> String {
    let title = elements
        .iter()
        .find(|e| e.control_type == "Window" && !e.name.trim().is_empty())
        .map(|e| e.name.clone())
        .or_else(|| target.map(str::to_string))
        .unwrap_or_else(|| "(unknown)".to_string());

    let cell_of = |e: &UiElement| -> String {
        let (cx, cy) = e.center();
        let mx = (cx - view.x) as f64 / view.w.max(1) as f64 * 1000.0;
        let my = (cy - view.y) as f64 / view.h.max(1) as f64 * 1000.0;
        if (0.0..=1000.0).contains(&mx) && (0.0..=1000.0).contains(&my) {
            format!(" @cell{}", grid.cell_at(mx, my))
        } else {
            " @off-view".to_string()
        }
    };

    // Dedup identical entries and cap total size: some windows expose enormous,
    // heavily-repeated UIA trees, and an oversized turn payload aborts the Live
    // session. Keep the state compact and unique.
    let mut readouts = String::new();
    let mut clickable = String::new();
    let mut seen = std::collections::HashSet::new();
    for e in elements {
        let name = e.name.trim();
        if name.is_empty() {
            continue;
        }
        if readouts.len() + clickable.len() > 3200 {
            break;
        }
        // Ground-truth control state (on/off/selected/expanded/value) as a tag,
        // so the model reads state as text instead of guessing from pixels.
        let state = e
            .state
            .as_deref()
            .map(|s| format!(" [{s}]"))
            .unwrap_or_default();
        if e.control_type == "Text" {
            let line = format!("- {name}{state}{}\n", cell_of(e));
            if seen.insert(line.clone()) {
                readouts.push_str(&line);
            }
        } else if is_clickable(e.control_type) {
            let flag = if e.enabled { "" } else { " [disabled]" };
            let line = format!(
                "- {} \"{name}\"{flag}{state}{}\n",
                e.control_type,
                cell_of(e)
            );
            if seen.insert(line.clone()) {
                clickable.push_str(&line);
            }
        }
    }
    if readouts.is_empty() {
        readouts.push_str("(none)\n");
    }
    format!(
        "WINDOW: {title}\n\nREADOUTS (live values, with grid cell):\n{readouts}\nCLICKABLE \
(click_target by its name, or click_at its @cellN grid cell; a [tag] like [on]/[off]/[selected]/[expanded]/[value N] \
is its CURRENT state - TRUST that over the screenshot):\n{clickable}\nNote: targets with NO \
UIA element (game boards, canvas, images) are not listed - locate them visually, using the @cell anchors \
above as reference, then zoom that cell and click_at.\n"
    )
}

pub(super) fn wait_for_setup(socket: &mut Sock) -> Result<()> {
    set_socket_short_timeout(socket)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if Instant::now() > deadline {
            anyhow::bail!("timed out waiting for setupComplete");
        }
        let text = match socket.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            Ok(Message::Close(f)) => anyhow::bail!("server closed during setup: {f:?}"),
            Ok(_) => continue,
            Err(e) if is_transient_socket_read_error(&e) => continue,
            Err(e) => anyhow::bail!("setup read error: {e}"),
        };
        for ev in parse_server_message(&text) {
            if matches!(ev, ServerEvent::SetupComplete) {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::frame_file_name;

    #[test]
    fn frame_names_are_unique_and_carry_correlation_ids() {
        let first = frame_file_name(8, 3, Some(21), 5);
        let second = frame_file_name(9, 3, Some(21), 5);

        assert_ne!(first, second);
        assert!(first.contains("turn-0003"));
        assert!(first.contains("action-000021"));
        assert!(first.contains("step-0005"));
    }
}
