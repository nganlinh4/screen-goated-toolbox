use super::appearance::VisualSignature;
use super::geometry::PixelRegion;

#[derive(Clone, Copy)]
pub(super) struct LayoutInput {
    pub id: u16,
    pub pixels: PixelRegion,
    pub appearance: Option<VisualSignature>,
}

pub(super) struct LayoutBlock {
    pub member_ids: Vec<u16>,
    pub pixels: PixelRegion,
}

pub(super) fn plan_blocks(inputs: &[LayoutInput]) -> Vec<LayoutBlock> {
    let mut links = Vec::new();
    for left in 0..inputs.len() {
        for right in left + 1..inputs.len() {
            if let Some(score) = link_score(inputs[left], inputs[right]) {
                links.push((score, left, right));
            }
        }
    }
    links.sort_unstable();
    let mut parents = (0..inputs.len()).collect::<Vec<_>>();
    for (_, left, right) in links {
        let left_root = find(&mut parents, left);
        let right_root = find(&mut parents, right);
        if left_root == right_root {
            continue;
        }
        let members = (0..inputs.len())
            .filter(|index| {
                let root = find(&mut parents, *index);
                root == left_root || root == right_root
            })
            .collect::<Vec<_>>();
        if merge_is_valid(inputs, &members) {
            parents[right_root] = left_root;
        }
    }
    let mut groups = std::collections::BTreeMap::<usize, Vec<LayoutInput>>::new();
    for (index, input) in inputs.iter().copied().enumerate() {
        groups
            .entry(find(&mut parents, index))
            .or_default()
            .push(input);
    }
    let mut blocks = groups
        .into_values()
        .map(|mut members| {
            if members.iter().all(|member| is_vertical(member.pixels)) {
                members.sort_by_key(|member| (std::cmp::Reverse(member.pixels.x), member.pixels.y));
            } else {
                members.sort_by_key(|member| (member.pixels.y, member.pixels.x));
            }
            LayoutBlock {
                member_ids: members.iter().map(|member| member.id).collect(),
                pixels: union_pixels(members.iter().map(|member| member.pixels)),
            }
        })
        .collect::<Vec<_>>();
    blocks.sort_by_key(|block| (block.pixels.y, block.pixels.x));
    blocks
}

fn link_score(left: LayoutInput, right: LayoutInput) -> Option<u64> {
    let left_vertical = is_vertical(left.pixels);
    let right_vertical = is_vertical(right.pixels);
    if left_vertical != right_vertical || !style_compatible(left.appearance, right.appearance) {
        return None;
    }
    let linked = if left_vertical {
        vertical_columns_touch(left.pixels, right.pixels)
    } else {
        horizontal_rows_touch(left.pixels, right.pixels)
    };
    linked.then(|| center_distance_squared(left.pixels, right.pixels))
}

fn horizontal_rows_touch(left: PixelRegion, right: PixelRegion) -> bool {
    let (upper, lower) = if left.y <= right.y {
        (left, right)
    } else {
        (right, left)
    };
    let height = upper.height.max(lower.height).max(1);
    let gap = i64::from(lower.y) - i64::from(upper.y.saturating_add(upper.height));
    if gap < -i64::from(height) / 3 || gap > i64::from(height) * 2 / 5 {
        return false;
    }
    let overlap = overlap_length(upper.x, upper.width, lower.x, lower.width);
    let aligned_left = upper.x.abs_diff(lower.x) <= height.saturating_mul(3) / 2;
    overlap.saturating_mul(2) >= upper.width.min(lower.width).max(1) || aligned_left
}

fn vertical_columns_touch(left: PixelRegion, right: PixelRegion) -> bool {
    let overlap = overlap_length(left.y, left.height, right.y, right.height);
    if overlap.saturating_mul(2) < left.height.min(right.height).max(1) {
        return false;
    }
    let (first, second) = if left.x <= right.x {
        (left, right)
    } else {
        (right, left)
    };
    let gap = second.x.saturating_sub(first.x.saturating_add(first.width));
    gap <= left.width.max(right.width).saturating_mul(3) / 4
}

fn style_compatible(left: Option<VisualSignature>, right: Option<VisualSignature>) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return true;
    };
    let background_matches = left.background_confidence < 20
        || right.background_confidence < 20
        || color_distance(left.background_rgb, right.background_rgb) <= 52;
    let foreground_matches = left.foreground_confidence < 3
        || right.foreground_confidence < 3
        || match (left.foreground_rgb, right.foreground_rgb) {
            (Some(left), Some(right)) => color_distance(left, right) <= 96,
            _ => true,
        };
    background_matches && foreground_matches
}

fn merge_is_valid(inputs: &[LayoutInput], members: &[usize]) -> bool {
    let union = union_pixels(members.iter().map(|index| inputs[*index].pixels));
    let union_area = u64::from(union.width) * u64::from(union.height);
    let covered_area = members
        .iter()
        .map(|index| {
            let pixels = inputs[*index].pixels;
            u64::from(pixels.width) * u64::from(pixels.height)
        })
        .sum::<u64>();
    if covered_area.saturating_mul(100) < union_area.saturating_mul(45) {
        return false;
    }
    !(0..inputs.len())
        .filter(|index| !members.contains(index))
        .any(|outside| is_blocking_obstacle(inputs, members, outside, union))
}

