//! Identity-bracketed frames used by the continuous voice runtime.

use super::*;
use base64::{Engine as _, engine::general_purpose};

/// A gridded snapshot of the current foreground window with no click marker.
#[derive(Clone)]
pub(in crate::overlay::computer_control) struct SnapshotFrame {
    pub b64: String,
    pub source: FrameSource,
    pub captured_at: Instant,
    pub byte_count: usize,
}

pub(in crate::overlay::computer_control) fn snapshot(
    target: Option<&str>,
) -> Result<SnapshotFrame> {
    let origin_turn_id = super::super::telemetry::current_turn();
    let frame_id = super::super::telemetry::next_frame("snapshot");
    let capture_t0 = Instant::now();
    let (cap, surface, view, _) =
        super::frame_identity::capture_current(target, None, || window_view(target, false))?;
    let browser_structured = matches!(
        surface,
        super::super::controller::world::SurfaceIdentity::Browser { .. }
    );
    let captured_at = Instant::now();
    let capture_ms = capture_t0.elapsed().as_millis();
    let encode_t0 = Instant::now();
    let (elements, accessibility_observed) = if browser_structured {
        (Vec::new(), true)
    } else {
        match uia::enumerate(target) {
            Ok(elements) => (elements, true),
            Err(_) => (Vec::new(), false),
        }
    };
    super::frame_identity::validate_current(target, &surface)?;
    let show_grid = !browser_structured
        && accessibility_observed
        && accessible_rects(&elements, view).is_empty();
    let grid = Grid::from_env();
    let (jpeg, _) = session::encode_view(
        &cap,
        view,
        VIEW_SHORT,
        show_grid.then_some(grid),
        None,
        None,
    )?;
    let window_title = super::super::uia::pointer_context().0;
    let artifact_name = if frame_id == 1 || frame_id.is_multiple_of(30) {
        let name = format!("live-frame-{frame_id:06}.jpg");
        let path = super::super::telemetry::trace_dir().join(&name);
        match std::fs::write(&path, &jpeg) {
            Ok(()) => Some(name),
            Err(error) => {
                super::super::telemetry::artifact_write_failed("live_frame", &path, None, &error);
                None
            }
        }
    } else {
        None
    };
    super::super::telemetry::frame_ready(super::super::telemetry::FrameReady {
        turn_id: origin_turn_id,
        frame_id,
        reason: "snapshot",
        capture_ms,
        encode_ms: encode_t0.elapsed().as_millis(),
        byte_count: jpeg.len(),
        target,
        view: [view.x, view.y, view.w, view.h],
        window_title: &window_title,
        artifact_path: artifact_name.as_deref(),
    });
    super::super::telemetry::event(
        "frame_surface_bound",
        "capture",
        super::super::telemetry::Privacy::Safe,
        json!({"frame_id": frame_id, "surface": &surface}),
    );
    Ok(SnapshotFrame {
        b64: general_purpose::STANDARD.encode(&jpeg),
        source: FrameSource { frame_id, surface },
        captured_at,
        byte_count: jpeg.len(),
    })
}
