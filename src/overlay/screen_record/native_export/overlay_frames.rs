// Rust-side overlay frame quad generation.
//
// Replaces the JS per-frame loop in overlayBaker.ts. Instead of JS generating
// 40K+ OverlayFrame objects, serializing them to JSON, and sending over IPC,
// Rust generates the quads directly from compact atlas metadata + segment data.
//
// Input: OverlayAtlasMetadata (few KB) sent once from JS after atlas baking.
// Output: Vec<OverlayFrame> indexed by frame_idx.

use super::config::{CursorVisibilitySegment, OverlayFrame, OverlayQuad, SpeedPoint, TrimSegment};

// --- Constants (mirror keystrokeTypes.ts) ---
const KEYSTROKE_ANIM_ENTER_SEC: f64 = 0.18;
const KEYSTROKE_ANIM_EXIT_SEC: f64 = 0.20;
const KEYSTROKE_SLOT_SPARSE_GAP_LIMIT: usize = 2;
const DEFAULT_KEYSTROKE_OVERLAY_X: f64 = 50.0;
const DEFAULT_KEYSTROKE_OVERLAY_Y: f64 = 100.0;
const DEFAULT_KEYSTROKE_OVERLAY_SCALE: f64 = 1.0;
const KEYSTROKE_OVERLAY_MIN_SCALE: f64 = 0.45;
const KEYSTROKE_OVERLAY_MAX_SCALE: f64 = 2.4;
const TEXT_FADE_DUR: f64 = 0.3;

// --- Metadata types (deserialized from JS IPC) ---

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OverlayAtlasMetadata {
    pub atlas_width: u32,
    pub atlas_height: u32,
    #[serde(default)]
    pub text_entries: Vec<TextAtlasEntry>,
    #[serde(default)]
    pub keystroke_entries: Vec<KeystrokeAtlasEntry>,
    #[serde(default)]
    pub keystroke_mode: String,
    #[serde(default)]
    pub keystroke_delay_sec: f64,
    #[serde(default)]
    pub overlay_x: f64,
    #[serde(default)]
    pub overlay_y: f64,
    #[serde(default = "default_overlay_scale")]
    pub overlay_scale: f64,
    #[serde(default)]
    pub visibility_segments: Vec<CursorVisibilitySegment>,
    #[serde(default)]
    pub display_events: Vec<KeystrokeExportEvent>,
    #[serde(default)]
    pub keyboard_start_times: Vec<f64>,
    #[serde(default)]
    pub keyboard_indices: Vec<usize>,
    #[serde(default)]
    pub mouse_start_times: Vec<f64>,
    #[serde(default)]
    pub mouse_indices: Vec<usize>,
    #[serde(default)]
    pub keyboard_max_duration: f64,
    #[serde(default)]
    pub mouse_max_duration: f64,
    #[serde(default)]
    pub event_slots: Vec<usize>,
    #[serde(default)]
    pub event_identities: Vec<String>,
    #[serde(default)]
    pub keyboard_slot_representative_widths: Vec<f64>,
    #[serde(default)]
    pub mouse_slot_representative_widths: Vec<f64>,
}

