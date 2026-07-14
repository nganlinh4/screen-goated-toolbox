use super::super::session::View;
use super::DetBox;

/// Keep the visible set bounded without letting one dense toolbar consume every
/// mark. Take the strongest proposal in each coarse spatial bucket first, then
/// fill remaining slots by confidence and return predictable reading order.
pub(in crate::overlay::computer_control) fn select_marks(
    mut boxes: Vec<DetBox>,
    view: View,
    limit: usize,
) -> Vec<DetBox> {
    if limit == 0 {
        return Vec::new();
    }
    if boxes.len() <= limit {
        boxes.sort_by_key(spatial_key);
        return boxes;
    }
    boxes.sort_by(|left, right| right.score.total_cmp(&left.score));
    const COLUMNS: usize = 6;
    const ROWS: usize = 4;
    let mut occupied = [false; COLUMNS * ROWS];
    let mut selected = vec![false; boxes.len()];
    let mut selected_count = 0;
    for (index, item) in boxes.iter().enumerate() {
        let column = bucket(item.cx, view.x, view.w, COLUMNS);
        let row = bucket(item.cy, view.y, view.h, ROWS);
        let slot = row * COLUMNS + column;
        if !occupied[slot] {
            occupied[slot] = true;
            selected[index] = true;
            selected_count += 1;
            if selected_count == limit {
                break;
            }
        }
    }
    if selected_count < limit {
        for is_selected in &mut selected {
            if !*is_selected {
                *is_selected = true;
                selected_count += 1;
                if selected_count == limit {
                    break;
                }
            }
        }
    }
    let mut result: Vec<_> = boxes
        .into_iter()
        .zip(selected)
        .filter_map(|(item, keep)| keep.then_some(item))
        .collect();
    result.sort_by_key(spatial_key);
    result
}

fn bucket(position: i32, origin: i32, length: i32, count: usize) -> usize {
    let offset = i64::from(position - origin).clamp(0, i64::from(length.saturating_sub(1).max(0)));
    ((offset * count as i64) / i64::from(length.max(1))).min(count.saturating_sub(1) as i64)
        as usize
}

fn spatial_key(item: &DetBox) -> (i32, i32, i32, i32) {
    (item.top, item.left, item.bottom, item.right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detected(cx: i32, cy: i32, score: f32) -> DetBox {
        DetBox {
            cx,
            cy,
            score,
            left: cx - 5,
            top: cy - 5,
            right: cx + 5,
            bottom: cy + 5,
            label: None,
        }
    }

    #[test]
    fn spatial_pass_preserves_remote_regions_before_dense_fill() {
        let view = View {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        };
        let mut boxes: Vec<_> = (0..40)
            .map(|index| detected(50 + index, 50 + index, 0.99 - index as f32 / 1000.0))
            .collect();
        boxes.push(detected(1100, 700, 0.70));
        let selected = select_marks(boxes, view, 10);
        assert_eq!(selected.len(), 10);
        assert!(
            selected
                .iter()
                .any(|item| item.cx == 1100 && item.cy == 700)
        );
    }

    #[test]
    fn selection_handles_zero_limit_and_negative_origins() {
        let view = View {
            x: -1000,
            y: -400,
            w: 1200,
            h: 800,
        };
        assert!(select_marks(vec![detected(-900, -300, 0.8)], view, 0).is_empty());
        assert_eq!(
            select_marks(vec![detected(-900, -300, 0.8)], view, 30).len(),
            1
        );
    }
}
