//! View capture/rendering, click-coordinate mapping, and the structured
//! element-state formatting — split out of `uia_task.rs` to keep it within the
//! file-size limit. `use super::*` pulls in the shared imports/types/consts.

use super::*;
use base64::{Engine as _, engine::general_purpose};

mod input;
mod semantic_marks;
pub(super) use input::*;
use semantic_marks::semantic_filter_detector_marks;

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

pub(super) struct RenderRequest<'a> {
    pub dir: &'a str,
    pub target: Option<&'a str>,
    pub step: usize,
    pub view: View,
    pub whole_screen: bool,
    pub preserve_view: bool,
    pub bound_source: Option<&'a FrameSource>,
    pub perception_surface: Option<&'a super::super::controller::world::SurfaceIdentity>,
    pub grid: Grid,
    pub marker: Option<(i32, i32)>,
    pub reason: &'a str,
    pub action: Option<super::super::telemetry::ActionTrace>,
    pub existing_marks: &'a [(i32, i32, u32)],
    pub detector_start_id: Option<u32>,
    pub excluded_rects: &'a [[i32; 4]],
    pub show_grid: bool,
}

pub(super) struct Rendered {
    pub frame_b64: String,
    pub view: View,
    pub fingerprint: Vec<u8>,
    pub frame_id: u64,
    pub surface: super::super::controller::world::SurfaceIdentity,
    pub source: FrameSource,
    pub fixed_view_retained: bool,
    pub perception_matched: bool,
    pub detected: Vec<super::super::detector::DetBox>,
}

pub(super) fn render_view(request: RenderRequest<'_>) -> Result<Rendered> {
    let RenderRequest {
        dir,
        target,
        step,
        view: requested_view,
        whole_screen,
        preserve_view,
        bound_source,
        perception_surface,
        grid,
        marker,
        reason,
        action,
        existing_marks,
        detector_start_id,
        excluded_rects,
        show_grid,
    } = request;
    use super::super::telemetry;
    let frame_id = telemetry::next_frame_for(reason, action);
    let capture_t0 = Instant::now();
    let fixed = preserve_view
        .then(|| bound_source.map(|source| (&source.surface, requested_view)))
        .flatten();
    let (cap, frame_surface, view, fixed_view_retained) =
        match super::frame_identity::capture_current(target, fixed, || {
            window_view(target, whole_screen)
        }) {
            Ok(captured) => captured,
            Err(error) => {
                frame_event(
                    "frame_failed",
                    action,
                    json!({"frame_id": frame_id, "reason": reason, "phase": "capture", "error": error.to_string()}),
                );
                return Err(error);
            }
        };
    let source = FrameSource {
        frame_id,
        surface: frame_surface,
    };
    let perception_matched = perception_matches(perception_surface, &source.surface);
    let surface = source.surface.clone();
    let capture_ms = capture_t0.elapsed().as_millis();
    let detector_start_id = perception_matched.then_some(detector_start_id).flatten();
    let excluded_rects = if perception_matched {
        excluded_rects
    } else {
        &[]
    };
    let existing_marks = if perception_matched {
        existing_marks
    } else {
        &[]
    };
    let mut detected = detector_start_id
        .map(|_| super::super::detector::detect_capture(&cap, view, frame_id))
        .unwrap_or_default();
    let detected_before_filter = detected.len();
    detected.retain(|item| {
        !excluded_rects.iter().any(|rect| {
            item.cx >= rect[0] && item.cx <= rect[2] && item.cy >= rect[1] && item.cy <= rect[3]
        })
    });
    if detected.len() != detected_before_filter {
        telemetry::event(
            "detector_anchor_filter",
            "detector",
            telemetry::Privacy::Safe,
            json!({
                "frame_id": frame_id,
                "before": detected_before_filter,
                "after": detected.len(),
                "reason": "center_overlaps_accessible_control",
            }),
        );
    }
    let filtered_count = detected.len();
    detected =
        super::super::detector::select_marks(detected, view, super::super::detector::DISPLAY_MARKS);
    if detected.len() != filtered_count {
        telemetry::event(
            "detector_anchor_cap",
            "detector",
            telemetry::Privacy::Safe,
            json!({
                "frame_id": frame_id,
                "before": filtered_count,
                "after": detected.len(),
                "strategy": "spatial_coverage_then_confidence",
            }),
        );
    }
    if let Some(first_id) = detector_start_id {
        detected = semantic_filter_detector_marks(&cap, view, frame_id, first_id, detected);
    }
    let detector_marks: Vec<(i32, i32, u32)> = detector_start_id
        .map(|first| {
            detected
                .iter()
                .enumerate()
                .map(|(index, item)| (item.cx, item.cy, first.saturating_add(index as u32)))
                .collect()
        })
        .unwrap_or_default();
    let marks = if detector_start_id.is_some() {
        detector_marks.as_slice()
    } else {
        existing_marks
    };
    // One coordinate vocabulary per frame: mark IDs are precise, while drawing
    // the coarse grid at the same time doubles label noise and creates collisions.
    let shown_grid = (perception_matched && show_grid && marks.is_empty()).then_some(grid);
    let encode_t0 = Instant::now();
    let (jpeg, shown) = match session::encode_view(
        &cap,
        view,
        VIEW_SHORT,
        shown_grid,
        marker,
        Some(marks),
    ) {
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
            "surface": surface,
            "frame_source": &source,
            "perception_matched": perception_matched,
            "marker_screen_px": marker.map(|(x, y)| [x, y]),
            "coarse_grid_shown": shown_grid.is_some(),
        }),
    );
    eprintln!(
        "[cc] step {step:02} frame {frame_id} {} KB -> {}",
        jpeg.len() / 1024,
        path.display()
    );
    Ok(Rendered {
        frame_b64: general_purpose::STANDARD.encode(&jpeg),
        view: shown,
        fingerprint: fp,
        frame_id,
        surface,
        source,
        fixed_view_retained,
        perception_matched,
        detected,
    })
}

