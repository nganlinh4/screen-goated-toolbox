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

type HyperboloidPoint = [f64; 4];

fn camera_to_hyperboloid(cx: f64, cy: f64, altitude: f64) -> HyperboloidPoint {
    let v = altitude.max(1e-6);
    let radius_sq = cx * cx + cy * cy;
    [
        (radius_sq + v * v + 1.0) / (2.0 * v),
        cx / v,
        cy / v,
        (radius_sq + v * v - 1.0) / (2.0 * v),
    ]
}

fn hyperboloid_to_camera(point: HyperboloidPoint) -> (f64, f64, f64) {
    let altitude = 1.0 / (point[0] - point[3]).max(1e-9);
    (point[1] * altitude, point[2] * altitude, altitude)
}

fn interpolate_camera_geodesic(
    from: (f64, f64, f64),
    to: (f64, f64, f64),
    progress: f64,
) -> (f64, f64, f64) {
    let t = progress.clamp(0.0, 1.0);
    if t == 0.0 {
        return from;
    }
    if t == 1.0 {
        return to;
    }
    let a = camera_to_hyperboloid(from.0, from.1, from.2);
    let b = camera_to_hyperboloid(to.0, to.1, to.2);
    let lorentz_dot = -a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let distance = (-lorentz_dot).max(1.0).acosh();
    if distance < 1e-7 {
        return (
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            from.2 * (to.2 / from.2).powf(t),
        );
    }
    let denominator = distance.sinh();
    let weight_a = ((1.0 - t) * distance).sinh() / denominator;
    let weight_b = (t * distance).sinh() / denominator;
    hyperboloid_to_camera([
        weight_a * a[0] + weight_b * b[0],
        weight_a * a[1] + weight_b * b[1],
        weight_a * a[2] + weight_b * b[2],
        weight_a * a[3] + weight_b * b[3],
    ])
}

