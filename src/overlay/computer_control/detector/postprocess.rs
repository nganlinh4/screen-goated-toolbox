use super::MAX_CANDIDATES;

/// A detected clickable region in physical SCREEN px.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DetBox {
    pub(crate) cx: i32,
    pub(crate) cy: i32,
    pub(crate) score: f32,
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) right: i32,
    pub(crate) bottom: i32,
    pub(crate) label: Option<String>,
}

#[derive(Debug)]
pub(super) struct PostprocessResult {
    pub boxes: Vec<DetBox>,
    pub thresholded: usize,
    pub rejected_invalid: usize,
    pub suppressed_duplicates: usize,
    pub truncated: usize,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn postprocess(
    dets_shape: &[i64],
    dets: &[f32],
    labels_shape: &[i64],
    labels: &[f32],
    crop_width: f32,
    crop_height: f32,
    origin_x: i32,
    origin_y: i32,
    threshold: f32,
    duplicate_iou_threshold: f32,
) -> anyhow::Result<PostprocessResult> {
    validate_shapes(dets_shape, dets, labels_shape, labels)?;
    if !crop_width.is_finite()
        || !crop_height.is_finite()
        || crop_width <= 0.0
        || crop_height <= 0.0
    {
        anyhow::bail!("invalid crop size {crop_width}x{crop_height}");
    }
    let classes = labels_shape[2] as usize;
    let mut candidates = Vec::new();
    let mut thresholded = 0;
    let mut rejected_invalid = 0;
    for (coords, logits) in dets.chunks_exact(4).zip(labels.chunks_exact(classes)) {
        let Some(best) = logits
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .reduce(f32::max)
        else {
            rejected_invalid += 1;
            continue;
        };
        let score = sigmoid(best);
        if score < threshold {
            continue;
        }
        thresholded += 1;
        let Some(bounds) = normalized_bounds(coords) else {
            rejected_invalid += 1;
            continue;
        };
        let [left, top, right, bottom] = bounds;
        let screen_left = origin_x + (left * crop_width).round() as i32;
        let screen_top = origin_y + (top * crop_height).round() as i32;
        let screen_right = origin_x + (right * crop_width).round() as i32;
        let screen_bottom = origin_y + (bottom * crop_height).round() as i32;
        if screen_right <= screen_left || screen_bottom <= screen_top {
            rejected_invalid += 1;
            continue;
        }
        candidates.push(DetBox {
            cx: screen_left + (screen_right - screen_left) / 2,
            cy: screen_top + (screen_bottom - screen_top) / 2,
            score,
            left: screen_left,
            top: screen_top,
            right: screen_right,
            bottom: screen_bottom,
            label: None,
        });
    }
    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));

    let mut deduplicated: Vec<DetBox> = Vec::with_capacity(candidates.len());
    let mut suppressed_duplicates = 0;
    for candidate in candidates {
        if deduplicated.iter().any(|accepted| {
            intersection_over_union(&candidate, accepted) > duplicate_iou_threshold
                || same_click_outcome(&candidate, accepted)
        }) {
            suppressed_duplicates += 1;
        } else {
            deduplicated.push(candidate);
        }
    }
    let truncated = deduplicated.len().saturating_sub(MAX_CANDIDATES);
    deduplicated.truncate(MAX_CANDIDATES);
    // IDs should be predictable from the screenshot, not confidence rank.
    deduplicated.sort_by_key(|item| (item.top, item.left, item.bottom, item.right));
    Ok(PostprocessResult {
        boxes: deduplicated,
        thresholded,
        rejected_invalid,
        suppressed_duplicates,
        truncated,
    })
}

fn same_click_outcome(left: &DetBox, right: &DetBox) -> bool {
    let dx = i64::from(left.cx) - i64::from(right.cx);
    let dy = i64::from(left.cy) - i64::from(right.cy);
    dx * dx + dy * dy <= 12 * 12
}

fn validate_shapes(
    dets_shape: &[i64],
    dets: &[f32],
    labels_shape: &[i64],
    labels: &[f32],
) -> anyhow::Result<()> {
    if dets_shape.len() != 3 || dets_shape[0] != 1 || dets_shape[2] != 4 {
        anyhow::bail!("unexpected dets shape {dets_shape:?}; expected [1,N,4]");
    }
    if labels_shape.len() != 3 || labels_shape[0] != 1 {
        anyhow::bail!("unexpected labels shape {labels_shape:?}; expected [1,N,C]");
    }
    let queries = usize::try_from(dets_shape[1])
        .map_err(|_| anyhow::anyhow!("invalid dets query dimension {}", dets_shape[1]))?;
    let label_queries = usize::try_from(labels_shape[1])
        .map_err(|_| anyhow::anyhow!("invalid labels query dimension {}", labels_shape[1]))?;
    let classes = usize::try_from(labels_shape[2])
        .map_err(|_| anyhow::anyhow!("invalid labels class dimension {}", labels_shape[2]))?;
    if queries != label_queries || classes == 0 {
        anyhow::bail!("incompatible output shapes: dets {dets_shape:?}, labels {labels_shape:?}");
    }
    let expected_dets = queries
        .checked_mul(4)
        .ok_or_else(|| anyhow::anyhow!("dets shape overflows address space"))?;
    let expected_labels = queries
        .checked_mul(classes)
        .ok_or_else(|| anyhow::anyhow!("labels shape overflows address space"))?;
    if dets.len() != expected_dets || labels.len() != expected_labels {
        anyhow::bail!(
            "tensor length mismatch: dets {} != {expected_dets}, labels {} != {expected_labels}",
            dets.len(),
            labels.len()
        );
    }
    Ok(())
}

