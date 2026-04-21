// Rust port of videoRenderer.ts camera path generation.
// Mirrors generateBakedPath + calculateCurrentZoomStateInternal + blendZoomStates.

use super::config::{BakedCameraFrame, VideoSegment, ZoomKeyframe};

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
    ZoomState { zoom_factor: zoom, position_x: pos_x, position_y: pos_y }
}

// Adaptive blending window: larger for bigger movements.
fn dynamic_window(az: f64, ax: f64, ay: f64, bz: f64, bx: f64, by: f64) -> f64 {
    let dx = (ax - bx).abs();
    let dy = (ay - by).abs();
    let dz = (az - bz).abs();
    let score = (dx * dx + dy * dy).sqrt() + dz * 0.5;
    (score * 3.0).clamp(1.5, 4.0)
}

// Port of calculateCurrentZoomStateInternal for the export case
// (srcCropW = viewW = croppedW → contain-fit is identity → posX/posY = relX/relY).
fn calculate_zoom_state(current_time: f64, segment: &VideoSegment, cropped_w: f64, cropped_h: f64) -> ZoomState {
    let crop = segment.crop.as_ref();
    let (crop_x, crop_y, crop_w_frac, crop_h_frac) = crop
        .map(|c| (c.x, c.y, c.width, c.height))
        .unwrap_or((0.0, 0.0, 1.0, 1.0));

    // --- 1. AUTO STATE from smoothMotionPath ---
    let auto_state: Option<ZoomState> = if !segment.smooth_motion_path.is_empty() {
        let path = &segment.smooth_motion_path;
        let vid_full_w = cropped_w / crop_w_frac;
        let vid_full_h = cropped_h / crop_h_frac;
        let default_x = vid_full_w * crop_x + cropped_w / 2.0;
        let default_y = vid_full_h * crop_y + cropped_h / 2.0;

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
                    let t = if span > 1e-10 { (current_time - p1.time) / span } else { 0.0 };
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
                    let it = if span > 1e-10 { (current_time - ip1.time) / span } else { 0.0 };
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
        let _ = (default_x, default_y); // suppress unused

        // Convert video pixel coords → posX/posY [0,1] anchor space.
        // For export: srcCropW = viewW = croppedW → contain-fit is identity → posX = relX.
        let full_w = cropped_w / crop_w_frac;
        let full_h = cropped_h / crop_h_frac;
        let crop_off_x = full_w * crop_x;
        let crop_off_y = full_h * crop_y;
        let pos_x = ((cam_x - crop_off_x) / cropped_w).clamp(0.0, 1.0);
        let pos_y = ((cam_y - crop_off_y) / cropped_h).clamp(0.0, 1.0);

        Some(ZoomState { zoom_factor: cam_zoom, position_x: pos_x, position_y: pos_y })
    } else {
        None
    };

    let has_auto = auto_state.is_some();

    // --- 2. MANUAL KEYFRAME STATE ---
    let mut sorted_kfs: Vec<&ZoomKeyframe> = segment.zoom_keyframes.iter().collect();
    sorted_kfs.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));

    let (manual_state, manual_influence): (Option<ZoomState>, f64) = if sorted_kfs.is_empty() {
        (None, 0.0)
    } else {
        let next_idx = sorted_kfs.partition_point(|k| k.time <= current_time);
        // prev_kf = last keyframe with time <= current_time
        let prev_kf = if next_idx > 0 { Some(sorted_kfs[next_idx - 1]) } else { None };
        // next_kf = first keyframe with time > current_time
        let next_kf = sorted_kfs.get(next_idx).copied();

        if let (Some(prev), Some(next)) = (prev_kf, next_kf) {
            // BETWEEN two keyframes — full influence, smoothly interpolate
            let span = next.time - prev.time;
            let raw_t = if span > 1e-10 { (current_time - prev.time) / span } else { 1.0 };
            let eased_t = ease_camera_move(raw_t.clamp(0.0, 1.0));
            let prev_z = ZoomState { zoom_factor: prev.zoom_factor, position_x: prev.position_x, position_y: prev.position_y };
            let next_z = ZoomState { zoom_factor: next.zoom_factor, position_x: next.position_x, position_y: next.position_y };
            (Some(blend_zoom(&prev_z, &next_z, eased_t)), 1.0)
        } else if let Some(prev) = prev_kf {
            // AFTER LAST KEYFRAME — decay back to auto
            let prev_z = ZoomState { zoom_factor: prev.zoom_factor, position_x: prev.position_x, position_y: prev.position_y };
            if has_auto {
                let target = auto_state.as_ref().unwrap();
                let window = dynamic_window(
                    prev.zoom_factor, prev.position_x, prev.position_y,
                    target.zoom_factor, target.position_x, target.position_y,
                );
                let elapsed = current_time - prev.time;
                let influence = if elapsed < window { 1.0 - ease_camera_move(elapsed / window) } else { 0.0 };
                (Some(prev_z), influence)
            } else {
                // No auto path — hold keyframe forever
                (Some(prev_z), 1.0)
            }
        } else if let Some(next) = next_kf {
            // BEFORE FIRST KEYFRAME — ramp up to keyframe
            let next_z = ZoomState { zoom_factor: next.zoom_factor, position_x: next.position_x, position_y: next.position_y };
            let target = auto_state.as_ref().unwrap_or(&DEFAULT_STATE);
            let window = if next.duration > 0.0 {
                next.duration
            } else {
                dynamic_window(
                    next.zoom_factor, next.position_x, next.position_y,
                    target.zoom_factor, target.position_x, target.position_y,
                )
            };
            let time_to_next = next.time - current_time;
            let influence = if time_to_next <= window {
                ease_camera_move(1.0 - time_to_next / window)
            } else {
                0.0
            };
            (Some(next_z), influence)
        } else {
            (None, 0.0)
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
        frames.push(BakedCameraFrame { time: t, x: global_x, y: global_y, zoom: state.zoom_factor });

        if t >= full_end - 1e-9 { break; }
        t = (t + step).min(full_end);
    }

    eprintln!("[CameraPath] Generated {} frames [{:.3}s..{:.3}s] at {}fps", frames.len(), full_start, full_end, fps);
    frames
}
