// Overlay frame types, math helpers, and visual state animation.

use super::super::config::{CursorVisibilitySegment, SpeedPoint};

// --- Constants (mirror keystrokeTypes.ts) ---
pub(crate) const KEYSTROKE_ANIM_ENTER_SEC: f64 = 0.18;
pub(crate) const KEYSTROKE_ANIM_EXIT_SEC: f64 = 0.20;
pub(super) const KEYSTROKE_SLOT_SPARSE_GAP_LIMIT: usize = 2;
pub(super) const DEFAULT_KEYSTROKE_OVERLAY_X: f64 = 50.0;
pub(super) const DEFAULT_KEYSTROKE_OVERLAY_Y: f64 = 100.0;
pub(super) const DEFAULT_KEYSTROKE_OVERLAY_SCALE: f64 = 1.0;
pub(super) const KEYSTROKE_OVERLAY_MIN_SCALE: f64 = 0.45;
pub(super) const KEYSTROKE_OVERLAY_MAX_SCALE: f64 = 2.4;
pub(super) const TEXT_FADE_DUR: f64 = 0.3;

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

pub(super) fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

pub(super) fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * clamp01(t)
}

pub(super) fn ease_out_cubic(t: f64) -> f64 {
    let p = 1.0 - clamp01(t);
    1.0 - p * p * p
}

pub(super) fn ease_in_cubic(t: f64) -> f64 {
    let p = clamp01(t);
    p * p * p
}

pub(super) fn upper_bound(sorted: &[f64], value: f64) -> usize {
    sorted.partition_point(|&v| v <= value)
}

pub(super) fn is_time_inside_segments(time: f64, segments: &[CursorVisibilitySegment]) -> bool {
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

pub(super) fn get_speed(time: f64, speed_points: &[SpeedPoint]) -> f64 {
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

pub(super) fn compute_percentile(values: &mut [f64], percentile: f64) -> f64 {
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

pub(super) struct VisualState {
    pub alpha: f64,
    pub scale: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub translate_y: f64,
    pub hold_mix: f64,
    pub lane_weight: f64,
}

pub(super) fn get_keystroke_visual_state(
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

pub(super) struct ActiveEvent {
    pub event_index: usize,
    pub start_time: f64,
    pub end_time: f64,
    pub slot: usize,
}

#[expect(
    clippy::too_many_arguments,
    reason = "event lookup needs all timeline arrays"
)]
pub(super) fn find_active_events(
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

/// Bubble gap between bubbles in the same lane.
pub(super) fn get_keystroke_lane_bubble_gap_px(font_size: f64) -> f64 {
    (font_size * 0.22).max(8.0).round()
}
