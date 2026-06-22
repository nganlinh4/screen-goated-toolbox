//! View capture/rendering, click-coordinate mapping, and the structured
//! element-state formatting — split out of `uia_task.rs` to keep it within the
//! file-size limit. `use super::*` pulls in the shared imports/types/consts.

use super::*;

/// The view the model is grounded on. DEFAULT = the ACTIVE window (precise clicks,
/// no whole-screen ambiguity) - the target window if `target` is set, else the
/// foreground one. When `whole_screen` is set (the model called see_whole_screen for
/// awareness / to find another window) it's the WHOLE virtual desktop instead.
pub(super) fn window_view(target: Option<&str>, whole_screen: bool) -> View {
    if !whole_screen
        && let Some((x, y, w, h)) = uia::target_window_rect(target)
    {
        return View { x, y, w, h };
    }
    let (x, y, w, h) = uia::virtual_desktop();
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
    std::env::var("CC_VC_MIN").ok().and_then(|s| s.parse().ok()).unwrap_or(14)
}

pub(super) fn render_view(
    dir: &str,
    step: usize,
    view: View,
    grid: Grid,
    marker: Option<(i32, i32)>,
) -> Result<(String, View, Vec<u8>)> {
    let cap = session::capture_virtual()?;
    let (jpeg, shown) = session::encode_view(&cap, view, VIEW_SHORT, Some(grid), marker)?;
    // Fingerprint the CLEAN region around the click (no grid/marker overlay), so we
    // can tell whether the click changed ITS OWN cell - ignoring a timer/animation
    // elsewhere. With no marker (turn 0 / keyboard) fall back to the whole view.
    let fp = match marker {
        Some((mx, my)) => session::region_fingerprint(&cap, mx, my, VC_HALF),
        None => session::view_fingerprint(&cap, shown),
    };
    std::fs::write(format!("{dir}/step-{step:02}.jpg"), &jpeg).ok();
    eprintln!("[cc] step {step:02} frame {} KB", jpeg.len() / 1024);
    Ok((general_purpose::STANDARD.encode(&jpeg), shown, fp))
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
        .filter(|e| !e.name.trim().is_empty() && (e.control_type == "Text" || is_clickable(e.control_type)))
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
pub(super) fn format_state(elements: &[UiElement], target: Option<&str>, view: View, grid: Grid) -> String {
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
        let state = e.state.as_deref().map(|s| format!(" [{s}]")).unwrap_or_default();
        if e.control_type == "Text" {
            let line = format!("- {name}{state}{}\n", cell_of(e));
            if seen.insert(line.clone()) {
                readouts.push_str(&line);
            }
        } else if is_clickable(e.control_type) {
            let flag = if e.enabled { "" } else { " [disabled]" };
            let line = format!("- {} \"{name}\"{flag}{state}{}\n", e.control_type, cell_of(e));
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
(click_element by exact name; @cellN = its grid cell; a [tag] like [on]/[off]/[selected]/[expanded]/[value N] \
is its CURRENT state - TRUST that over the screenshot):\n{clickable}\nNote: targets with NO \
UIA element (game boards, canvas, images) are not listed - locate them visually, using the @cell anchors \
above as reference, then zoom that cell and click_at.\n"
    )
}

/// Resolve an element by exact name (case-insensitive) and click its true center.
/// Prefers an enabled, on-screen match; falls back to a unique substring match.
pub(super) fn click_by_name(
    elements: &[UiElement],
    want: &str,
    dry: bool,
    profile: &HumanProfile,
    cancel: &AtomicBool,
) -> Value {
    let want_l = want.trim().to_lowercase();
    if want_l.is_empty() {
        return json!({"ok": false, "error": "missing name"});
    }
    let exact: Vec<&UiElement> = elements
        .iter()
        .filter(|e| e.name.to_lowercase() == want_l)
        .collect();
    let candidates = if !exact.is_empty() {
        exact
    } else {
        elements
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&want_l))
            .collect()
    };
    let Some(e) = candidates.iter().find(|e| e.enabled).or_else(|| candidates.first()) else {
        return json!({"ok": false, "error": format!("no element named '{want}' on screen")});
    };
    if !e.enabled {
        return json!({"ok": false, "error": format!("element '{}' is disabled", e.name)});
    }
    let (cx, cy) = e.center();
    let (vx, vy, vw, vh) = uia::virtual_desktop();
    let nx = (cx - vx) as f64 / vw.max(1) as f64 * 1000.0;
    let ny = (cy - vy) as f64 / vh.max(1) as f64 * 1000.0;
    if dry {
        return json!({"ok": true, "note": "dry", "clicked": e.name, "norm": [nx.round(), ny.round()], "screen_px": [cx, cy]});
    }
    // Pass the element's true width so the humanized cursor's Fitts-law timing
    // and aim-jitter scale to the real target size.
    let target_w = (e.right - e.left).max(1) as f64;
    let r = executor::execute_ex(
        "click",
        &json!({"x": nx, "y": ny, "target_w": target_w}),
        profile,
        cancel,
    );
    json!({"ok": true, "clicked": e.name, "control_type": e.control_type, "result": r, "screen_px": [cx, cy]})
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