fn is_blocking_obstacle(
    inputs: &[LayoutInput],
    members: &[usize],
    outside: usize,
    union: PixelRegion,
) -> bool {
    let region = inputs[outside].pixels;
    let intersection = intersection_area(union, region);
    let meaningful = intersection.saturating_mul(10)
        >= (u64::from(region.width) * u64::from(region.height)).max(40);
    meaningful
        && !members
            .iter()
            .any(|member| link_score(inputs[*member], inputs[outside]).is_some())
}

fn intersection_area(left: PixelRegion, right: PixelRegion) -> u64 {
    u64::from(overlap_length(left.x, left.width, right.x, right.width))
        * u64::from(overlap_length(left.y, left.height, right.y, right.height))
}

fn center_distance_squared(left: PixelRegion, right: PixelRegion) -> u64 {
    let left_center = (
        u64::from(left.x) * 2 + u64::from(left.width),
        u64::from(left.y) * 2 + u64::from(left.height),
    );
    let right_center = (
        u64::from(right.x) * 2 + u64::from(right.width),
        u64::from(right.y) * 2 + u64::from(right.height),
    );
    let dx = left_center.0.abs_diff(right_center.0);
    let dy = left_center.1.abs_diff(right_center.1);
    dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
}

fn color_distance(left: [u8; 3], right: [u8; 3]) -> u16 {
    left.into_iter()
        .zip(right)
        .map(|(left, right)| u16::from(left.abs_diff(right)))
        .sum()
}

fn overlap_length(left: u32, left_width: u32, right: u32, right_width: u32) -> u32 {
    left.saturating_add(left_width)
        .min(right.saturating_add(right_width))
        .saturating_sub(left.max(right))
}

fn is_vertical(region: PixelRegion) -> bool {
    region.height > region.width.saturating_mul(3) / 2
}

fn union_pixels(regions: impl Iterator<Item = PixelRegion>) -> PixelRegion {
    let mut left = u32::MAX;
    let mut top = u32::MAX;
    let mut right = 0;
    let mut bottom = 0;
    for region in regions {
        left = left.min(region.x);
        top = top.min(region.y);
        right = right.max(region.x.saturating_add(region.width));
        bottom = bottom.max(region.y.saturating_add(region.height));
    }
    PixelRegion {
        x: left,
        y: top,
        width: right.saturating_sub(left).max(1),
        height: bottom.saturating_sub(top).max(1),
    }
}

fn find(parents: &mut [usize], index: usize) -> usize {
    if parents[index] != index {
        parents[index] = find(parents, parents[index]);
    }
    parents[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u16, x: u32, y: u32, width: u32) -> LayoutInput {
        LayoutInput {
            id,
            pixels: PixelRegion {
                x,
                y,
                width,
                height: 20,
            },
            appearance: None,
        }
    }

    #[test]
    fn wrapped_rows_share_their_complete_union() {
        let blocks = plan_blocks(&[
            row(1, 10, 10, 300),
            row(2, 12, 28, 260),
            row(3, 12, 49, 240),
        ]);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].member_ids, [1, 2, 3]);
        assert_eq!(blocks[0].pixels.height, 59);
    }

    #[test]
    fn paragraph_breaks_and_parallel_columns_remain_separate() {
        let blocks = plan_blocks(&[
            row(1, 10, 10, 200),
            row(2, 10, 45, 200),
            row(3, 400, 10, 200),
        ]);
        assert_eq!(blocks.len(), 3);
    }

    #[test]
    fn stacked_rows_with_distinct_styles_remain_independent() {
        let signature = |background_rgb, foreground_rgb| VisualSignature {
            background_rgb,
            background_confidence: 80,
            foreground_rgb: Some(foreground_rgb),
            foreground_confidence: 40,
        };
        let mut first = row(1, 10, 10, 200);
        first.appearance = Some(signature([20, 20, 20], [250, 250, 250]));
        let mut second = row(2, 10, 28, 200);
        second.appearance = Some(signature([20, 20, 20], [40, 120, 240]));
        assert_eq!(plan_blocks(&[first, second]).len(), 2);
    }

    #[test]
    fn incompatible_element_inside_a_union_protects_its_layout_space() {
        let signature = |background_rgb, foreground_rgb| VisualSignature {
            background_rgb,
            background_confidence: 90,
            foreground_rgb: Some(foreground_rgb),
            foreground_confidence: 40,
        };
        let text_style = signature([5, 5, 5], [210, 210, 210]);
        let mut heading = row(1, 10, 10, 100);
        heading.appearance = Some(text_style);
        let mut first_line = row(2, 10, 30, 300);
        first_line.appearance = Some(text_style);
        let mut second_line = row(3, 10, 50, 220);
        second_line.appearance = Some(text_style);
        let mut control = row(4, 250, 10, 60);
        control.appearance = Some(signature([240, 240, 240], [20, 20, 20]));

        let blocks = plan_blocks(&[heading, first_line, second_line, control]);
        assert_eq!(blocks.len(), 3);
        assert!(blocks.iter().any(|block| block.member_ids == [2, 3]));
        assert!(blocks.iter().all(|block| block.member_ids != [1, 2, 3]));
    }

    #[test]
    fn adjacent_vertical_columns_share_a_readable_surface_in_reading_order() {
        let column = |id, x, width, height| LayoutInput {
            id,
            pixels: PixelRegion {
                x,
                y: 10,
                width,
                height,
            },
            appearance: None,
        };
        let blocks = plan_blocks(&[
            column(1, 44, 20, 80),
            column(2, 30, 20, 70),
            column(3, 0, 10, 75),
        ]);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].member_ids, [1, 2]);
    }
}