// Blend two views along the perceptually shortest coupled pan/zoom path.
fn blend_zoom(a: &ZoomState, b: &ZoomState, t: f64) -> ZoomState {
    let za = a.zoom_factor.max(0.1);
    let zb = b.zoom_factor.max(0.1);
    let (cax, cay) = to_viewport_center(za, a.position_x, a.position_y);
    let (cbx, cby) = to_viewport_center(zb, b.position_x, b.position_y);
    let (cx, cy, altitude) =
        interpolate_camera_geodesic((cax, cay, 1.0 / za), (cbx, cby, 1.0 / zb), t);
    let zoom = 1.0 / altitude.max(1e-6);
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

// Port of calculateCurrentZoomStateInternal for export.
fn calculate_zoom_state(
    current_time: f64,
    segment: &VideoSegment,
    cropped_w: f64,
    cropped_h: f64,
    canvas: (f64, f64, f64),
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

        // Map crop-relative coordinates into the scaled, contain-fitted canvas.
        let full_w = cropped_w / crop_w_frac;
        let full_h = cropped_h / crop_h_frac;
        let crop_off_x = full_w * crop_x;
        let crop_off_y = full_h * crop_y;
        let (view_w, view_h, scale) = canvas;
        let fit = (view_w / cropped_w.max(1.0)).min(view_h / cropped_h.max(1.0));
        let width = cropped_w.max(1.0) * fit * scale;
        let height = cropped_h.max(1.0) * fit * scale;
        let pos_x = ((view_w - width) / 2.0 + (cam_x - crop_off_x) / cropped_w * width) / view_w;
        let pos_y = ((view_h - height) / 2.0 + (cam_y - crop_off_y) / cropped_h * height) / view_h;

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
        let mut enabled: Vec<&ZoomBlock> =
            segment.zoom_blocks.iter().filter(|b| b.enabled).collect();
        enabled.sort_by(|a, b| a.start_time.total_cmp(&b.start_time));
        let mut direct_state: Option<ZoomState> = None;
        for pair in enabled.windows(2) {
            let from = pair[0];
            let to = pair[1];
            if !from.direct_transition_to_next {
                continue;
            }
            let transition_start = from.end_time - from.ease_out.max(0.0);
            let transition_end = to.start_time + to.ease_in.max(0.0);
            if transition_end <= transition_start + 1e-6
                || current_time < transition_start
                || current_time > transition_end
            {
                continue;
            }
            let progress = ease_camera_move(
                (current_time - transition_start) / (transition_end - transition_start),
            );
            let block_state = |block: &ZoomBlock| {
                let (position_x, position_y) = if block.follow_cursor {
                    auto_state
                        .as_ref()
                        .map(|state| (state.position_x, state.position_y))
                        .unwrap_or((block.position_x, block.position_y))
                } else {
                    (block.position_x, block.position_y)
                };
                ZoomState {
                    zoom_factor: block.zoom_factor,
                    position_x,
                    position_y,
                }
            };
            direct_state = Some(blend_zoom(&block_state(from), &block_state(to), progress));
            break;
        }
        if let Some(direct) = direct_state {
            (Some(direct), 1.0)
        } else {
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

/// The export camera evaluated per sample with the preview's exact math.
/// Every render sample (speed-mapped frame times, motion-blur sub-samples)
/// reads the true camera state instead of a lerp between grid frames.
pub struct ExactCameraPath {
    segment: VideoSegment,
    cropped_w: f64,
    cropped_h: f64,
    crop_offset_x: f64,
    crop_offset_y: f64,
    canvas: (f64, f64, f64),
    start: f64,
    end: f64,
}

impl ExactCameraPath {
    /// None when the segment has no kept range to evaluate.
    pub fn new(
        segment: &VideoSegment,
        source_width: u32,
        source_height: u32,
        canvas: (f64, f64, f64),
    ) -> Option<Self> {
        let first = segment.trim_segments.first()?;
        let last = segment.trim_segments.last()?;
        let (crop_x, crop_y, crop_w_frac, crop_h_frac) = segment
            .crop
            .as_ref()
            .map(|c| (c.x, c.y, c.width, c.height))
            .unwrap_or((0.0, 0.0, 1.0, 1.0));
        Some(Self {
            segment: segment.clone(),
            cropped_w: source_width as f64 * crop_w_frac,
            cropped_h: source_height as f64 * crop_h_frac,
            crop_offset_x: source_width as f64 * crop_x,
            crop_offset_y: source_height as f64 * crop_y,
            canvas,
            start: first.start_time,
            end: last.end_time,
        })
    }

    /// Camera anchor in global source pixels and zoom at a source time. Times
    /// outside the kept range hold the edge state, as the baked path did.
    pub fn sample(&self, time: f64) -> (f64, f64, f64) {
        let t = time.clamp(self.start, self.end.max(self.start));
        let state = calculate_zoom_state(
            t,
            &self.segment,
            self.cropped_w,
            self.cropped_h,
            self.canvas,
        );
        (
            self.crop_offset_x + state.position_x * (self.cropped_w.max(1.0) as u32) as f64,
            self.crop_offset_y + state.position_y * (self.cropped_h.max(1.0) as u32) as f64,
            state.zoom_factor,
        )
    }
}

/// Where export reads its camera: a path supplied with the job, or the exact
/// per-sample evaluator when the worker derives the camera itself.
pub enum CameraSource {
    Baked(Vec<BakedCameraFrame>),
    Exact(Box<ExactCameraPath>),
}

impl CameraSource {
    pub fn sample(&self, time: f64) -> (f64, f64, f64) {
        match self {
            Self::Baked(frames) => super::sampling::sample_baked_path(time, frames),
            Self::Exact(path) => path.sample(time),
        }
    }
}

/// Generate baked camera path in Rust, one exact sample per output frame.
/// Output: {time, x(globalPx), y(globalPx), zoom}[] indexed in SOURCE time.
#[cfg(test)]
pub fn generate_camera_path(
    segment: &VideoSegment,
    source_width: u32,
    source_height: u32,
    fps: u32,
    canvas: (f64, f64, f64),
) -> Vec<BakedCameraFrame> {
    let Some(path) = ExactCameraPath::new(segment, source_width, source_height, canvas) else {
        return vec![];
    };
    let step = 1.0 / fps as f64;
    let mut frames = Vec::new();
    let mut t = path.start;
    loop {
        let (x, y, zoom) = path.sample(t);
        frames.push(BakedCameraFrame {
            time: t,
            x,
            y,
            zoom,
        });
        if t >= path.end - 1e-9 {
            break;
        }
        t = (t + step).min(path.end);
    }
    frames
}

#[cfg(test)]
#[path = "camera_path_tests.rs"]
mod tests;
