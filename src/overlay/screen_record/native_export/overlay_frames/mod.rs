// Rust-side overlay frame quad generation.
//
// Replaces the JS per-frame loop in overlayBaker.ts. Instead of JS generating
// 40K+ OverlayFrame objects, serializing them to JSON, and sending over IPC,
// Rust generates the quads directly from compact atlas metadata + segment data.
//
// Input: OverlayAtlasMetadata (few KB) sent once from JS after atlas baking.
// Output: Vec<OverlayFrame> indexed by frame_idx.

mod layout;
mod types;

use layout::{
    LaneItem, Placement, compute_slot_width_hints, get_lane_gap_px, layout_keystroke_lane,
};
use types::{
    ActiveEvent, DEFAULT_KEYSTROKE_OVERLAY_SCALE, DEFAULT_KEYSTROKE_OVERLAY_X,
    DEFAULT_KEYSTROKE_OVERLAY_Y, KEYSTROKE_OVERLAY_MAX_SCALE, KEYSTROKE_OVERLAY_MIN_SCALE,
    TEXT_FADE_DUR, clamp01, find_active_events, get_keystroke_visual_state, get_speed,
    is_time_inside_segments,
};
pub use types::{KeystrokeAtlasEntry, OverlayAtlasMetadata};

use super::config::{OverlayFrame, OverlayQuad, SpeedPoint, TrimSegment};

// --- Main entry point ---

/// Generate overlay frames from atlas metadata + segment data.
/// This replaces the JS per-frame quad loop in overlayBaker.ts.
pub fn generate_overlay_frames(
    meta: &OverlayAtlasMetadata,
    trim_segments: &[TrimSegment],
    speed_points: &[SpeedPoint],
    fps: u32,
    canvas_width: f64,
    canvas_height: f64,
) -> Vec<OverlayFrame> {
    if trim_segments.is_empty() {
        return Vec::new();
    }

    let atlas_w = meta.atlas_width as f64;
    let atlas_h = meta.atlas_height.max(1) as f64;
    let out_dt = 1.0 / fps as f64;
    let delay_sec = meta.keystroke_delay_sec;
    let has_keystrokes = meta.keystroke_mode != "off" && !meta.display_events.is_empty();

    let overlay_x = if meta.overlay_x > 0.0 {
        meta.overlay_x
    } else {
        DEFAULT_KEYSTROKE_OVERLAY_X
    };
    let overlay_y = if meta.overlay_y > 0.0 {
        meta.overlay_y
    } else {
        DEFAULT_KEYSTROKE_OVERLAY_Y
    };
    let overlay_scale = if meta.overlay_scale > 0.0 {
        meta.overlay_scale
            .clamp(KEYSTROKE_OVERLAY_MIN_SCALE, KEYSTROKE_OVERLAY_MAX_SCALE)
    } else {
        DEFAULT_KEYSTROKE_OVERLAY_SCALE
    };
    let _ = overlay_scale; // Scale is baked into the atlas bubble layout widths

    let anchor_x_px = (overlay_x / 100.0) * canvas_width;
    let baseline_y_px = (overlay_y / 100.0) * canvas_height;

    // Build effective ends for display events
    let effective_ends: Vec<f64> = meta.display_events.iter().map(|e| e.end_time).collect();

    // Build keystroke atlas entry lookup by unique_key
    let keystroke_map: std::collections::HashMap<&str, &KeystrokeAtlasEntry> = meta
        .keystroke_entries
        .iter()
        .map(|e| (e.unique_key.as_str(), e))
        .collect();

    let end_time = trim_segments.last().unwrap().end_time;
    let mut seg_idx = 0usize;
    let mut t = trim_segments[0].start_time;
    let mut frames = Vec::new();

    while t < end_time - 1e-9 {
        // Advance to next trim segment if needed
        while seg_idx < trim_segments.len() && t >= trim_segments[seg_idx].end_time {
            seg_idx += 1;
            if seg_idx < trim_segments.len() {
                t = trim_segments[seg_idx].start_time;
            }
        }
        if seg_idx >= trim_segments.len() {
            break;
        }

        let mut quads = Vec::new();

        // Text overlays
        emit_text_quads(&mut quads, meta, t, atlas_w, atlas_h);

        // Keystroke overlays
        if has_keystrokes && is_time_inside_segments(t, &meta.visibility_segments) {
            emit_keystroke_quads(
                &mut quads,
                meta,
                &keystroke_map,
                &effective_ends,
                t,
                delay_sec,
                canvas_width,
                canvas_height,
                anchor_x_px,
                baseline_y_px,
                atlas_w,
                atlas_h,
            );
        }

        if quads.is_empty() {
            frames.push(OverlayFrame {
                frame_index: None,
                quads: Vec::new(),
            });
        } else {
            frames.push(OverlayFrame {
                frame_index: None,
                quads,
            });
        }

        let speed = get_speed(t, speed_points).clamp(0.1, 16.0);
        t += speed * out_dt;
    }

    frames
}

