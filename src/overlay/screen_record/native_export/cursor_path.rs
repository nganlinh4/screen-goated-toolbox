// Rust port of videoRenderer.ts cursor path generation.
// Mirrors processCursorPositions + generateBakedCursorPath + getCursorVisibility.

use super::config::{
    BackgroundConfig, BakedCursorFrame, CursorVisibilitySegment, MousePosition, VideoSegment,
};

// --- Constants (mirror cursorHiding.ts) ---
const FADE_IN_DURATION: f64 = 0.2;
const FADE_OUT_DURATION: f64 = 0.25;
const SCALE_HIDDEN: f64 = 0.5;

// --- Cursor physics defaults (mirror videoRenderer.ts) ---
const DEFAULT_CURSOR_OFFSET_SEC: f64 = 0.0;
const DEFAULT_CURSOR_WIGGLE_STRENGTH: f64 = 0.30;
const DEFAULT_CURSOR_WIGGLE_DAMPING: f64 = 0.55;
const DEFAULT_CURSOR_WIGGLE_RESPONSE: f64 = 6.5;
const DEFAULT_CURSOR_TILT_DEG: f64 = -10.0;
const DEFAULT_CURSOR_SMOOTHNESS: f64 = 5.0;

// --- Squish state machine constants ---
const CLICK_FUSE_THRESHOLD: f64 = 0.05;
const SQUISH_TARGET: f64 = 0.75;
const QUICK_CLICK_WINDOW: f64 = 0.1;
const SQUISH_DOWN_DUR_BASE: f64 = 0.10;
const SQUISH_DOWN_DUR_MIN: f64 = 0.04;
const RELEASE_DUR_BASE: f64 = 0.15;
const RELEASE_DUR_MIN: f64 = 0.04;

// Internal processed position (after spring dynamics).
#[derive(Clone)]
struct Pos {
    x: f64,
    y: f64,
    timestamp: f64,
    is_clicked: bool,
    cursor_type: String,
    cursor_rotation: f64,
}

// ─── Background config helpers ────────────────────────────────────────────────

fn get_cursor_offset_sec(bg: Option<&BackgroundConfig>) -> f64 {
    bg.and_then(|b| b.cursor_movement_delay)
        .map(|v| v.clamp(-0.5, 0.5))
        .unwrap_or(DEFAULT_CURSOR_OFFSET_SEC)
}

fn get_cursor_smoothness(bg: Option<&BackgroundConfig>) -> f64 {
    bg.and_then(|b| b.cursor_smoothness)
        .map(|v| v.clamp(0.0, 10.0))
        .unwrap_or(DEFAULT_CURSOR_SMOOTHNESS)
}

fn get_wiggle_strength(bg: Option<&BackgroundConfig>) -> f64 {
    bg.and_then(|b| b.cursor_wiggle_strength)
        .map(|v| v.clamp(0.0, 1.0))
        .unwrap_or(DEFAULT_CURSOR_WIGGLE_STRENGTH)
}

fn get_wiggle_damping(bg: Option<&BackgroundConfig>) -> f64 {
    bg.and_then(|b| b.cursor_wiggle_damping)
        .map(|v| v.clamp(0.35, 0.98))
        .unwrap_or(DEFAULT_CURSOR_WIGGLE_DAMPING)
}

fn get_wiggle_response(bg: Option<&BackgroundConfig>) -> f64 {
    bg.and_then(|b| b.cursor_wiggle_response)
        .map(|v| v.clamp(2.0, 12.0))
        .unwrap_or(DEFAULT_CURSOR_WIGGLE_RESPONSE)
}

fn get_tilt_rad(bg: Option<&BackgroundConfig>) -> f64 {
    let deg = bg
        .and_then(|b| b.cursor_tilt_angle)
        .unwrap_or(DEFAULT_CURSOR_TILT_DEG);
    deg * (std::f64::consts::PI / 180.0)
}

fn get_cursor_pack(bg: Option<&BackgroundConfig>) -> &str {
    let Some(b) = bg else {
        return "screenstudio";
    };
    if let Some(ref p) = b.cursor_pack {
        return p.as_str();
    }
    // Fallback: infer from per-type variants
    for v in [
        b.cursor_default_variant.as_deref(),
        b.cursor_text_variant.as_deref(),
        b.cursor_pointer_variant.as_deref(),
        b.cursor_open_hand_variant.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        match v {
            "jepriwin11" | "sgtpixel" | "sgtai" | "sgtcool" | "sgtcute" | "macos26"
            | "sgtwatermelon" | "sgtfastfood" | "sgtveggie" | "sgtvietnam" | "sgtkorea"
            | "screenstudio" => return v,
            _ => {}
        }
    }
    "screenstudio"
}