fn default_overlay_scale() -> f64 {
    1.0
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct TextAtlasEntry {
    pub id: String,
    pub start_time: f64,
    pub end_time: f64,
    pub rect_x: f32,
    pub rect_y: f32,
    pub rect_w: f32,
    pub rect_h: f32,
    pub hit_x: f32,
    pub hit_y: f32,
    pub pad: f32,
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KeystrokeAtlasEntry {
    pub unique_key: String,
    pub normal_rect_x: f32,
    pub normal_rect_y: f32,
    pub normal_rect_w: f32,
    pub normal_rect_h: f32,
    pub held_rect_x: f32,
    pub held_rect_y: f32,
    pub held_rect_w: f32,
    pub held_rect_h: f32,
    pub layout_width: f32,
    pub layout_height: f32,
    pub layout_font_size: f32,
    pub layout_margin_bottom: f32,
    pub pad: f32,
    pub bubble_width: f32,
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct KeystrokeExportEvent {
    pub id: String,
    pub unique_key: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub start_time: f64,
    pub end_time: f64,
    #[serde(default)]
    pub is_hold: bool,
}

// --- Math helpers ---

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * clamp01(t)
}

fn ease_out_cubic(t: f64) -> f64 {
    let p = 1.0 - clamp01(t);
    1.0 - p * p * p
}

fn ease_in_cubic(t: f64) -> f64 {
    let p = clamp01(t);
    p * p * p
}

fn upper_bound(sorted: &[f64], value: f64) -> usize {
    sorted.partition_point(|&v| v <= value)
}

fn is_time_inside_segments(time: f64, segments: &[CursorVisibilitySegment]) -> bool {
    if segments.is_empty() {
        return false;
    }
    let idx = segments.partition_point(|s| s.start_time <= time);
    if idx == 0 {
        return false;
    }
    let seg = &segments[idx - 1];
    time >= seg.start_time && time <= seg.end_time
}

fn get_speed(time: f64, speed_points: &[SpeedPoint]) -> f64 {
    if speed_points.is_empty() {
        return 1.0;
    }
    let idx = speed_points.partition_point(|p| p.time <= time);
    if idx == 0 {
        return speed_points[0].speed.max(0.1);
    }
    if idx >= speed_points.len() {
        return speed_points.last().unwrap().speed.max(0.1);
    }
    let prev = &speed_points[idx - 1];
    let next = &speed_points[idx];
    let span = (next.time - prev.time).max(1e-10);
    let t = ((time - prev.time) / span).clamp(0.0, 1.0);
    (prev.speed + (next.speed - prev.speed) * t).max(0.1)
}

fn compute_percentile(values: &mut [f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((values.len() as f64 - 1.0) * percentile)
        .floor()
        .max(0.0)
        .min((values.len() - 1) as f64) as usize;
    values[idx]
}

// --- Visual state animation (mirrors getKeystrokeVisualState) ---

struct VisualState {
    alpha: f64,
    scale: f64,
    scale_x: f64,
    scale_y: f64,
    translate_y: f64,
    hold_mix: f64,
    lane_weight: f64,
}

fn get_keystroke_visual_state(
    current_time: f64,
    event_start: f64,
    event_end: f64,
    is_mouse: bool,
    is_hold: bool,
) -> VisualState {
    let duration = (event_end - event_start).max(0.001);
    let enter_span = KEYSTROKE_ANIM_ENTER_SEC.min(duration * 0.36);
    let exit_span = KEYSTROKE_ANIM_EXIT_SEC.min(duration * 0.36);
    let exit_start = event_start.max(event_end - exit_span);

    let mut state = VisualState {
        alpha: 1.0,
        scale: 1.0,
        scale_x: 1.0,
        scale_y: 1.0,
        translate_y: 0.0,
        hold_mix: 0.0,
        lane_weight: 1.0,
    };

    // Enter animation
    if current_time < event_start + enter_span {
        let t = clamp01((current_time - event_start) / enter_span.max(0.001));
        let eased = ease_out_cubic(t);
        let lane_lead = ease_out_cubic(clamp01((t - 0.03) / 0.42));
        state.alpha = lerp(0.0, 1.0, eased);
        state.scale = lerp(0.93, 1.01, eased);
        state.scale_x = lerp(1.1, 1.0, eased);
        state.scale_y = lerp(0.9, 1.0, eased);
        state.translate_y = lerp(10.0, 0.0, eased);
        state.lane_weight = lane_lead;
        if t > 0.76 {
            let settle_t = (t - 0.76) / 0.24;
            state.scale = lerp(state.scale, 1.0, ease_out_cubic(settle_t));
        }
    }

    // Hold animation
    if is_hold {
        let hold_start = event_start + enter_span * 0.6;
        let hold_end = event_end - exit_span * 0.4;
        if hold_end > hold_start && current_time >= hold_start && current_time <= hold_end {
            let transition_sec = 0.08;
            let hold_mix_in = clamp01((current_time - hold_start) / transition_sec);
            let hold_mix_out = clamp01((hold_end - current_time) / transition_sec);
            let hold_mix = hold_mix_in.min(hold_mix_out);
            let squish = if is_mouse { 0.06 } else { 0.045 } * hold_mix;
            state.hold_mix = hold_mix;
            state.scale *= lerp(1.0, 1.014, hold_mix);
            state.scale_x *= 1.0 + squish;
            state.scale_y *= 1.0 - squish * 0.72;
            state.translate_y += lerp(0.0, -2.6, hold_mix);
            state.lane_weight = state.lane_weight.max(lerp(0.84, 1.0, hold_mix));
        }
    }

    // Exit animation
    if current_time > exit_start {
        let t = clamp01((current_time - exit_start) / (event_end - exit_start).max(0.001));
        let eased = ease_in_cubic(t);
        state.alpha *= lerp(1.0, 0.0, eased);
        state.scale *= lerp(1.0, 0.93, eased);
        state.scale_x *= lerp(1.0, 1.04, eased);
        state.scale_y *= lerp(1.0, 0.89, eased);
        state.translate_y += lerp(0.0, -9.0, eased);
        state.hold_mix *= lerp(1.0, 0.15, eased);
        state.lane_weight *= lerp(1.0, 0.76, eased);
    }

    state
}

// --- Active event lookup (mirrors findActiveKeystrokeEventsForKind) ---

struct ActiveEvent {
    event_index: usize,
    start_time: f64,
    end_time: f64,
    slot: usize,
}

fn find_active_events(
    start_times: &[f64],
    indices: &[usize],
    events: &[KeystrokeExportEvent],
    effective_ends: &[f64],
    event_identities: &[String],
    event_slots: &[usize],
    current_time: f64,
    delay_sec: f64,
    max_duration: f64,
) -> Vec<ActiveEvent> {
    if start_times.is_empty() || indices.is_empty() {
        return Vec::new();
    }
    let idx = upper_bound(start_times, current_time - delay_sec);
    if idx == 0 {
        return Vec::new();
    }
    let min_start_candidate = current_time - delay_sec - max_duration - 0.000001;
    let mut active = Vec::new();
    let mut seen_identities = std::collections::HashSet::new();

    let mut cursor = idx - 1;
    loop {
        if cursor >= indices.len() {
            break;
        }
        let event_index = indices[cursor];
        if event_index >= events.len() {
            if cursor == 0 {
                break;
            }
            cursor -= 1;
            continue;
        }
        let event = &events[event_index];
        if event.start_time < min_start_candidate {
            break;
        }
        let delayed_start = event.start_time + delay_sec;
        let delayed_end = effective_ends
            .get(event_index)
            .copied()
            .unwrap_or(event.end_time)
            + delay_sec;
        if current_time >= delayed_start && current_time <= delayed_end {
            let identity = event_identities
                .get(event_index)
                .cloned()
                .unwrap_or_default();
            if !seen_identities.contains(&identity) {
                seen_identities.insert(identity);
                active.push(ActiveEvent {
                    event_index,
                    start_time: delayed_start,
                    end_time: delayed_end,
                    slot: event_slots.get(event_index).copied().unwrap_or(0),
                });
            }
        }
        if cursor == 0 {
            break;
        }
        cursor -= 1;
    }

    active.sort_by(|a, b| {
        a.slot.cmp(&b.slot).then_with(|| {
            b.start_time
                .partial_cmp(&a.start_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    active
}

// --- Lane layout (mirrors layoutKeystrokeLane) ---

struct LaneItem {
    event_index: usize,
    visual: VisualState,
    bubble_width: f64,
    layout_width: f64,
    layout_height: f64,
    layout_font_size: f64,
    layout_margin_bottom: f64,
    pad: f64,
    slot: usize,
}

struct Placement {
    item_idx: usize,
    x: f64,
    y: f64,
}

fn get_keystroke_lane_bubble_gap_px(font_size: f64) -> f64 {
    (font_size * 0.22).max(8.0).round()
}

fn get_keystroke_pair_gap_px(primary_font_size: f64, secondary_font_size: f64) -> f64 {
    (primary_font_size.max(secondary_font_size) * 0.58)
        .max(14.0)
        .round()
}

fn get_slot_advance_px(
    slot_index: usize,
    bubble_gap: f64,
    slot_width_hints: &[f64],
    fallback_width: f64,
) -> f64 {
    let slot_width = slot_width_hints
        .get(slot_index)
        .copied()
        .unwrap_or(fallback_width);
    let full_advance = slot_width + bubble_gap;
    if slot_index < KEYSTROKE_SLOT_SPARSE_GAP_LIMIT {
        full_advance
    } else {
        let tail_offset = (slot_index - KEYSTROKE_SLOT_SPARSE_GAP_LIMIT + 1) as f64;
        full_advance * (0.64 / tail_offset.sqrt())
    }
}

fn layout_keystroke_lane(
    items: &[LaneItem],
    canvas_width: f64,
    lane_gap_px: f64,
    align_right: bool,
    slot_width_hints: &[f64],
    anchor_x_px: f64,
    baseline_y_px: f64,
) -> Vec<Placement> {
    if items.is_empty() {
        return Vec::new();
    }

    let max_font = items
        .iter()
        .map(|i| i.layout_font_size)
        .fold(16.0_f64, f64::max);
    let margin_x = (max_font * 0.34).max(10.0).round();
    let left_bound = margin_x;
    let right_bound = (canvas_width - margin_x).max(left_bound);
    let barrier_x = anchor_x_px;
    let center_anchor = if align_right {
        barrier_x - lane_gap_px * 0.5
    } else {
        barrier_x + lane_gap_px * 0.5
    };
    let bubble_gap = get_keystroke_lane_bubble_gap_px(max_font);
    let hint_average = if !slot_width_hints.is_empty() {
        slot_width_hints.iter().sum::<f64>() / slot_width_hints.len() as f64
    } else {
        items.iter().map(|i| i.bubble_width).sum::<f64>() / items.len() as f64
    };
    let max_active_slot = items.iter().map(|i| i.slot).max().unwrap_or(0);
    let max_hint_slot = slot_width_hints.len().saturating_sub(1);
    let max_slot = max_active_slot.max(max_hint_slot);

    let mut slot_offsets = vec![0.0_f64; max_slot + 1];
    for slot in 1..=max_slot {
        slot_offsets[slot] = slot_offsets[slot - 1]
            + get_slot_advance_px(slot - 1, bubble_gap, slot_width_hints, hint_average);
    }

    // Compress if spread exceeds max
    let max_raw_offset = *slot_offsets.last().unwrap_or(&0.0);
    let max_spread_px = (canvas_width * 0.24).clamp(150.0, 290.0).round();
    if max_raw_offset > max_spread_px {
        let preserve_slot = 2.min(max_slot);
        let preserve_px = slot_offsets.get(preserve_slot).copied().unwrap_or(0.0);
        let compressible_raw = (max_raw_offset - preserve_px).max(0.0);
        let compressible_target = (max_spread_px - preserve_px).max(0.0);
        let compression = if compressible_raw > 0.0 {
            (compressible_target / compressible_raw).clamp(0.0, 1.0)
        } else {
            1.0
        };
        for slot in (preserve_slot + 1)..=max_slot {
            let raw = slot_offsets[slot];
            slot_offsets[slot] = preserve_px + (raw - preserve_px) * compression;
        }
    }

    let mut placements: Vec<Placement> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let y = (baseline_y_px - item.layout_height as f64 - item.layout_margin_bottom as f64)
                .round();
            let slot_offset = slot_offsets.get(item.slot).copied().unwrap_or(0.0);
            let x = if align_right {
                let right_edge = center_anchor - slot_offset;
                right_edge - item.bubble_width
            } else {
                center_anchor + slot_offset
            };
            Placement {
                item_idx: idx,
                x,
                y,
            }
        })
        .collect();

    // Collision avoidance
    if align_right {
        for i in 1..placements.len() {
            let prev_x = placements[i - 1].x;
            let prev_font = items[placements[i - 1].item_idx].layout_font_size;
            let cur_font = items[placements[i].item_idx].layout_font_size;
            let pair_gap = get_keystroke_lane_bubble_gap_px(prev_font.max(cur_font));
            let cur_bw = items[placements[i].item_idx].bubble_width;
            let max_allowed_x = prev_x - pair_gap - cur_bw;
            if placements[i].x > max_allowed_x {
                placements[i].x = max_allowed_x;
            }
        }
        let leftmost_x = placements.last().map(|p| p.x).unwrap_or(0.0);
        let overflow = left_bound - leftmost_x;
        if overflow > 0.001 {
            let mut max_shift = f64::INFINITY;
            for p in &placements {
                let bw = items[p.item_idx].bubble_width;
                let placement_max_x = left_bound.max(right_bound - bw);
                max_shift = max_shift.min(placement_max_x - p.x);
            }
            let shift = overflow.min(max_shift).max(0.0);
            if shift > 0.001 {
                for p in &mut placements {
                    p.x += shift;
                }
            }
        }
    } else {
        for i in 1..placements.len() {
            let prev_x = placements[i - 1].x;
            let prev_bw = items[placements[i - 1].item_idx].bubble_width;
            let prev_font = items[placements[i - 1].item_idx].layout_font_size;
            let cur_font = items[placements[i].item_idx].layout_font_size;
            let pair_gap = get_keystroke_lane_bubble_gap_px(prev_font.max(cur_font));
            let min_allowed_x = prev_x + prev_bw + pair_gap;
            if placements[i].x < min_allowed_x {
                placements[i].x = min_allowed_x;
            }
        }
        let rightmost = placements
            .last()
            .map(|p| {
                let bw = items[p.item_idx].bubble_width;
                p.x + bw
            })
            .unwrap_or(0.0);
        let overflow = rightmost - right_bound;
        if overflow > 0.001 {
            let mut max_left_shift = f64::INFINITY;
            for p in &placements {
                max_left_shift = max_left_shift.min(p.x - left_bound);
            }
            let shift = overflow.min(max_left_shift).max(0.0);
            if shift > 0.001 {
                for p in &mut placements {
                    p.x -= shift;
                }
            }
        }
    }

    // Final clamp
    for p in &mut placements {
        let bw = items[p.item_idx].bubble_width;
        let max_x = left_bound.max(right_bound - bw);
        p.x = p.x.max(left_bound).min(max_x).round();
    }

    placements
}

// --- Slot width hints (mirrors getKeystrokeSlotWidthHints) ---

fn compute_slot_width_hints(
    representative_widths: &[f64],
    lane_items: &[LaneItem],
    canvas_height: f64,
) -> Vec<f64> {
    let mut measured_widths: Vec<f64> = representative_widths
        .iter()
        .copied()
        .filter(|w| *w > 0.0)
        .collect();
    let lane_average = if !lane_items.is_empty() {
        lane_items.iter().map(|i| i.bubble_width).sum::<f64>() / lane_items.len() as f64
    } else {
        0.0
    };
    let median_width = compute_percentile(&mut measured_widths, 0.5);
    let fallback_width = if median_width > 0.0 {
        median_width
    } else if lane_average > 0.0 {
        lane_average
    } else {
        (canvas_height * 0.09).max(110.0)
    };
    let min_width = (fallback_width * 0.56).max(36.0);
    let max_width = (fallback_width * 1.24).max(min_width);

    representative_widths
        .iter()
        .map(|&raw| {
            let w = if raw > 0.0 && raw.is_finite() {
                raw
            } else {
                fallback_width
            };
            w.max(min_width).min(max_width)
        })
        .collect()
}

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

        // Keystroke overlays
        if has_keystrokes && is_time_inside_segments(t, &meta.visibility_segments) {
            let keyboard_events = find_active_events(
                &meta.keyboard_start_times,
                &meta.keyboard_indices,
                &meta.display_events,
                &effective_ends,
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
                    &effective_ends,
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
                    let visual = get_keystroke_visual_state(
                        t,
                        ae.start_time,
                        ae.end_time,
                        is_mouse,
                        event.is_hold,
                    );
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

            let lane_gap_px = get_keystroke_pair_gap_px(
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

            let emit_quads =
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

            emit_quads(&keyboard_items, &keyboard_placements, &mut quads);
            emit_quads(&mouse_items, &mouse_placements, &mut quads);
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