fn normalized_bounds(coords: &[f32]) -> Option<[f32; 4]> {
    if coords.len() != 4 || coords.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let [cx, cy, width, height] = [coords[0], coords[1], coords[2], coords[3]];
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let left = (cx - width / 2.0).clamp(0.0, 1.0);
    let top = (cy - height / 2.0).clamp(0.0, 1.0);
    let right = (cx + width / 2.0).clamp(0.0, 1.0);
    let bottom = (cy + height / 2.0).clamp(0.0, 1.0);
    (right > left && bottom > top).then_some([left, top, right, bottom])
}

fn sigmoid(value: f32) -> f32 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

fn intersection_over_union(a: &DetBox, b: &DetBox) -> f32 {
    let intersection_width = (a.right.min(b.right) - a.left.max(b.left)).max(0);
    let intersection_height = (a.bottom.min(b.bottom) - a.top.max(b.top)).max(0);
    let intersection = intersection_width as i64 * intersection_height as i64;
    let area_a = (a.right - a.left).max(0) as i64 * (a.bottom - a.top).max(0) as i64;
    let area_b = (b.right - b.left).max(0) as i64 * (b.bottom - b.top).max(0) as i64;
    let union = area_a + area_b - intersection;
    if union <= 0 {
        0.0
    } else {
        intersection as f32 / union as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(dets: &[f32], labels: &[f32], queries: i64) -> anyhow::Result<PostprocessResult> {
        postprocess(
            &[1, queries, 4],
            dets,
            &[1, queries, 1],
            labels,
            200.0,
            100.0,
            -50,
            20,
            0.45,
            0.92,
        )
    }

    #[test]
    fn maps_clipped_box_to_physical_screen_pixels() {
        let result = process(&[0.05, 0.5, 0.2, 0.4], &[4.0], 1).unwrap();
        assert_eq!(result.boxes.len(), 1);
        assert_eq!(result.boxes[0].left, -50);
        assert_eq!(result.boxes[0].right, -20);
        assert_eq!(result.boxes[0].top, 50);
        assert_eq!(result.boxes[0].bottom, 90);
        assert_eq!((result.boxes[0].cx, result.boxes[0].cy), (-35, 70));
    }

    #[test]
    fn rejects_malformed_tensors_without_indexing_them() {
        let bad_shape = postprocess(
            &[1, 2, 3],
            &[0.0; 6],
            &[1, 2, 1],
            &[1.0; 2],
            100.0,
            100.0,
            0,
            0,
            0.45,
            0.92,
        );
        assert!(bad_shape.unwrap_err().to_string().contains("dets shape"));
        let short_data = process(&[0.5; 4], &[1.0, 1.0], 2);
        assert!(
            short_data
                .unwrap_err()
                .to_string()
                .contains("length mismatch")
        );
    }

    #[test]
    fn rejects_non_finite_degenerate_and_fully_offscreen_boxes() {
        let result = process(
            &[
                f32::NAN,
                0.5,
                0.2,
                0.2,
                0.5,
                0.5,
                -0.2,
                0.2,
                2.0,
                2.0,
                0.1,
                0.1,
            ],
            &[4.0, 4.0, 4.0],
            3,
        )
        .unwrap();
        assert!(result.boxes.is_empty());
        assert_eq!(result.rejected_invalid, 3);
    }

    #[test]
    fn dedupe_keeps_one_mark_for_the_same_click_outcome() {
        let result = process(
            &[
                0.25, 0.5, 0.2, 0.2, // strongest duplicate
                0.251, 0.5, 0.2, 0.2, // near-exact duplicate
                0.25, 0.5, 0.1, 0.1, // legitimate nested control
                0.75, 0.5, 0.2, 0.2, // separate control
            ],
            &[5.0, 4.0, 3.5, 3.0],
            4,
        )
        .unwrap();
        assert_eq!(result.boxes.len(), 2);
        assert_eq!(result.suppressed_duplicates, 2);
        assert_eq!(result.truncated, 0);
        assert_eq!(result.boxes[0].cx, 0);
        assert!(result.boxes.iter().any(|item| item.cx == 100));
    }

    #[test]
    fn stable_sigmoid_handles_extreme_logits() {
        assert_eq!(sigmoid(f32::INFINITY), 1.0);
        assert_eq!(sigmoid(f32::NEG_INFINITY), 0.0);
        assert!((sigmoid(0.0) - 0.5).abs() < f32::EPSILON);
    }
}
