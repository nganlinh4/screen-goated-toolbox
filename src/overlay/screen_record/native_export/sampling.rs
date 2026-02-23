use super::config::{BakedCameraFrame, ParsedBakedCursorFrame};

pub fn sample_baked_path(time: f64, baked_path: &[BakedCameraFrame]) -> (f64, f64, f64) {
    if baked_path.is_empty() {
        return (0.0, 0.0, 1.0);
    }

    let idx = baked_path.partition_point(|p| p.time < time);

    if idx == 0 {
        let p = &baked_path[0];
        return (p.x, p.y, p.zoom);
    }

    if idx >= baked_path.len() {
        let p = baked_path.last().unwrap();
        return (p.x, p.y, p.zoom);
    }

    let p1 = &baked_path[idx - 1];
    let p2 = &baked_path[idx];

    let t = (time - p1.time) / (p2.time - p1.time).max(0.0001);
    let t = t.clamp(0.0, 1.0);

    let x = p1.x + (p2.x - p1.x) * t;
    let y = p1.y + (p2.y - p1.y) * t;
    let zoom = p1.zoom + (p2.zoom - p1.zoom) * t;

    (x, y, zoom)
}

pub fn sample_parsed_baked_cursor(
    time: f64,
    baked_path: &[ParsedBakedCursorFrame],
) -> Option<(f64, f64, f64, f32, f64, f64)> {
    if baked_path.is_empty() {
        return None;
    }

    let idx = baked_path.partition_point(|p| p.time < time);

    if idx == 0 {
        let p = &baked_path[0];
        return Some((p.x, p.y, p.scale, p.type_id, p.opacity, p.rotation));
    }

    if idx >= baked_path.len() {
        let p = baked_path.last().unwrap();
        return Some((p.x, p.y, p.scale, p.type_id, p.opacity, p.rotation));
    }

    let p1 = &baked_path[idx - 1];
    let p2 = &baked_path[idx];

    let t = (time - p1.time) / (p2.time - p1.time).max(0.0001);
    let t = t.clamp(0.0, 1.0);

    let x = p1.x + (p2.x - p1.x) * t;
    let y = p1.y + (p2.y - p1.y) * t;
    let scale = p1.scale + (p2.scale - p1.scale) * t;
    let opacity = p1.opacity + (p2.opacity - p1.opacity) * t;
    let rotation = lerp_angle_rad(p1.rotation, p2.rotation, t);
    let type_id = if t < 0.5 { p1.type_id } else { p2.type_id };

    Some((x, y, scale, type_id, opacity, rotation))
}

fn normalize_angle_rad(a: f64) -> f64 {
    let mut angle = a;
    while angle > std::f64::consts::PI {
        angle -= std::f64::consts::PI * 2.0;
    }
    while angle < -std::f64::consts::PI {
        angle += std::f64::consts::PI * 2.0;
    }
    angle
}

fn lerp_angle_rad(from: f64, to: f64, t: f64) -> f64 {
    let delta = normalize_angle_rad(to - from);
    normalize_angle_rad(from + delta * t)
}
