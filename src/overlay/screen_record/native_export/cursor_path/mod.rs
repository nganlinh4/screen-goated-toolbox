// Rust port of videoRenderer.ts cursor path generation.
// Mirrors processCursorPositions + generateBakedCursorPath + getCursorVisibility.

mod processing;
mod spring;
mod visibility;

use super::config::{BackgroundConfig, BakedCursorFrame, MousePosition, VideoSegment};
use processing::{interpolate_pos, process_cursor_positions};
use visibility::{
    build_mousedown_events, effective_event_end, find_active_mousedown_event_fast,
    find_next_mousedown_event_fast, find_prev_mousedown_event_fast, get_cursor_visibility,
    get_keystroke_delay_sec, has_mousedown_events, normalize_mouse_positions_to_source_frame,
    squish_ease_down, squish_ease_up, CLICK_FUSE_THRESHOLD, QUICK_CLICK_WINDOW,
    RELEASE_DUR_BASE, RELEASE_DUR_MIN, SQUISH_DOWN_DUR_BASE, SQUISH_DOWN_DUR_MIN,
    SQUISH_TARGET,
};

// --- Cursor physics defaults (mirror videoRenderer.ts) ---
const DEFAULT_CURSOR_OFFSET_SEC: f64 = 0.0;
const DEFAULT_CURSOR_WIGGLE_STRENGTH: f64 = 0.30;
const DEFAULT_CURSOR_WIGGLE_DAMPING: f64 = 0.55;
const DEFAULT_CURSOR_WIGGLE_RESPONSE: f64 = 6.5;
const DEFAULT_CURSOR_TILT_DEG: f64 = -10.0;
const DEFAULT_CURSOR_SMOOTHNESS: f64 = 5.0;

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
    let mousedown_events = if use_keystroke_clicks {
        build_mousedown_events(segment)
    } else {
        vec![]
    };

    let mut t = full_start;
    loop {
        let cursor_t = t + cursor_offset_sec;
        let pos = interpolate_pos(cursor_t, &processed);

        if let Some(ref pos) = pos {
            let lookup_t = t - keystroke_delay_sec;
            let active_event = if use_keystroke_clicks {
                find_active_mousedown_event_fast(&mousedown_events, lookup_t)
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
                            let prev_event = find_prev_mousedown_event_fast(
                                &mousedown_events,
                                active.start_time - 0.01,
                            );
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
                            let next_event = find_next_mousedown_event_fast(
                                &mousedown_events,
                                next_lookup_start,
                            );
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