fn emit_text_quads(
    quads: &mut Vec<OverlayQuad>,
    meta: &OverlayAtlasMetadata,
    t: f64,
    atlas_w: f64,
    atlas_h: f64,
) {
    for text in &meta.text_entries {
        if t >= text.start_time && t <= text.end_time {
            let elapsed = t - text.start_time;
            let remaining = text.end_time - t;
            let mut alpha = 1.0_f64;
            if elapsed < TEXT_FADE_DUR {
                alpha = elapsed / TEXT_FADE_DUR;
            }
            if remaining < TEXT_FADE_DUR {
                alpha = alpha.min(remaining / TEXT_FADE_DUR);
            }
            if alpha > 0.001 {
                quads.push(OverlayQuad {
                    x: text.hit_x - text.pad,
                    y: text.hit_y - text.pad,
                    w: text.rect_w,
                    h: text.rect_h,
                    u: text.rect_x / atlas_w as f32,
                    v: text.rect_y / atlas_h as f32,
                    uw: text.rect_w / atlas_w as f32,
                    vh: text.rect_h / atlas_h as f32,
                    alpha: alpha as f32,
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_keystroke_quads(
    quads: &mut Vec<OverlayQuad>,
    meta: &OverlayAtlasMetadata,
    keystroke_map: &std::collections::HashMap<&str, &KeystrokeAtlasEntry>,
    effective_ends: &[f64],
    t: f64,
    delay_sec: f64,
    canvas_width: f64,
    canvas_height: f64,
    anchor_x_px: f64,
    baseline_y_px: f64,
    atlas_w: f64,
    atlas_h: f64,
) {
    let keyboard_events = find_active_events(
        &meta.keyboard_start_times,
        &meta.keyboard_indices,
        &meta.display_events,
        effective_ends,
        &meta.event_identities,
        &meta.event_slots,
        t,
        delay_sec,
        meta.keyboard_max_duration,
    );
    let mouse_events = if meta.keystroke_mode == "keyboardMouse" {
        find_active_events(
            &meta.mouse_start_times,
            &meta.mouse_indices,
            &meta.display_events,
            effective_ends,
            &meta.event_identities,
            &meta.event_slots,
            t,
            delay_sec,
            meta.mouse_max_duration,
        )
    } else {
        Vec::new()
    };

    let build_lane_items = |active_events: &[ActiveEvent]| -> Vec<LaneItem> {
        let mut items = Vec::new();
        for ae in active_events {
            let event = &meta.display_events[ae.event_index];
            let is_mouse = event.event_type == "mousedown" || event.event_type == "wheel";
            let visual =
                get_keystroke_visual_state(t, ae.start_time, ae.end_time, is_mouse, event.is_hold);
            if visual.alpha <= 0.001 {
                continue;
            }
            let entry = keystroke_map.get(event.unique_key.as_str());
            let (
                layout_width,
                layout_height,
                layout_font_size,
                layout_margin_bottom,
                pad,
                bubble_width,
            ) = if let Some(e) = entry {
                (
                    e.layout_width as f64,
                    e.layout_height as f64,
                    e.layout_font_size as f64,
                    e.layout_margin_bottom as f64,
                    e.pad as f64,
                    e.bubble_width as f64,
                )
            } else {
                continue;
            };
            items.push(LaneItem {
                event_index: ae.event_index,
                visual,
                bubble_width,
                layout_width,
                layout_height,
                layout_font_size,
                layout_margin_bottom,
                pad,
                slot: ae.slot,
            });
        }
        items.sort_by(|a, b| {
            a.slot.cmp(&b.slot).then_with(|| {
                let a_start = meta.display_events[a.event_index].start_time;
                let b_start = meta.display_events[b.event_index].start_time;
                b_start
                    .partial_cmp(&a_start)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        items
    };

    let keyboard_items = build_lane_items(&keyboard_events);
    let mouse_items = build_lane_items(&mouse_events);

    let lane_gap_px = get_lane_gap_px(
        keyboard_items
            .first()
            .map(|i| i.layout_font_size)
            .unwrap_or(16.0),
        mouse_items
            .first()
            .map(|i| i.layout_font_size)
            .unwrap_or(16.0),
    );

    let keyboard_slot_hints = compute_slot_width_hints(
        &meta.keyboard_slot_representative_widths,
        &keyboard_items,
        canvas_height,
    );
    let mouse_slot_hints = compute_slot_width_hints(
        &meta.mouse_slot_representative_widths,
        &mouse_items,
        canvas_height,
    );

    let emit_lane_quads =
        |items: &[LaneItem], placements: &[Placement], quads: &mut Vec<OverlayQuad>| {
            for p in placements {
                let item = &items[p.item_idx];
                let event = &meta.display_events[item.event_index];
                let entry = match keystroke_map.get(event.unique_key.as_str()) {
                    Some(e) => e,
                    None => continue,
                };
                let visual = &item.visual;
                let base_w = item.layout_width + item.pad * 2.0;
                let base_h = item.layout_height + item.pad * 2.0;
                let draw_w = base_w * visual.scale * visual.scale_x;
                let draw_h = base_h * visual.scale * visual.scale_y;
                let cx = p.x + item.layout_width / 2.0;
                let cy = p.y + item.layout_height / 2.0 + visual.translate_y;
                let quad_x = cx - draw_w / 2.0;
                let quad_y = cy - draw_h / 2.0;
                let mix = clamp01(visual.hold_mix);

                let alpha_held = visual.alpha * mix;
                let alpha_normal = if alpha_held >= 0.999 {
                    0.0
                } else {
                    (visual.alpha * (1.0 - mix)) / (1.0 - alpha_held)
                };

                if alpha_normal > 0.001 {
                    quads.push(OverlayQuad {
                        x: quad_x as f32,
                        y: quad_y as f32,
                        w: draw_w as f32,
                        h: draw_h as f32,
                        u: entry.normal_rect_x / atlas_w as f32,
                        v: entry.normal_rect_y / atlas_h as f32,
                        uw: entry.normal_rect_w / atlas_w as f32,
                        vh: entry.normal_rect_h / atlas_h as f32,
                        alpha: alpha_normal as f32,
                    });
                }
                if alpha_held > 0.001 {
                    quads.push(OverlayQuad {
                        x: quad_x as f32,
                        y: quad_y as f32,
                        w: draw_w as f32,
                        h: draw_h as f32,
                        u: entry.held_rect_x / atlas_w as f32,
                        v: entry.held_rect_y / atlas_h as f32,
                        uw: entry.held_rect_w / atlas_w as f32,
                        vh: entry.held_rect_h / atlas_h as f32,
                        alpha: alpha_held as f32,
                    });
                }
            }
        };

    let keyboard_placements = layout_keystroke_lane(
        &keyboard_items,
        canvas_width,
        lane_gap_px,
        true, // right-align
        &keyboard_slot_hints,
        anchor_x_px,
        baseline_y_px,
    );
    let mouse_placements = layout_keystroke_lane(
        &mouse_items,
        canvas_width,
        lane_gap_px,
        false, // left-align
        &mouse_slot_hints,
        anchor_x_px,
        baseline_y_px,
    );

    emit_lane_quads(&keyboard_items, &keyboard_placements, quads);
    emit_lane_quads(&mouse_items, &mouse_placements, quads);
}