// ─── Cursor type resolution ────────────────────────────────────────────────────

fn resolve_semantic(raw: &str) -> &'static str {
    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "text" | "ibeam" => "text",
        "pointer" | "hand" => "pointer",
        "wait" => "wait",
        "appstarting" => "appstarting",
        "crosshair" | "cross" => "crosshair",
        "resize_ns" | "sizens" => "resize_ns",
        "resize_we" | "sizewe" => "resize_we",
        "resize_nwse" | "sizenwse" => "resize_nwse",
        "resize_nesw" | "sizenesw" => "resize_nesw",
        "move" | "sizeall" | "drag" | "dragging" | "openhand" | "open-hand" | "open_hand"
        | "closedhand" | "closed-hand" | "closed_hand" | "closehand" | "close-hand"
        | "close_hand" | "grab" | "grabbing" => "grab",
        _ => "default",
    }
}

fn build_cursor_type(semantic: &str, pack: &str, is_clicked: bool) -> String {
    // "grab" semantic resolves to openhand/closehand based on click state
    let effective = if semantic == "grab" {
        if is_clicked { "closehand" } else { "openhand" }
    } else {
        semantic
    };

    let suffix = match pack {
        "macos26" | "sgtcool" | "sgtai" | "sgtpixel" | "jepriwin11" | "sgtcute"
        | "sgtwatermelon" | "sgtfastfood" | "sgtveggie" | "sgtvietnam" | "sgtkorea" => pack,
        _ => "screenstudio",
    };

    // Map semantic to type name used in cursor atlas
    let type_name = match effective {
        "text" => "text",
        "pointer" => "pointer",
        "openhand" => "openhand",
        "closehand" => "closehand",
        "wait" => "wait",
        "appstarting" => "appstarting",
        "crosshair" => "crosshair",
        "resize_ns" => "resize-ns",
        "resize_we" => "resize-we",
        "resize_nwse" => "resize-nwse",
        "resize_nesw" => "resize-nesw",
        _ => "default",
    };

    format!("{}-{}", type_name, suffix)
}

fn resolve_cursor_type(raw: &str, bg: Option<&BackgroundConfig>, is_clicked: bool) -> String {
    let pack = get_cursor_pack(bg);
    let semantic = resolve_semantic(raw);
    build_cursor_type(semantic, pack, is_clicked)
}

/// Whether this cursor type gets rotation applied.
fn should_cursor_rotate(cursor_type: &str) -> bool {
    let t = cursor_type.to_ascii_lowercase();
    t.starts_with("default-") || t.starts_with("pointer-") || t.starts_with("text-")
}

/// Whether this cursor type gets the static tilt offset.
fn should_cursor_tilt(cursor_type: &str) -> bool {
    let t = cursor_type.to_ascii_lowercase();
    t.starts_with("default") || t.starts_with("pointer")
}

// ─── Spring physics ────────────────────────────────────────────────────────────

struct SpringResult {
    value: f64,
    velocity: f64,
}

/// Analytical damped spring step (exact solution of damped harmonic oscillator ODE).
fn spring_step_scalar(
    current: f64,
    target: f64,
    velocity: f64,
    angular_freq: f64,
    damping_ratio: f64,
    dt: f64,
) -> SpringResult {
    let disp = current - target;
    if disp.abs() < 1e-8 && velocity.abs() < 1e-8 {
        return SpringResult {
            value: target,
            velocity: 0.0,
        };
    }

    let omega = angular_freq;
    let zeta = damping_ratio;

    let (new_disp, new_vel) = if zeta < 1.0 - 1e-6 {
        // Underdamped — oscillatory
        let alpha = omega * (1.0 - zeta * zeta).sqrt();
        let decay = (-omega * zeta * dt).exp();
        let cos_a = (alpha * dt).cos();
        let sin_a = (alpha * dt).sin();
        let nd = decay * (disp * cos_a + ((velocity + omega * zeta * disp) / alpha) * sin_a);
        let nv = decay
            * (velocity * cos_a
                - ((velocity * zeta * omega + omega * omega * disp) / alpha) * sin_a);
        (nd, nv)
    } else if zeta > 1.0 + 1e-6 {
        // Overdamped — exponential decay
        let disc = (zeta * zeta - 1.0).sqrt();
        let s1 = -omega * (zeta - disc);
        let s2 = -omega * (zeta + disc);
        let c2 = (velocity - s1 * disp) / (s2 - s1);
        let c1 = disp - c2;
        let e1 = (s1 * dt).exp();
        let e2 = (s2 * dt).exp();
        (c1 * e1 + c2 * e2, c1 * s1 * e1 + c2 * s2 * e2)
    } else {
        // Critically damped
        let decay = (-omega * dt).exp();
        let nd = (disp + (velocity + omega * disp) * dt) * decay;
        let nv = (velocity - (velocity + omega * disp) * omega * dt) * decay;
        (nd, nv)
    };

    SpringResult {
        value: target + new_disp,
        velocity: new_vel,
    }
}

