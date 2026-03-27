// Keystroke lane layout logic (mirrors layoutKeystrokeLane from JS).

use super::types::{
    KEYSTROKE_SLOT_SPARSE_GAP_LIMIT, VisualState, compute_percentile,
    get_keystroke_lane_bubble_gap_px,
};

pub(super) struct LaneItem {
    pub event_index: usize,
    pub visual: VisualState,
    pub bubble_width: f64,
    pub layout_width: f64,
    pub layout_height: f64,
    pub layout_font_size: f64,
    pub layout_margin_bottom: f64,
    pub pad: f64,
    pub slot: usize,
}

pub(super) struct Placement {
    pub item_idx: usize,
    pub x: f64,
    pub y: f64,
}

fn get_keystroke_pair_gap_px(primary_font_size: f64, secondary_font_size: f64) -> f64 {
    (primary_font_size.max(secondary_font_size) * 0.58)
        .max(14.0)
        .round()
}

pub(super) fn get_lane_gap_px(primary_font_size: f64, secondary_font_size: f64) -> f64 {
    get_keystroke_pair_gap_px(primary_font_size, secondary_font_size)
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

pub(super) fn layout_keystroke_lane(
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
    compress_slot_offsets(&mut slot_offsets, max_slot, canvas_width);

    let mut placements: Vec<Placement> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let y = (baseline_y_px - item.layout_height - item.layout_margin_bottom).round();
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
        apply_collision_right_align(&mut placements, items, left_bound, right_bound);
    } else {
        apply_collision_left_align(&mut placements, items, left_bound, right_bound);
    }

    // Final clamp
    for p in &mut placements {
        let bw = items[p.item_idx].bubble_width;
        let max_x = left_bound.max(right_bound - bw);
        p.x = p.x.max(left_bound).min(max_x).round();
    }

    placements
}

fn compress_slot_offsets(slot_offsets: &mut [f64], max_slot: usize, canvas_width: f64) {
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
        for offset in &mut slot_offsets[(preserve_slot + 1)..=max_slot] {
            let raw = *offset;
            *offset = preserve_px + (raw - preserve_px) * compression;
        }
    }
}

fn apply_collision_right_align(
    placements: &mut [Placement],
    items: &[LaneItem],
    left_bound: f64,
    right_bound: f64,
) {
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
        for p in placements.iter() {
            let bw = items[p.item_idx].bubble_width;
            let placement_max_x = left_bound.max(right_bound - bw);
            max_shift = max_shift.min(placement_max_x - p.x);
        }
        let shift = overflow.min(max_shift).max(0.0);
        if shift > 0.001 {
            for p in placements.iter_mut() {
                p.x += shift;
            }
        }
    }
}

fn apply_collision_left_align(
    placements: &mut [Placement],
    items: &[LaneItem],
    left_bound: f64,
    right_bound: f64,
) {
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
        for p in placements.iter() {
            max_left_shift = max_left_shift.min(p.x - left_bound);
        }
        let shift = overflow.min(max_left_shift).max(0.0);
        if shift > 0.001 {
            for p in placements.iter_mut() {
                p.x -= shift;
            }
        }
    }
}

// --- Slot width hints (mirrors getKeystrokeSlotWidthHints) ---

pub(super) fn compute_slot_width_hints(
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