fn perception_matches(
    perception: Option<&super::super::controller::world::SurfaceIdentity>,
    frame: &super::super::controller::world::SurfaceIdentity,
) -> bool {
    perception == Some(frame)
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

/// Order-independent signature of accessible identity, value, state, and geometry.
pub(super) fn state_signature(elements: &[UiElement]) -> String {
    let mut states: Vec<String> = elements
        .iter()
        .filter(|e| {
            !e.name.trim().is_empty() && (e.control_type == "Text" || is_clickable(e.control_type))
        })
        .map(|e| {
            format!(
                "{}|{}|value={:?}|state={:?}|enabled={}|required={}|rect={},{},{},{}",
                e.control_type,
                e.name,
                e.value,
                e.state,
                e.enabled,
                e.required,
                e.left,
                e.top,
                e.right,
                e.bottom,
            )
        })
        .collect();
    states.sort_unstable();
    states.dedup();
    states.join("\n")
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
    show_grid: bool,
    indexed_controls: Option<&str>,
) -> String {
    let title = elements
        .iter()
        .find(|e| e.control_type == "Window" && !e.name.trim().is_empty())
        .map(|e| e.name.clone())
        .or_else(|| {
            let title = uia::pointer_context().0;
            (!title.trim().is_empty()).then_some(title)
        })
        .or_else(|| target.map(str::to_string))
        .unwrap_or_else(|| "(unknown)".to_string());

    let cell_of = |e: &UiElement| -> String {
        if !show_grid {
            return String::new();
        }
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
    let controls = indexed_controls.unwrap_or(&clickable);
    let spatial = if show_grid { ", with grid cell" } else { "" };
    let blind_route = if show_grid {
        "use a detector mark when present, otherwise zoom a grid cell before click_at"
    } else {
        "use a detector mark when present, otherwise use the vision targeting tools"
    };
    format!(
        "WINDOW: {title}\n\nREADOUTS (live values{spatial}):\n{readouts}\nINDEXED CONTROLS \
(act by @id; click is one ordinary click, activate enters/opens; current [state] is ground truth):\n\
{controls}\nTargets with no accessible control are not listed; {blind_route}.\n"
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
    use super::{frame_file_name, perception_matches};
    use crate::overlay::computer_control::controller::world::{
        BrowserWindowIdentity, SurfaceIdentity,
    };

    fn browser_window() -> BrowserWindowIdentity {
        BrowserWindowIdentity {
            browser_window_id: 2,
            hwnd: 3,
            pid: 4,
            generation: 5,
        }
    }

    #[test]
    fn frame_names_are_unique_and_carry_correlation_ids() {
        let first = frame_file_name(8, 3, Some(21), 5);
        let second = frame_file_name(9, 3, Some(21), 5);

        assert_ne!(first, second);
        assert!(first.contains("turn-0003"));
        assert!(first.contains("action-000021"));
        assert!(first.contains("step-0005"));
    }

    #[test]
    fn overlays_require_the_same_document_or_native_generation_as_pixels() {
        let browser_frame = SurfaceIdentity::Browser {
            tab_id: 8,
            document_id: "doc-new".into(),
            window: browser_window(),
        };
        let browser_state = SurfaceIdentity::Browser {
            tab_id: 8,
            document_id: "doc-old".into(),
            window: browser_window(),
        };
        let native_frame = SurfaceIdentity::Native {
            hwnd: 3,
            pid: 4,
            generation: 6,
        };
        let native_state = SurfaceIdentity::Native {
            hwnd: 3,
            pid: 4,
            generation: 5,
        };

        assert!(!perception_matches(Some(&browser_state), &browser_frame));
        assert!(!perception_matches(Some(&native_state), &native_frame));
        assert!(perception_matches(Some(&browser_frame), &browser_frame));
        assert!(!perception_matches(None, &browser_frame));
    }
}