fn normalize_angle(angle: f64) -> f64 {
    let mut a = angle;
    while a > std::f64::consts::PI {
        a -= std::f64::consts::TAU;
    }
    while a < -std::f64::consts::PI {
        a += std::f64::consts::TAU;
    }
    a
}

fn spring_step_angle(
    current: f64,
    target: f64,
    velocity: f64,
    angular_freq: f64,
    damping_ratio: f64,
    dt: f64,
) -> SpringResult {
    let adjusted_target = current + normalize_angle(target - current);
    spring_step_scalar(
        current,
        adjusted_target,
        velocity,
        angular_freq,
        damping_ratio,
        dt,
    )
}

/// Smooth-damp scalar (Spring-Damper, Unity-style). Used for heading smoothing.
fn smooth_damp_scalar(
    current: f64,
    target: f64,
    velocity: f64,
    smooth_time: f64,
    max_speed: f64,
    dt: f64,
) -> SpringResult {
    let safe_t = smooth_time.max(0.0001);
    let omega = 2.0 / safe_t;
    let x = omega * dt;
    let exp = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);

    let mut change = current - target;
    let original_target = target;
    let max_change = max_speed * safe_t;
    change = change.clamp(-max_change, max_change);
    let adj_target = current - change;

    let temp = (velocity + omega * change) * dt;
    let new_velocity = (velocity - omega * temp) * exp;
    let mut output = adj_target + (change + temp) * exp;

    if (original_target - current > 0.0) == (output > original_target) {
        output = original_target;
    }

    SpringResult {
        value: output,
        velocity: new_velocity,
    }
}

fn smooth_damp_angle(
    current: f64,
    target: f64,
    velocity: f64,
    smooth_time: f64,
    max_speed: f64,
    dt: f64,
) -> SpringResult {
    let adjusted_target = current + normalize_angle(target - current);
    smooth_damp_scalar(
        current,
        adjusted_target,
        velocity,
        smooth_time,
        max_speed,
        dt,
    )
}

fn lerp_angle(from: f64, to: f64, t: f64) -> f64 {
    normalize_angle(from + normalize_angle(to - from) * t)
}

// ─── Catmull-Rom interpolation ─────────────────────────────────────────────────

