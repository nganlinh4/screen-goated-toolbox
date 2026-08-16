use sgt_screen_text_detector_protocol::DetectedRegion;

const MIN_SIDE: u32 = 4;

pub(super) fn normalize(regions: &mut Vec<DetectedRegion>) {
    remove_nested_duplicates(regions);
    partition_overlapping_padding(regions);
}

fn remove_nested_duplicates(regions: &mut Vec<DetectedRegion>) {
    let snapshot = regions.clone();
    *regions = snapshot
        .iter()
        .enumerate()
        .filter(|(candidate_index, candidate)| {
            !snapshot
                .iter()
                .enumerate()
                .any(|(container_index, container)| {
                    candidate_index != &container_index
                        && ((same_bounds(candidate, container)
                            && candidate_index > &container_index)
                            || nested_annotation(candidate, container))
                })
        })
        .map(|(_, region)| region.clone())
        .collect();
}

fn nested_annotation(candidate: &DetectedRegion, container: &DetectedRegion) -> bool {
    if same_bounds(candidate, container) {
        return false;
    }
    let intersection = intersection_area(candidate, container);
    let candidate_area = area(candidate);
    if candidate_area == 0 || intersection.saturating_mul(100) < candidate_area.saturating_mul(80) {
        return false;
    }
    let candidate_minor = width(candidate).min(height(candidate));
    let container_minor = width(container).min(height(container));
    let candidate_major = width(candidate).max(height(candidate));
    let container_major = width(container).max(height(container));
    same_orientation(candidate, container)
        && candidate_minor.saturating_mul(2) <= container_minor
        && candidate_major.saturating_mul(3) <= container_major.saturating_mul(2)
}

fn partition_overlapping_padding(regions: &mut [DetectedRegion]) {
    for left_index in 0..regions.len() {
        for right_index in left_index + 1..regions.len() {
            let (head, tail) = regions.split_at_mut(right_index);
            let left = &mut head[left_index];
            let right = &mut tail[0];
            if intersection_area(left, right) == 0 {
                continue;
            }
            let left_center = center(left);
            let right_center = center(right);
            let dx = left_center.0.abs_diff(right_center.0);
            let dy = left_center.1.abs_diff(right_center.1);
            if dx >= dy && left_center.0 != right_center.0 {
                split_x(left, right, left_center.0, right_center.0);
            } else if left_center.1 != right_center.1 {
                split_y(left, right, left_center.1, right_center.1);
            }
        }
    }
}

fn split_x(left: &mut DetectedRegion, right: &mut DetectedRegion, a: u32, b: u32) {
    let boundary = a.saturating_add(b).div_ceil(2);
    let (before, after) = if a < b { (left, right) } else { (right, left) };
    if boundary >= before.left.saturating_add(MIN_SIDE)
        && boundary.saturating_add(MIN_SIDE) <= after.right
    {
        before.right = before.right.min(boundary);
        after.left = after.left.max(boundary);
    }
}

fn split_y(left: &mut DetectedRegion, right: &mut DetectedRegion, a: u32, b: u32) {
    let boundary = a.saturating_add(b).div_ceil(2);
    let (before, after) = if a < b { (left, right) } else { (right, left) };
    if boundary >= before.top.saturating_add(MIN_SIDE)
        && boundary.saturating_add(MIN_SIDE) <= after.bottom
    {
        before.bottom = before.bottom.min(boundary);
        after.top = after.top.max(boundary);
    }
}

fn same_orientation(left: &DetectedRegion, right: &DetectedRegion) -> bool {
    (height(left) > width(left).saturating_mul(3) / 2)
        == (height(right) > width(right).saturating_mul(3) / 2)
}

fn same_bounds(left: &DetectedRegion, right: &DetectedRegion) -> bool {
    (left.left, left.top, left.right, left.bottom)
        == (right.left, right.top, right.right, right.bottom)
}

fn center(region: &DetectedRegion) -> (u32, u32) {
    (
        region.left.saturating_add(region.right) / 2,
        region.top.saturating_add(region.bottom) / 2,
    )
}

fn intersection_area(left: &DetectedRegion, right: &DetectedRegion) -> u64 {
    let width = left
        .right
        .min(right.right)
        .saturating_sub(left.left.max(right.left));
    let height = left
        .bottom
        .min(right.bottom)
        .saturating_sub(left.top.max(right.top));
    u64::from(width) * u64::from(height)
}

fn width(region: &DetectedRegion) -> u32 {
    region.right.saturating_sub(region.left)
}

fn height(region: &DetectedRegion) -> u32 {
    region.bottom.saturating_sub(region.top)
}

fn area(region: &DetectedRegion) -> u64 {
    u64::from(width(region)) * u64::from(height(region))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(left: u32, top: u32, right: u32, bottom: u32, text: &str) -> DetectedRegion {
        DetectedRegion {
            left,
            top,
            right,
            bottom,
            confidence: 0.95,
            text: text.to_string(),
            text_confidence: 0.95,
            alternatives: Vec::new(),
        }
    }

    #[test]
    fn nested_annotation_does_not_become_an_independent_surface() {
        let mut regions = vec![
            region(40, 20, 80, 160, "complete passage"),
            region(65, 60, 76, 102, "annotation"),
        ];
        normalize(&mut regions);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].text, "complete passage");
    }

    #[test]
    fn padded_neighboring_columns_receive_disjoint_geometry() {
        let mut regions = vec![
            region(20, 10, 55, 150, "first"),
            region(48, 12, 82, 148, "second"),
        ];
        normalize(&mut regions);
        assert_eq!(regions.len(), 2);
        assert!(regions[0].right <= regions[1].left);
    }

    #[test]
    fn padded_wrapped_lines_receive_disjoint_geometry() {
        let mut regions = vec![
            region(10, 20, 200, 46, "first line"),
            region(12, 42, 180, 68, "second line"),
        ];
        normalize(&mut regions);
        assert!(regions[0].bottom <= regions[1].top);
    }
}
