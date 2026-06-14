// Cursor processing pipeline: Catmull-Rom interpolation, spring dynamics,
// adaptive rotation wiggle, tilt offset, and position interpolation.

use super::spring::{
    lerp_angle, normalize_angle, smooth_damp_angle, spring_step_angle, spring_step_scalar,
};
use super::{
    Pos, get_cursor_smoothness, get_tilt_rad, get_wiggle_damping, get_wiggle_response,
    get_wiggle_strength, should_cursor_tilt,
};
use crate::overlay::screen_record::native_export::config::{BackgroundConfig, MousePosition};

// ─── Catmull-Rom interpolation ─────────────────────────────────────────────────

fn catmull_rom(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

// ─── Cursor processing pipeline ───────────────────────────────────────────────

/// Step 1: Catmull-Rom interpolation to 120fps + Gaussian blur + dedup.
pub(super) fn smooth_mouse_positions(
    positions: &[MousePosition],
    bg: Option<&BackgroundConfig>,
) -> Vec<Pos> {
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
        // WYSIWYG alignment to cursorDynamics.ts smoothMousePositions: clamp the
        // interpolated frame count to min(ceil(segDur*fps), 60). The preview caps
        // this at 60 frames per segment; export previously omitted the cap.
        let n_frames = ((seg_dur * target_fps).ceil() as usize).min(60);
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

    // WYSIWYG alignment to cursorDynamics.ts smoothMousePositions box-blur block:
    // a FIXED 3-pass UNIFORM box blur (each sample weighted 1) over the symmetric
    // window [max(0,i-half), min(n-1,i+half)], half = floor(windowSize/2). The mean
    // is the running window sum divided by the window length. The previous export
    // used an exponential-weight kernel, passes = ceil(windowSize/2), and a ±window
    // (≈2× too wide) half — diverging from the preview by 20-34px. Only x/y are
    // blurred; other fields are unchanged. The O(N) running-sum order mirrors the TS
    // accumulation exactly so both languages agree within 1e-6.
    let smoothness = get_cursor_smoothness(bg);
    let window_size = (smoothness * 2.0 + 1.0) as usize;
    let passes = 3; // 3-pass box blur approximates a Gaussian kernel in O(N) total
    let half = window_size / 2; // floor(windowSize / 2)
    let mut current = smoothed;

    for _ in 0..passes {
        let n = current.len();
        let mut new_x = vec![0.0_f64; n];
        let mut new_y = vec![0.0_f64; n];

        let mut run_x = 0.0_f64;
        let mut run_y = 0.0_f64;
        let mut win_start = 0usize;
        let mut win_end = half.min(n - 1);

        // Initialize the running sum for the first window centered at i=0.
        for point in &current[..=win_end] {
            run_x += point.x;
            run_y += point.y;
        }

        for i in 0..n {
            let target_start = i.saturating_sub(half);
            let target_end = (i + half).min(n - 1);

            while win_end < target_end {
                win_end += 1;
                run_x += current[win_end].x;
                run_y += current[win_end].y;
            }
            while win_start < target_start {
                run_x -= current[win_start].x;
                run_y -= current[win_start].y;
                win_start += 1;
            }

            let win_len = (win_end - win_start + 1) as f64;
            new_x[i] = run_x / win_len;
            new_y[i] = run_y / win_len;
        }

        for (i, pos) in current.iter_mut().enumerate() {
            pos.x = new_x[i];
            pos.y = new_y[i];
        }
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
pub(super) fn apply_spring_position_dynamics(
    positions: Vec<Pos>,
    bg: Option<&BackgroundConfig>,
) -> Vec<Pos> {
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
pub(super) fn apply_adaptive_cursor_wiggle(
    positions: Vec<Pos>,
    bg: Option<&BackgroundConfig>,
) -> Vec<Pos> {
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
pub(super) fn apply_cursor_tilt_offset(
    positions: Vec<Pos>,
    bg: Option<&BackgroundConfig>,
) -> Vec<Pos> {
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
pub(super) fn process_cursor_positions(
    raw: &[MousePosition],
    bg: Option<&BackgroundConfig>,
) -> Vec<Pos> {
    let smoothed = smooth_mouse_positions(raw, bg);
    let springed = apply_spring_position_dynamics(smoothed, bg);
    let wiggled = apply_adaptive_cursor_wiggle(springed, bg);
    apply_cursor_tilt_offset(wiggled, bg)
}

/// Interpolate processed positions at a given timestamp using binary search.
pub(super) fn interpolate_pos(time: f64, positions: &[Pos]) -> Option<Pos> {
    if positions.is_empty() {
        return None;
    }

    let next_idx = positions.partition_point(|p| p.timestamp <= time);

    if next_idx == 0 {
        return Some(positions[0].clone());
    }
    if next_idx >= positions.len() {
        return Some(positions.last().unwrap().clone());
    }

    let prev = &positions[next_idx - 1];

    // Exact match (within 1ms) — check the two nearest neighbors only
    if (prev.timestamp - time).abs() < 0.001 {
        return Some(prev.clone());
    }
    let next = &positions[next_idx];
    if (next.timestamp - time).abs() < 0.001 {
        return Some(next.clone());
    }

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

#[cfg(test)]
mod tests {
    use super::{catmull_rom, smooth_mouse_positions};
    use crate::overlay::screen_record::native_export::config::{BackgroundConfig, MousePosition};

    // Cross-language render-math golden for the Catmull-Rom primitive and the full
    // smoothMousePositions pipeline. TS preview (cursorDynamics.ts) is canonical.
    // Regenerate via screen-record/tests/unit/_generateRenderGolden.gen.ts.
    // See .claude/parity/render-camera-cursor.md.
    const GOLDEN: &str =
        include_str!("../../../../../parity-fixtures/render-camera-cursor/golden.json");

    #[test]
    fn catmull_rom_endpoints() {
        // Passes through p1 at t=0 and p2 at t=1.
        assert!((catmull_rom(0.0, 10.0, 30.0, 40.0, 0.0) - 10.0).abs() < 1e-12);
        assert!((catmull_rom(0.0, 10.0, 30.0, 40.0, 1.0) - 30.0).abs() < 1e-12);
    }

    #[test]
    fn catmull_rom_matches_golden() {
        let g: serde_json::Value = serde_json::from_str(GOLDEN).expect("golden parses");
        let tol = g["tolerance"].as_f64().unwrap();
        for c in g["cursorPrimitives"]["catmullRom"].as_array().unwrap() {
            let got = catmull_rom(
                c["p0"].as_f64().unwrap(),
                c["p1"].as_f64().unwrap(),
                c["p2"].as_f64().unwrap(),
                c["p3"].as_f64().unwrap(),
                c["t"].as_f64().unwrap(),
            );
            assert!(
                (got - c["value"].as_f64().unwrap()).abs() <= tol,
                "catmull drift at t={}",
                c["t"]
            );
        }
    }

    // Minimal BackgroundConfig JSON carrying the one field smooth_mouse_positions
    // reads (cursorSmoothness), plus the required base fields. Mirrors the TS
    // generator's smoothnessBg().
    fn bg_with_smoothness(smoothness: f64) -> BackgroundConfig {
        serde_json::from_value(serde_json::json!({
            "scale": 1.0,
            "borderRadius": 0.0,
            "backgroundType": "solid",
            "shadow": 0.0,
            "cursorScale": 1.0,
            "cursorSmoothness": smoothness,
        }))
        .expect("background config parses")
    }

    #[test]
    fn smooth_mouse_positions_matches_golden() {
        // Full pipeline (Catmull-Rom interp -> 3-pass uniform box blur -> dedup) is
        // now WYSIWYG-aligned with the TS preview and locked cross-language. The
        // Rust export must reproduce the canonical smoothMousePositions output for
        // each smoothness within 1e-6.
        let g: serde_json::Value = serde_json::from_str(GOLDEN).expect("golden parses");
        let tol = g["tolerance"].as_f64().unwrap();
        let sm = &g["cursorPrimitives"]["smoothMousePositions"];

        let input: Vec<MousePosition> =
            serde_json::from_value(sm["input"].clone()).expect("input positions parse");

        for case in sm["cases"].as_array().unwrap() {
            let smoothness = case["smoothness"].as_f64().unwrap();
            let bg = bg_with_smoothness(smoothness);
            let got = smooth_mouse_positions(&input, Some(&bg));
            let expected = case["output"].as_array().unwrap();

            assert_eq!(
                got.len(),
                expected.len(),
                "output length mismatch at smoothness={smoothness}"
            );
            for (i, (g_pos, e)) in got.iter().zip(expected.iter()).enumerate() {
                assert!(
                    (g_pos.x - e["x"].as_f64().unwrap()).abs() <= tol,
                    "x drift at smoothness={smoothness} idx={i}"
                );
                assert!(
                    (g_pos.y - e["y"].as_f64().unwrap()).abs() <= tol,
                    "y drift at smoothness={smoothness} idx={i}"
                );
                assert!(
                    (g_pos.timestamp - e["timestamp"].as_f64().unwrap()).abs() <= tol,
                    "timestamp drift at smoothness={smoothness} idx={i}"
                );
                assert_eq!(
                    g_pos.is_clicked,
                    e["isClicked"].as_bool().unwrap(),
                    "isClicked mismatch at smoothness={smoothness} idx={i}"
                );
                assert_eq!(
                    g_pos.cursor_type,
                    e["cursor_type"].as_str().unwrap(),
                    "cursor_type mismatch at smoothness={smoothness} idx={i}"
                );
            }
        }
    }
}