fn catmull_rom(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

// ─── cursor processing pipeline ───────────────────────────────────────────────

/// Step 1: Catmull-Rom interpolation to 120fps + Gaussian blur + dedup.
fn smooth_mouse_positions(positions: &[MousePosition], bg: Option<&BackgroundConfig>) -> Vec<Pos> {
    if positions.len() < 4 {
        return positions
            .iter()
            .map(|p| Pos {
                x: p.x,
                y: p.y,
                timestamp: p.timestamp,
                is_clicked: p.is_clicked,
                cursor_type: p
                    .cursor_type
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
                cursor_rotation: p.cursor_rotation.unwrap_or(0.0),
            })
            .collect();
    }

    let target_fps = 120.0_f64;
    let mut smoothed: Vec<Pos> = Vec::new();

    for i in 0..positions.len().saturating_sub(3) {
        let p0 = &positions[i];
        let p1 = &positions[i + 1];
        let p2 = &positions[i + 2];
        let p3 = &positions[i + 3];

        let seg_dur = p2.timestamp - p1.timestamp;
        let n_frames = (seg_dur * target_fps).ceil() as usize;
        let n_frames = n_frames.max(1);

        for frame in 0..n_frames {
            let t = frame as f64 / n_frames as f64;
            let timestamp = p1.timestamp + seg_dur * t;
            let x = catmull_rom(p0.x, p1.x, p2.x, p3.x, t);
            let y = catmull_rom(p0.y, p1.y, p2.y, p3.y, t);
            let is_clicked = p1.is_clicked || p2.is_clicked;
            let cursor_type = if t < 0.5 {
                p1.cursor_type
                    .clone()
                    .unwrap_or_else(|| "default".to_string())
            } else {
                p2.cursor_type
                    .clone()
                    .unwrap_or_else(|| "default".to_string())
            };
            smoothed.push(Pos {
                x,
                y,
                timestamp,
                is_clicked,
                cursor_type,
                cursor_rotation: 0.0,
            });
        }
    }

    if smoothed.is_empty() {
        return positions
            .iter()
            .map(|p| Pos {
                x: p.x,
                y: p.y,
                timestamp: p.timestamp,
                is_clicked: p.is_clicked,
                cursor_type: p
                    .cursor_type
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
                cursor_rotation: p.cursor_rotation.unwrap_or(0.0),
            })
            .collect();
    }

    // Gaussian blur passes
    let smoothness = get_cursor_smoothness(bg);
    let window_size = (smoothness * 2.0 + 1.0) as usize;
    let passes = ((window_size as f64) / 2.0).ceil() as usize;
    let mut current = smoothed;

    for _ in 0..passes {
        let mut pass: Vec<Pos> = Vec::with_capacity(current.len());
        let n = current.len();
        for i in 0..n {
            let j_start = i.saturating_sub(window_size);
            let j_end = (i + window_size).min(n - 1);
            let mut sum_x = 0.0_f64;
            let mut sum_y = 0.0_f64;
            let mut total_w = 0.0_f64;
            for (offset, point) in current[j_start..=j_end].iter().enumerate() {
                let j = j_start + offset;
                let dist = (i as isize - j as isize).unsigned_abs() as f64;
                let w = (-dist * (0.5 / window_size as f64)).exp();
                sum_x += point.x * w;
                sum_y += point.y * w;
                total_w += w;
            }
            pass.push(Pos {
                x: sum_x / total_w,
                y: sum_y / total_w,
                timestamp: current[i].timestamp,
                is_clicked: current[i].is_clicked,
                cursor_type: current[i].cursor_type.clone(),
                cursor_rotation: current[i].cursor_rotation,
            });
        }
        current = pass;
    }

    // Dedup by distance threshold
    let threshold = 0.5 / (window_size as f64 / 2.0);
    let mut last = current[0].clone();
    let mut final_smoothed = vec![last.clone()];

    for item in current.into_iter().skip(1) {
        let dx = item.x - last.x;
        let dy = item.y - last.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > threshold || item.is_clicked != last.is_clicked {
            final_smoothed.push(item.clone());
            last = item;
        } else {
            final_smoothed.push(Pos {
                timestamp: item.timestamp,
                ..last.clone()
            });
        }
    }

    final_smoothed
}

/// Step 2: Position spring dynamics — physical inertia / trailing lag.
fn apply_spring_position_dynamics(positions: Vec<Pos>, bg: Option<&BackgroundConfig>) -> Vec<Pos> {
    if positions.len() < 2 {
        return positions;
    }

    let strength = get_wiggle_strength(bg);
    if strength <= 0.001 {
        return positions;
    }

    let damping = get_wiggle_damping(bg);
    let response_hz = get_wiggle_response(bg);

    let base_omega = 2.0 * std::f64::consts::PI * response_hz;
    let pos_omega = base_omega * (4.0 - strength * 2.5);
    let pos_zeta = (damping + 0.18).min(0.92);
    let max_disp = 8.0 + strength * 28.0;

    let mut result = Vec::with_capacity(positions.len());
    let mut sx = positions[0].x;
    let mut sy = positions[0].y;
    let mut vx = 0.0_f64;
    let mut vy = 0.0_f64;

    result.push(positions[0].clone());

    for i in 1..positions.len() {
        let prev = &positions[i - 1];
        let target = &positions[i];
        let dt = (target.timestamp - prev.timestamp).max(0.001);

        let rx = spring_step_scalar(sx, target.x, vx, pos_omega, pos_zeta, dt);
        let ry = spring_step_scalar(sy, target.y, vy, pos_omega, pos_zeta, dt);
        sx = rx.value;
        sy = ry.value;
        vx = rx.velocity;
        vy = ry.velocity;

        // Clamp displacement
        let dx = sx - target.x;
        let dy = sy - target.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > max_disp {
            let ratio = max_disp / dist;
            sx = target.x + dx * ratio;
            sy = target.y + dy * ratio;
            vx *= ratio;
            vy *= ratio;
        }

        result.push(Pos {
            x: sx,
            y: sy,
            ..target.clone()
        });
    }

    result
}

/// Step 3: Adaptive rotation wiggle — heading-based tilt spring.
fn apply_adaptive_cursor_wiggle(positions: Vec<Pos>, bg: Option<&BackgroundConfig>) -> Vec<Pos> {
    if positions.len() < 2 {
        return positions;
    }

    let strength = get_wiggle_strength(bg);
    if strength <= 0.001 {
        return positions;
    }

    let damping = get_wiggle_damping(bg);
    let response_hz = get_wiggle_response(bg);

    let max_tilt_rad = (2.2 + strength * 8.8) * (std::f64::consts::PI / 180.0);
    let heading_smooth_time = 0.28 - strength * 0.17;
    let tilt_gain = 0.33 + strength * 0.92;
    let speed_start = 120.0_f64;
    let speed_full = 1650.0_f64;

    let rotation_omega = 2.0 * std::f64::consts::PI * response_hz;
    let rotation_zeta = damping;

    let mut result = Vec::with_capacity(positions.len());
    let mut lag_heading = 0.0_f64;
    let mut lag_heading_vel = 0.0_f64;
    let mut has_heading = false;
    let mut cursor_rotation = 0.0_f64;
    let mut cursor_rotation_vel = 0.0_f64;

    result.push(Pos {
        cursor_rotation: 0.0,
        ..positions[0].clone()
    });

    for i in 1..positions.len() {
        let prev = &positions[i - 1];
        let target = &positions[i];
        let dt_raw = (target.timestamp - prev.timestamp).max(0.001);

        let tvx = (target.x - prev.x) / dt_raw;
        let tvy = (target.y - prev.y) / dt_raw;
        let speed = tvx.hypot(tvy);

        let mut tilt_target = 0.0_f64;
        if speed > speed_start {
            let heading = tvy.atan2(tvx);
            if !has_heading {
                lag_heading = heading;
                has_heading = true;
            }
            let hs = smooth_damp_angle(
                lag_heading,
                heading,
                lag_heading_vel,
                heading_smooth_time,
                18.0,
                dt_raw,
            );
            lag_heading = hs.value;
            lag_heading_vel = hs.velocity;

            let t_fade = ((speed - speed_start) / (speed_full - speed_start)).clamp(0.0, 1.0);
            let speed_fade = t_fade * t_fade * (3.0 - 2.0 * t_fade); // SmoothStep
            let raw_tilt = normalize_angle(heading - lag_heading) * tilt_gain * speed_fade;
            tilt_target = raw_tilt.clamp(-max_tilt_rad, max_tilt_rad);
        }

        let rs = spring_step_angle(
            cursor_rotation,
            tilt_target,
            cursor_rotation_vel,
            rotation_omega,
            rotation_zeta,
            dt_raw,
        );
        cursor_rotation = rs.value;
        cursor_rotation_vel = rs.velocity;

        result.push(Pos {
            cursor_rotation,
            ..target.clone()
        });
    }

    result
}

/// Step 4: Static tilt offset.
fn apply_cursor_tilt_offset(positions: Vec<Pos>, bg: Option<&BackgroundConfig>) -> Vec<Pos> {
    let tilt_rad = get_tilt_rad(bg);
    if tilt_rad.abs() < 0.0001 {
        return positions;
    }
    positions
        .into_iter()
        .map(|mut p| {
            if should_cursor_tilt(&p.cursor_type) {
                p.cursor_rotation += tilt_rad;
            }
            p
        })
        .collect()
}

/// Full cursor processing: smooth → spring pos → rotation wiggle → tilt.
fn process_cursor_positions(raw: &[MousePosition], bg: Option<&BackgroundConfig>) -> Vec<Pos> {
    let smoothed = smooth_mouse_positions(raw, bg);
    let springed = apply_spring_position_dynamics(smoothed, bg);
    let wiggled = apply_adaptive_cursor_wiggle(springed, bg);
    apply_cursor_tilt_offset(wiggled, bg)
}

/// Interpolate processed positions at a given timestamp.
fn interpolate_pos(time: f64, positions: &[Pos]) -> Option<Pos> {
    if positions.is_empty() {
        return None;
    }

    // Exact match (within 1ms)
    if let Some(p) = positions
        .iter()
        .find(|p| (p.timestamp - time).abs() < 0.001)
    {
        return Some(p.clone());
    }

    let next_idx = positions.partition_point(|p| p.timestamp <= time);

    if next_idx == 0 {
        return Some(positions[0].clone());
    }
    if next_idx >= positions.len() {
        return Some(positions.last().unwrap().clone());
    }

    let prev = &positions[next_idx - 1];
    let next = &positions[next_idx];
    let span = next.timestamp - prev.timestamp;
    let t = if span > 1e-10 {
        (time - prev.timestamp) / span
    } else {
        0.0
    };

    Some(Pos {
        x: prev.x + (next.x - prev.x) * t,
        y: prev.y + (next.y - prev.y) * t,
        timestamp: time,
        is_clicked: prev.is_clicked || next.is_clicked,
        cursor_type: next.cursor_type.clone(),
        cursor_rotation: lerp_angle(prev.cursor_rotation, next.cursor_rotation, t),
    })
}

// ─── Cursor visibility (mirrors getCursorVisibility in cursorHiding.ts) ────────

fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_cubic(t: f64) -> f64 {
    t * t * t
}

fn segment_fade_durations(start_time: f64, end_time: f64) -> (f64, f64) {
    let duration = (end_time - start_time).max(0.0);
    let min_fully_visible_duration = 0.06_f64;
    let preferred_total = FADE_IN_DURATION + FADE_OUT_DURATION;
    let max_fade_total = (duration - min_fully_visible_duration).max(0.0);

    if duration <= 0.0 || max_fade_total <= 0.0 || preferred_total <= 0.0 {
        return (0.0, 0.0);
    }

    let actual_total = preferred_total.min(max_fade_total);
    let fade_in = actual_total * (FADE_IN_DURATION / preferred_total);
    let fade_out = actual_total - fade_in;
    (fade_in, fade_out)
}

fn get_cursor_visibility(time: f64, segments: &Option<Vec<CursorVisibilitySegment>>) -> (f64, f64) {
    // (opacity, scale)
    let Some(segs) = segments else {
        // Feature off — always visible
        return (1.0, 1.0);
    };

    if segs.is_empty() {
        return (0.0, SCALE_HIDDEN);
    }

    for seg in segs {
        if time < seg.start_time || time > seg.end_time {
            continue;
        }

        let (fade_in, fade_out) = segment_fade_durations(seg.start_time, seg.end_time);
        let fade_in_end = seg.start_time + fade_in;
        let fade_out_start = seg.end_time - fade_out;

        if fade_in > 0.0 && time < fade_in_end {
            let t = ((time - seg.start_time) / fade_in).clamp(0.0, 1.0);
            let eased = ease_out_cubic(t);
            return (eased, SCALE_HIDDEN + (1.0 - SCALE_HIDDEN) * eased);
        }

        if fade_out > 0.0 && time > fade_out_start {
            let t = ((time - fade_out_start) / fade_out).clamp(0.0, 1.0);
            let eased = 1.0 - ease_in_cubic(t);
            return (eased, SCALE_HIDDEN + (1.0 - SCALE_HIDDEN) * eased);
        }

        return (1.0, 1.0);
    }

    (0.0, SCALE_HIDDEN)
}

fn get_keystroke_delay_sec(segment: &VideoSegment) -> f64 {
    segment.keystroke_delay_sec.clamp(-1.0, 1.0)
}

fn has_mousedown_events(segment: &VideoSegment) -> bool {
    segment
        .keystroke_events
        .iter()
        .any(|e| e.event_type.eq_ignore_ascii_case("mousedown"))
}

#[derive(Clone, Copy)]
struct MouseDownEventView {
    idx: usize,
    start_time: f64,
    end_time: f64,
    is_hold: bool,
}

fn to_mousedown_event(idx: usize, e: &super::config::KeystrokeEvent) -> MouseDownEventView {
    MouseDownEventView {
        idx,
        start_time: e.start_time,
        end_time: e.end_time,
        is_hold: e.is_hold,
    }
}

fn find_active_mousedown_event(
    segment: &VideoSegment,
    lookup_time: f64,
) -> Option<MouseDownEventView> {
    segment
        .keystroke_events
        .iter()
        .enumerate()
        .find_map(|(idx, e)| {
            if !e.event_type.eq_ignore_ascii_case("mousedown") {
                return None;
            }
            let ev = to_mousedown_event(idx, e);
            let active_end = if ev.is_hold {
                ev.end_time
            } else {
                ev.start_time + QUICK_CLICK_WINDOW
            };
            if lookup_time >= ev.start_time && lookup_time <= active_end {
                Some(ev)
            } else {
                None
            }
        })
}

fn find_prev_mousedown_event(
    segment: &VideoSegment,
    before_time: f64,
) -> Option<MouseDownEventView> {
    segment
        .keystroke_events
        .iter()
        .enumerate()
        .rev()
        .find_map(|(idx, e)| {
            if !e.event_type.eq_ignore_ascii_case("mousedown") || e.start_time >= before_time {
                return None;
            }
            Some(to_mousedown_event(idx, e))
        })
}

fn find_next_mousedown_event(
    segment: &VideoSegment,
    after_time: f64,
) -> Option<MouseDownEventView> {
    segment
        .keystroke_events
        .iter()
        .enumerate()
        .find_map(|(idx, e)| {
            if !e.event_type.eq_ignore_ascii_case("mousedown") || e.start_time <= after_time {
                return None;
            }
            Some(to_mousedown_event(idx, e))
        })
}

fn effective_event_end(event: MouseDownEventView) -> f64 {
    if event.is_hold {
        event.end_time
    } else {
        event.start_time + QUICK_CLICK_WINDOW
    }
}

fn squish_ease_down(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

fn squish_ease_up(t: f64, has_room: bool) -> f64 {
    if !has_room {
        return 1.0 - (1.0 - t).powi(3);
    }
    let c = 1.2_f64; // Preview overshoot profile.
    let tm1 = t - 1.0;
    1.0 + (c + 1.0) * tm1.powi(3) + c * tm1.powi(2)
}

fn normalize_mouse_positions_to_source_frame(
    mouse_positions: &[MousePosition],
    source_width: f64,
    source_height: f64,
) -> Vec<MousePosition> {
    mouse_positions
        .iter()
        .map(|position| {
            let capture_width = position
                .capture_width
                .filter(|value| value.is_finite() && *value > 1.0)
                .unwrap_or(source_width.max(1.0));
            let capture_height = position
                .capture_height
                .filter(|value| value.is_finite() && *value > 1.0)
                .unwrap_or(source_height.max(1.0));

            let mut normalized = position.clone();
            normalized.x = (position.x / capture_width) * source_width.max(1.0);
            normalized.y = (position.y / capture_height) * source_height.max(1.0);
            normalized
        })
        .collect()
}

// ─── Public API ────────────────────────────────────────────────────────────────

/// Generate baked cursor path in Rust.
/// Mirrors TypeScript generateBakedCursorPath(segment, mousePositions, backgroundConfig, fps).
pub fn generate_cursor_path(
    segment: &VideoSegment,
    mouse_positions: &[MousePosition],
    source_width: f64,
    source_height: f64,
    bg: Option<&BackgroundConfig>,
    fps: u32,
) -> Vec<BakedCursorFrame> {
    if !segment.use_custom_cursor {
        return vec![];
    }

    if segment.trim_segments.is_empty() {
        eprintln!("[CursorPath] No trim segments — skipping cursor path generation");
        return vec![];
    }

    let full_start = segment.trim_segments[0].start_time;
    let full_end = segment.trim_segments.last().unwrap().end_time;

    let normalized_mouse_positions =
        normalize_mouse_positions_to_source_frame(mouse_positions, source_width, source_height);
    let processed = process_cursor_positions(&normalized_mouse_positions, bg);
    let cursor_offset_sec = get_cursor_offset_sec(bg);
    let pack = get_cursor_pack(bg).to_string();

    let step = 1.0 / fps as f64;
    let n_frames = ((full_end - full_start) / step).ceil() as usize + 2;
    let mut baked = Vec::with_capacity(n_frames);

    let mut sim_squish_scale = 1.0_f64;
    let mut sim_squish_target = 1.0_f64;
    let mut sim_squish_anim_from = 1.0_f64;
    let mut sim_squish_anim_progress = 1.0_f64;
    let mut sim_squish_anim_duration = RELEASE_DUR_BASE;
    let mut sim_squish_has_room = false;
    let mut sim_last_hold_time = -1.0_f64;
    let mut sim_last_active_event_idx: Option<usize> = None;
    let mut sim_prev_fallback_clicked = false;
    let keystroke_delay_sec = get_keystroke_delay_sec(segment);
    let use_keystroke_clicks = has_mousedown_events(segment);

    let mut t = full_start;
    loop {
        let cursor_t = t + cursor_offset_sec;
        let pos = interpolate_pos(cursor_t, &processed);

        if let Some(ref pos) = pos {
            let lookup_t = t - keystroke_delay_sec;
            let active_event = if use_keystroke_clicks {
                find_active_mousedown_event(segment, lookup_t)
            } else {
                None
            };
            let is_clicked = if use_keystroke_clicks {
                active_event.is_some()
            } else {
                pos.is_clicked
            };
            let prev_last_hold_time = sim_last_hold_time;

            if is_clicked {
                sim_last_hold_time = t;
            }
            let time_since_last_hold = t - sim_last_hold_time;
            let should_squish = is_clicked
                || (sim_last_hold_time >= 0.0 && time_since_last_hold < CLICK_FUSE_THRESHOLD);

            let target_scale = if should_squish { SQUISH_TARGET } else { 1.0 };
            let active_event_idx = active_event.map(|e| e.idx);
            let is_new_click = if use_keystroke_clicks {
                matches!(active_event_idx, Some(idx) if sim_last_active_event_idx != Some(idx))
            } else {
                is_clicked && !sim_prev_fallback_clicked
            };
            if use_keystroke_clicks {
                sim_last_active_event_idx = active_event_idx;
            }

            if (target_scale - sim_squish_target).abs() > 1e-9 || is_new_click {
                if is_new_click && sim_squish_scale < 0.95 && prev_last_hold_time >= 0.0 {
                    // Rapid re-click while still squished: reset pulse origin like preview drawFrame.ts.
                    sim_squish_scale = 1.0;
                }
                sim_squish_anim_from = sim_squish_scale;
                sim_squish_target = target_scale;
                sim_squish_anim_progress = 0.0;

                if sim_squish_target < sim_squish_anim_from {
                    // Squish down: adapt speed to previous click proximity.
                    let gap_from_prev = if use_keystroke_clicks {
                        if let Some(active) = active_event {
                            let prev_event =
                                find_prev_mousedown_event(segment, active.start_time - 0.01);
                            let prev_effective_end = prev_event
                                .map(effective_event_end)
                                .unwrap_or(f64::NEG_INFINITY);
                            (active.start_time - prev_effective_end).max(0.0)
                        } else {
                            f64::INFINITY
                        }
                    } else {
                        f64::INFINITY
                    };
                    sim_squish_anim_duration = if gap_from_prev.is_finite()
                        && gap_from_prev < (SQUISH_DOWN_DUR_BASE * 2.0)
                    {
                        (gap_from_prev * 0.4).max(SQUISH_DOWN_DUR_MIN)
                    } else {
                        SQUISH_DOWN_DUR_BASE
                    };
                    sim_squish_has_room = false;
                } else {
                    // Release: animate only when there's real recent click context.
                    let recent_click = sim_last_hold_time >= 0.0
                        && t >= sim_last_hold_time
                        && (t - sim_last_hold_time) < (CLICK_FUSE_THRESHOLD + QUICK_CLICK_WINDOW);
                    if !recent_click {
                        sim_squish_anim_progress = 1.0;
                    } else {
                        let gap_to_next = if use_keystroke_clicks {
                            let active_effective_end =
                                active_event.map(effective_event_end).unwrap_or(lookup_t);
                            let next_lookup_start =
                                active_event.map(|e| e.start_time).unwrap_or(lookup_t) + 0.01;
                            let next_event = find_next_mousedown_event(segment, next_lookup_start);
                            next_event
                                .map(|e| (e.start_time - active_effective_end).max(0.0))
                                .unwrap_or(f64::INFINITY)
                        } else {
                            f64::INFINITY
                        };
                        sim_squish_has_room = gap_to_next > (RELEASE_DUR_BASE * 2.0);
                        sim_squish_anim_duration =
                            if gap_to_next.is_finite() && gap_to_next < (RELEASE_DUR_BASE * 2.0) {
                                (gap_to_next * 0.5).max(RELEASE_DUR_MIN)
                            } else {
                                RELEASE_DUR_BASE
                            };
                    }
                }
            }

            if sim_squish_anim_progress < 1.0 {
                let elapsed_sec = step.max(0.0001);
                let duration = sim_squish_anim_duration.max(0.0001);
                sim_squish_anim_progress =
                    (sim_squish_anim_progress + elapsed_sec / duration).min(1.0);
                let anim_t = sim_squish_anim_progress;
                let going_down = sim_squish_target < sim_squish_anim_from;
                let eased = if going_down {
                    squish_ease_down(anim_t)
                } else {
                    squish_ease_up(anim_t, sim_squish_has_room)
                };
                sim_squish_scale =
                    sim_squish_anim_from + (sim_squish_target - sim_squish_anim_from) * eased;
            } else {
                sim_squish_scale = sim_squish_target;
            }

            let (vis_opacity, vis_scale) =
                get_cursor_visibility(t, &segment.cursor_visibility_segments);
            let resolved_type = resolve_cursor_type(&pos.cursor_type, bg, is_clicked);
            let rotation = if should_cursor_rotate(&resolved_type) {
                (pos.cursor_rotation * 10000.0).round() / 10000.0
            } else {
                0.0
            };

            baked.push(BakedCursorFrame {
                time: t,
                x: pos.x,
                y: pos.y,
                scale: (sim_squish_scale * vis_scale * 1000.0).round() / 1000.0,
                is_clicked,
                cursor_type: resolved_type,
                opacity: (vis_opacity * 1000.0).round() / 1000.0,
                rotation,
            });
            if !use_keystroke_clicks {
                sim_prev_fallback_clicked = is_clicked;
            }
        } else {
            // No data: hold last or emit default
            if let Some(last) = baked.last() {
                let last = last.clone();
                baked.push(BakedCursorFrame { time: t, ..last });
            } else {
                baked.push(BakedCursorFrame {
                    time: t,
                    x: 0.0,
                    y: 0.0,
                    scale: 1.0,
                    is_clicked: false,
                    cursor_type: format!("default-{}", pack),
                    opacity: 1.0,
                    rotation: 0.0,
                });
            }
        }

        if t >= full_end - 1e-9 {
            break;
        }
        t = (t + step).min(full_end);
    }

    eprintln!(
        "[CursorPath] Generated {} frames [{:.3}s..{:.3}s] at {}fps",
        baked.len(),
        full_start,
        full_end,
        fps
    );
    baked
}
