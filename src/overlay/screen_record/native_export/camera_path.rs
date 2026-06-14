// Rust port of videoRenderer.ts camera path generation.
// Mirrors generateBakedPath + calculateCurrentZoomStateInternal + blendZoomStates.

use super::config::{BakedCameraFrame, VideoSegment, ZoomBlock};

// Internal zoom state in [0,1] anchor space.
#[derive(Clone)]
struct ZoomState {
    zoom_factor: f64,
    position_x: f64,
    position_y: f64,
}

const DEFAULT_STATE: ZoomState = ZoomState {
    zoom_factor: 1.0,
    position_x: 0.5,
    position_y: 0.5,
};

// Perlin's smootherStep: 6t⁵ - 15t⁴ + 10t³
fn ease_camera_move(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

// Convert zoom anchor params → visible viewport center.
fn to_viewport_center(zoom: f64, pos_x: f64, pos_y: f64) -> (f64, f64) {
    if zoom <= 1.0 {
        return (0.5, 0.5);
    }
    (pos_x + (0.5 - pos_x) / zoom, pos_y + (0.5 - pos_y) / zoom)
}

// Convert visible viewport center → zoom anchor params.
fn from_viewport_center(zoom: f64, cx: f64, cy: f64) -> (f64, f64) {
    if zoom <= 1.001 {
        return (cx, cy);
    }
    let s = 1.0 - 1.0 / zoom;
    ((cx - 0.5 / zoom) / s, (cy - 0.5 / zoom) / s)
}

// Blend two zoom states: log-space zoom + viewport-center-space position.
fn blend_zoom(a: &ZoomState, b: &ZoomState, t: f64) -> ZoomState {
    let za = a.zoom_factor.max(0.1);
    let zb = b.zoom_factor.max(0.1);
    let zoom = za * (zb / za).powf(t);
    let (cax, cay) = to_viewport_center(za, a.position_x, a.position_y);
    let (cbx, cby) = to_viewport_center(zb, b.position_x, b.position_y);
    let cx = cax + (cbx - cax) * t;
    let cy = cay + (cby - cay) * t;
    let (pos_x, pos_y) = from_viewport_center(zoom, cx, cy);
    ZoomState {
        zoom_factor: zoom,
        position_x: pos_x,
        position_y: pos_y,
    }
}

// Manual-zoom block envelope: 0 outside the block, ramps up over ease_in,
// holds at 1 across the body, ramps down over ease_out. Mirrors the TS
// zoomBlockEnvelope exactly (WYSIWYG export parity).
fn zoom_block_envelope(b: &ZoomBlock, t: f64) -> f64 {
    if t <= b.start_time || t >= b.end_time {
        return 0.0;
    }
    let dur = b.end_time - b.start_time;
    if dur <= 1e-6 {
        return 0.0;
    }
    let mut ease_in = b.ease_in.max(0.0);
    let mut ease_out = b.ease_out.max(0.0);
    if ease_in + ease_out > dur {
        let s = dur / (ease_in + ease_out);
        ease_in *= s;
        ease_out *= s;
    }
    let t_in = b.start_time + ease_in;
    let t_out = b.end_time - ease_out;
    if t < t_in && ease_in > 1e-6 {
        return ease_camera_move((t - b.start_time) / ease_in);
    }
    if t > t_out && ease_out > 1e-6 {
        return ease_camera_move((b.end_time - t) / ease_out);
    }
    1.0
}

// Port of calculateCurrentZoomStateInternal for the export case
// (srcCropW = viewW = croppedW → contain-fit is identity → posX/posY = relX/relY).
fn calculate_zoom_state(
    current_time: f64,
    segment: &VideoSegment,
    cropped_w: f64,
    cropped_h: f64,
) -> ZoomState {
    let crop = segment.crop.as_ref();
    let (crop_x, crop_y, crop_w_frac, crop_h_frac) = crop
        .map(|c| (c.x, c.y, c.width, c.height))
        .unwrap_or((0.0, 0.0, 1.0, 1.0));

    // --- 1. AUTO STATE from smoothMotionPath ---
    let auto_state: Option<ZoomState> = if !segment.smooth_motion_path.is_empty() {
        let path = &segment.smooth_motion_path;
        let vid_full_w = cropped_w / crop_w_frac;
        let vid_full_h = cropped_h / crop_h_frac;

        // Binary search: find first path point with time >= current_time
        let (mut cam_x, mut cam_y, mut cam_zoom) =
            match path.partition_point(|p| p.time < current_time) {
                0 => {
                    let p = &path[0];
                    (p.x, p.y, p.zoom)
                }
                i if i >= path.len() => {
                    let p = path.last().unwrap();
                    (p.x, p.y, p.zoom)
                }
                i => {
                    let p1 = &path[i - 1];
                    let p2 = &path[i];
                    let span = p2.time - p1.time;
                    let t = if span > 1e-10 {
                        (current_time - p1.time) / span
                    } else {
                        0.0
                    };
                    (
                        p1.x + (p2.x - p1.x) * t,
                        p1.y + (p2.y - p1.y) * t,
                        p1.zoom + (p2.zoom - p1.zoom) * t,
                    )
                }
            };

        // Apply zoomInfluencePoints
        if !segment.zoom_influence_points.is_empty() {
            let pts = &segment.zoom_influence_points;
            let influence = match pts.partition_point(|p| p.time < current_time) {
                0 => pts[0].value,
                i if i >= pts.len() => pts.last().unwrap().value,
                i => {
                    let ip1 = &pts[i - 1];
                    let ip2 = &pts[i];
                    let span = ip2.time - ip1.time;
                    let it = if span > 1e-10 {
                        (current_time - ip1.time) / span
                    } else {
                        0.0
                    };
                    let cos_t = (1.0 - (it * std::f64::consts::PI).cos()) / 2.0;
                    ip1.value * (1.0 - cos_t) + ip2.value * cos_t
                }
            };
            let center_x = vid_full_w * crop_x + cropped_w / 2.0;
            let center_y = vid_full_h * crop_y + cropped_h / 2.0;
            cam_zoom = 1.0 + (cam_zoom - 1.0) * influence;
            cam_x = center_x + (cam_x - center_x) * influence;
            cam_y = center_y + (cam_y - center_y) * influence;
        }

        // Convert video pixel coords → posX/posY [0,1] anchor space.
        // For export: srcCropW = viewW = croppedW → contain-fit is identity → posX = relX.
        let full_w = cropped_w / crop_w_frac;
        let full_h = cropped_h / crop_h_frac;
        let crop_off_x = full_w * crop_x;
        let crop_off_y = full_h * crop_y;
        let pos_x = ((cam_x - crop_off_x) / cropped_w).clamp(0.0, 1.0);
        let pos_y = ((cam_y - crop_off_y) / cropped_h).clamp(0.0, 1.0);

        Some(ZoomState {
            zoom_factor: cam_zoom,
            position_x: pos_x,
            position_y: pos_y,
        })
    } else {
        None
    };

    // --- 2. MANUAL ZOOM BLOCK STATE ---
    // Pick the enabled block with the strongest envelope at current_time. Gaps
    // between blocks yield 0 → the auto path / default shows through.
    let (manual_state, manual_influence): (Option<ZoomState>, f64) = {
        let mut best_env = 0.0_f64;
        let mut best: Option<&ZoomBlock> = None;
        for b in &segment.zoom_blocks {
            if !b.enabled {
                continue;
            }
            let env = zoom_block_envelope(b, current_time);
            if env > best_env {
                best_env = env;
                best = Some(b);
            }
        }
        match best {
            Some(b) if best_env > 0.0 => {
                let (px, py) = if b.follow_cursor {
                    match &auto_state {
                        Some(a) => (a.position_x, a.position_y),
                        None => (b.position_x, b.position_y),
                    }
                } else {
                    (b.position_x, b.position_y)
                };
                (
                    Some(ZoomState {
                        zoom_factor: b.zoom_factor,
                        position_x: px,
                        position_y: py,
                    }),
                    best_env,
                )
            }
            _ => (None, 0.0),
        }
    };

    // --- 3. FINAL BLEND ---
    let result = if let Some(auto) = &auto_state {
        if let Some(manual) = &manual_state {
            if manual_influence > 0.001 {
                blend_zoom(auto, manual, manual_influence)
            } else {
                auto.clone()
            }
        } else {
            auto.clone()
        }
    } else if let Some(manual) = &manual_state {
        if manual_influence > 0.001 {
            blend_zoom(&DEFAULT_STATE, manual, manual_influence)
        } else {
            DEFAULT_STATE
        }
    } else {
        return DEFAULT_STATE;
    };

    ZoomState {
        zoom_factor: result.zoom_factor,
        position_x: result.position_x.clamp(0.0, 1.0),
        position_y: result.position_y.clamp(0.0, 1.0),
    }
}

/// Generate baked camera path in Rust.
/// Mirrors TypeScript generateBakedPath(segment, sourceWidth, sourceHeight, fps).
/// Output: {time, x(globalPx), y(globalPx), zoom}[] indexed in SOURCE time.
pub fn generate_camera_path(
    segment: &VideoSegment,
    source_width: u32,
    source_height: u32,
    fps: u32,
) -> Vec<BakedCameraFrame> {
    if segment.trim_segments.is_empty() {
        eprintln!("[CameraPath] No trim segments — skipping camera path generation");
        return vec![];
    }

    let crop = segment.crop.as_ref();
    let (crop_x, crop_y, crop_w_frac, crop_h_frac) = crop
        .map(|c| (c.x, c.y, c.width, c.height))
        .unwrap_or((0.0, 0.0, 1.0, 1.0));

    let cropped_w = source_width as f64 * crop_w_frac;
    let cropped_h = source_height as f64 * crop_h_frac;
    let crop_offset_x = source_width as f64 * crop_x;
    let crop_offset_y = source_height as f64 * crop_y;

    let full_start = segment.trim_segments[0].start_time;
    let full_end = segment.trim_segments.last().unwrap().end_time;

    let step = 1.0 / fps as f64;
    let n_frames = ((full_end - full_start) / step).ceil() as usize + 2;
    let mut frames = Vec::with_capacity(n_frames);

    let mut t = full_start;
    loop {
        let state = calculate_zoom_state(t, segment, cropped_w, cropped_h);
        let global_x = crop_offset_x + state.position_x * cropped_w;
        let global_y = crop_offset_y + state.position_y * cropped_h;
        frames.push(BakedCameraFrame {
            time: t,
            x: global_x,
            y: global_y,
            zoom: state.zoom_factor,
        });

        if t >= full_end - 1e-9 {
            break;
        }
        t = (t + step).min(full_end);
    }

    eprintln!(
        "[CameraPath] Generated {} frames [{:.3}s..{:.3}s] at {}fps",
        frames.len(),
        full_start,
        full_end,
        fps
    );
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cross-language render-math golden. The TS preview (cameraZoom.ts:
    // calculateCurrentZoomStateInternal / zoomBlockEnvelope) is canonical; this
    // Rust export must reproduce the SAME fixture within 1e-6 — this is the
    // WYSIWYG (preview == export) lock. Regenerate via
    // screen-record/tests/unit/_generateRenderGolden.gen.ts.
    // See .claude/parity/render-camera-cursor.md.
    const GOLDEN: &str =
        include_str!("../../../../parity-fixtures/render-camera-cursor/golden.json");

    fn golden() -> serde_json::Value {
        serde_json::from_str(GOLDEN).expect("golden fixture parses")
    }

    // ── Pure-helper behavior ──────────────────────────────────────────────────

    #[test]
    fn ease_camera_move_endpoints_and_midpoint() {
        // smootherStep: clamped at the ends, symmetric, 0.5 at t=0.5.
        assert_eq!(ease_camera_move(-0.5), 0.0);
        assert_eq!(ease_camera_move(0.0), 0.0);
        assert_eq!(ease_camera_move(1.0), 1.0);
        assert_eq!(ease_camera_move(1.5), 1.0);
        assert!((ease_camera_move(0.5) - 0.5).abs() < 1e-12);
        // Monotonic increasing on [0,1].
        let mut prev = ease_camera_move(0.0);
        for i in 1..=20 {
            let v = ease_camera_move(i as f64 / 20.0);
            assert!(v >= prev - 1e-12);
            prev = v;
        }
    }

    #[test]
    fn viewport_center_round_trips() {
        // from_viewport_center(to_viewport_center(..)) is identity for zoom > 1.
        for &zoom in &[1.5_f64, 2.0, 3.7] {
            for &(px, py) in &[(0.2_f64, 0.8_f64), (0.5, 0.5), (0.9, 0.1)] {
                let (cx, cy) = to_viewport_center(zoom, px, py);
                let (rx, ry) = from_viewport_center(zoom, cx, cy);
                assert!((rx - px).abs() < 1e-9, "posX round-trip drift");
                assert!((ry - py).abs() < 1e-9, "posY round-trip drift");
            }
        }
        // At zoom <= 1 the viewport center collapses to the canvas center.
        assert_eq!(to_viewport_center(1.0, 0.2, 0.8), (0.5, 0.5));
    }

    #[test]
    fn blend_zoom_hits_endpoints() {
        let a = ZoomState { zoom_factor: 1.0, position_x: 0.5, position_y: 0.5 };
        let b = ZoomState { zoom_factor: 2.5, position_x: 0.2, position_y: 0.7 };
        let at0 = blend_zoom(&a, &b, 0.0);
        assert!((at0.zoom_factor - a.zoom_factor).abs() < 1e-9);
        assert!((at0.position_x - a.position_x).abs() < 1e-9);
        assert!((at0.position_y - a.position_y).abs() < 1e-9);
        let at1 = blend_zoom(&a, &b, 1.0);
        assert!((at1.zoom_factor - b.zoom_factor).abs() < 1e-9);
        assert!((at1.position_x - b.position_x).abs() < 1e-9);
        assert!((at1.position_y - b.position_y).abs() < 1e-9);
    }

    #[test]
    fn zoom_block_envelope_edges() {
        let b = ZoomBlock {
            start_time: 2.0,
            end_time: 8.0,
            ease_in: 1.0,
            ease_out: 1.0,
            zoom_factor: 1.5,
            position_x: 0.5,
            position_y: 0.5,
            follow_cursor: false,
            enabled: true,
        };
        assert_eq!(zoom_block_envelope(&b, 1.9), 0.0); // before
        assert_eq!(zoom_block_envelope(&b, 8.1), 0.0); // after
        assert_eq!(zoom_block_envelope(&b, 5.0), 1.0); // hold
        let mid_in = zoom_block_envelope(&b, 2.5);
        assert!(mid_in > 0.0 && mid_in < 1.0); // ramp-in
        let mid_out = zoom_block_envelope(&b, 7.5);
        assert!(mid_out > 0.0 && mid_out < 1.0); // ramp-out
    }

    // ── Cross-language golden ─────────────────────────────────────────────────

    #[test]
    fn camera_cases_match_golden() {
        let g = golden();
        let tol = g["tolerance"].as_f64().unwrap();
        for case in g["camera"]["cases"].as_array().unwrap() {
            let view = case["view"].as_f64().unwrap();
            let seg: VideoSegment =
                serde_json::from_value(case["segment"].clone()).expect("segment parses");
            // crop=null + view==source means contain-fit is identity, so the Rust
            // anchor posX/posY matches the TS calculateCurrentZoomStateInternal output.
            for s in case["samples"].as_array().unwrap() {
                let t = s["t"].as_f64().unwrap();
                let st = calculate_zoom_state(t, &seg, view, view);
                let name = case["name"].as_str().unwrap();
                assert!(
                    (st.zoom_factor - s["zoom"].as_f64().unwrap()).abs() <= tol,
                    "{name}: zoom drift at t={t}"
                );
                assert!(
                    (st.position_x - s["posX"].as_f64().unwrap()).abs() <= tol,
                    "{name}: posX drift at t={t}"
                );
                assert!(
                    (st.position_y - s["posY"].as_f64().unwrap()).abs() <= tol,
                    "{name}: posY drift at t={t}"
                );
            }
        }
    }

    #[test]
    fn zoom_block_envelope_matches_golden() {
        let g = golden();
        let tol = g["tolerance"].as_f64().unwrap();
        let env = &g["camera"]["zoomBlockEnvelope"];
        let b: ZoomBlock =
            serde_json::from_value(env["block"].clone()).expect("envelope block parses");
        for s in env["samples"].as_array().unwrap() {
            let got = zoom_block_envelope(&b, s["t"].as_f64().unwrap());
            assert!(
                (got - s["value"].as_f64().unwrap()).abs() <= tol,
                "envelope drift at t={}",
                s["t"]
            );
        }
    }
}
