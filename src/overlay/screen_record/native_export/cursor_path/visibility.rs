// Cursor visibility, mousedown event helpers, squish easing, and mouse position normalization.

use crate::overlay::screen_record::native_export::config::{
    CursorVisibilitySegment, KeystrokeEvent, MousePosition, VideoSegment,
};

// --- Visibility fade constants (mirror cursorHiding.ts) ---
const FADE_IN_DURATION: f64 = 0.2;
const FADE_OUT_DURATION: f64 = 0.25;
pub(super) const SCALE_HIDDEN: f64 = 0.5;

// --- Squish state machine constants ---
pub(super) const CLICK_FUSE_THRESHOLD: f64 = 0.05;
pub(super) const SQUISH_TARGET: f64 = 0.75;
pub(super) const QUICK_CLICK_WINDOW: f64 = 0.1;
pub(super) const SQUISH_DOWN_DUR_BASE: f64 = 0.10;
pub(super) const SQUISH_DOWN_DUR_MIN: f64 = 0.04;
pub(super) const RELEASE_DUR_BASE: f64 = 0.15;
pub(super) const RELEASE_DUR_MIN: f64 = 0.04;

// ─── Easing functions ──────────────────────────────────────────────────────────

fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_cubic(t: f64) -> f64 {
    t * t * t
}

pub(super) fn squish_ease_down(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

pub(super) fn squish_ease_up(t: f64, has_room: bool) -> f64 {
    if !has_room {
        return 1.0 - (1.0 - t).powi(3);
    }
    let c = 1.2_f64; // Preview overshoot profile.
    let tm1 = t - 1.0;
    1.0 + (c + 1.0) * tm1.powi(3) + c * tm1.powi(2)
}

// ─── Cursor visibility (mirrors getCursorVisibility in cursorHiding.ts) ────────

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

pub(super) fn get_cursor_visibility(
    time: f64,
    segments: &Option<Vec<CursorVisibilitySegment>>,
) -> (f64, f64) {
    // (opacity, scale)
    let Some(segs) = segments else {
        // Feature off — always visible
        return (1.0, 1.0);
    };

    if segs.is_empty() {
        return (0.0, SCALE_HIDDEN);
    }

    // Binary search: find the last segment whose start_time <= time
    let idx = segs.partition_point(|s| s.start_time <= time);
    // Check the candidate segment (idx-1) if it contains time
    if idx > 0 {
        let seg = &segs[idx - 1];
        if time <= seg.end_time {
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
    }

    (0.0, SCALE_HIDDEN)
}

// ─── Mousedown event helpers ───────────────────────────────────────────────────

pub(super) fn get_keystroke_delay_sec(segment: &VideoSegment) -> f64 {
    segment.keystroke_delay_sec.clamp(-1.0, 1.0)
}

pub(super) fn has_mousedown_events(segment: &VideoSegment) -> bool {
    segment
        .keystroke_events
        .iter()
        .any(|e| e.event_type.eq_ignore_ascii_case("mousedown"))
}

#[derive(Clone, Copy)]
pub(super) struct MouseDownEventView {
    pub idx: usize,
    pub start_time: f64,
    pub end_time: f64,
    pub is_hold: bool,
}

fn to_mousedown_event(
    idx: usize,
    e: &KeystrokeEvent,
) -> MouseDownEventView {
    MouseDownEventView {
        idx,
        start_time: e.start_time,
        end_time: e.end_time,
        is_hold: e.is_hold,
    }
}

/// Pre-filtered and sorted list of mousedown events for O(log n) lookup.
pub(super) fn build_mousedown_events(segment: &VideoSegment) -> Vec<MouseDownEventView> {
    let mut events: Vec<MouseDownEventView> = segment
        .keystroke_events
        .iter()
        .enumerate()
        .filter(|(_, e)| e.event_type.eq_ignore_ascii_case("mousedown"))
        .map(|(idx, e)| to_mousedown_event(idx, e))
        .collect();
    events.sort_by(|a, b| {
        a.start_time
            .partial_cmp(&b.start_time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    events
}

pub(super) fn find_active_mousedown_event_fast(
    events: &[MouseDownEventView],
    lookup_time: f64,
) -> Option<MouseDownEventView> {
    // Binary search to find the last event whose start_time <= lookup_time
    let idx = events.partition_point(|e| e.start_time <= lookup_time);
    // Check the event just before and at the partition point
    for &check_idx in &[idx.wrapping_sub(1), idx] {
        if check_idx < events.len() {
            let ev = events[check_idx];
            let active_end = if ev.is_hold {
                ev.end_time
            } else {
                ev.start_time + QUICK_CLICK_WINDOW
            };
            if lookup_time >= ev.start_time && lookup_time <= active_end {
                return Some(ev);
            }
        }
    }
    None
}

pub(super) fn find_prev_mousedown_event_fast(
    events: &[MouseDownEventView],
    before_time: f64,
) -> Option<MouseDownEventView> {
    let idx = events.partition_point(|e| e.start_time < before_time);
    if idx > 0 { Some(events[idx - 1]) } else { None }
}

pub(super) fn find_next_mousedown_event_fast(
    events: &[MouseDownEventView],
    after_time: f64,
) -> Option<MouseDownEventView> {
    let idx = events.partition_point(|e| e.start_time <= after_time);
    if idx < events.len() {
        Some(events[idx])
    } else {
        None
    }
}

pub(super) fn effective_event_end(event: MouseDownEventView) -> f64 {
    if event.is_hold {
        event.end_time
    } else {
        event.start_time + QUICK_CLICK_WINDOW
    }
}

// ─── Mouse position normalization ──────────────────────────────────────────────

pub(super) fn normalize_mouse_positions_to_source_frame(
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
