//! Allocate intersecting source footprints once, before any asynchronous reveal.
use std::collections::HashMap;

use anyhow::{Result, bail};

use super::PixelRegion;

pub(super) fn partition(
    regions: &[(u16, usize, PixelRegion)],
) -> Result<HashMap<u16, Vec<PixelRegion>>> {
    // The tighter source rectangle owns an overlap; a larger surrounding
    // heading must leave room for an independently located caption. Stable
    // source identity, not completion or paint order, breaks geometric ties.
    let mut ranked = regions.to_vec();
    ranked.sort_by_key(|&(id, _, r)| (u64::from(r.width) * u64::from(r.height), id));
    let mut occupied: Vec<(usize, PixelRegion)> = Vec::new();
    let mut result = HashMap::new();
    for (id, owner, region) in ranked {
        let mut pieces = vec![region];
        for &(previous_owner, previous) in &occupied {
            if previous_owner == owner {
                continue;
            }
            pieces = pieces
                .into_iter()
                .flat_map(|piece| subtract(piece, previous))
                .collect();
            if pieces.is_empty() {
                // No physical area can display this independent unit safely.
                // Preserve the capture instead of silently covering its text.
                bail!("source text footprints have no independent paint area");
            }
        }
        occupied.extend(pieces.iter().map(|&piece| (owner, piece)));
        result.insert(id, pieces);
    }
    Ok(result)
}

fn subtract(region: PixelRegion, obstacle: PixelRegion) -> Vec<PixelRegion> {
    let right = region.x + region.width;
    let bottom = region.y + region.height;
    let left = region.x.max(obstacle.x);
    let top = region.y.max(obstacle.y);
    let end_x = right.min(obstacle.x + obstacle.width);
    let end_y = bottom.min(obstacle.y + obstacle.height);
    if left >= end_x || top >= end_y {
        return vec![region];
    }
    [
        PixelRegion {
            height: top - region.y,
            ..region
        },
        PixelRegion {
            y: end_y,
            height: bottom - end_y,
            ..region
        },
        PixelRegion {
            y: top,
            width: left - region.x,
            height: end_y - top,
            ..region
        },
        PixelRegion {
            x: end_x,
            y: top,
            width: right - end_x,
            height: end_y - top,
        },
    ]
    .into_iter()
    .filter(|r| r.width > 0 && r.height > 0)
    .collect()
}

/// Join only the whitespace between consecutive lines belonging to one unit.
/// Independent source rectangles remain obstacles, including unrevealed ones.
pub(super) fn join_lines(
    mut shapes: Vec<PixelRegion>,
    members: &[PixelRegion],
    obstacles: &[PixelRegion],
) -> Vec<PixelRegion> {
    let mut lines = members.to_vec();
    lines.sort_by_key(|r| (r.y, r.x));
    for pair in lines.windows(2) {
        let [a, b] = [pair[0], pair[1]];
        let top = a.y + a.height;
        let left = a.x.max(b.x);
        let right = (a.x + a.width).min(b.x + b.width);
        if b.y <= top || right <= left || b.y - top > a.height.max(b.height) * 2 {
            continue;
        }
        let mut bridge = vec![PixelRegion {
            x: left,
            y: top,
            width: right - left,
            height: b.y - top,
        }];
        for &obstacle in obstacles {
            bridge = bridge
                .into_iter()
                .flat_map(|r| subtract(r, obstacle))
                .collect();
        }
        shapes.extend(bridge);
    }
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u32, y: u32, width: u32, height: u32) -> PixelRegion {
        PixelRegion {
            x,
            y,
            width,
            height,
        }
    }

    fn contains(r: PixelRegion, x: u32, y: u32) -> bool {
        x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height
    }

    #[test]
    fn overlapping_units_have_one_pixel_owner_and_preserve_the_source_union() {
        for second in [rect(5, 5, 10, 5), rect(10, 15, 20, 10), rect(20, 0, 5, 10)] {
            let inputs = [(1, 0, rect(0, 0, 20, 20)), (2, 1, second)];
            let result = partition(&inputs).unwrap();
            let reversed = partition(&[inputs[1], inputs[0]]).unwrap();
            assert_eq!(result, reversed);
            for y in 0..30 {
                for x in 0..35 {
                    let before = inputs.iter().any(|&(_, _, r)| contains(r, x, y));
                    let owners = result
                        .values()
                        .filter(|rs| rs.iter().any(|&r| contains(r, x, y)))
                        .count();
                    assert_eq!(owners, usize::from(before));
                }
            }
        }
    }

    #[test]
    fn members_of_the_same_unit_keep_their_full_footprints() {
        let inputs = [(1, 0, rect(0, 0, 20, 20)), (2, 0, rect(5, 5, 10, 5))];
        let result = partition(&inputs).unwrap();
        assert_eq!(result[&1], [inputs[0].2]);
        assert_eq!(result[&2], [inputs[1].2]);
    }

    #[test]
    fn impossible_independent_geometry_fails_without_silently_erasing_a_unit() {
        assert!(partition(&[(1, 0, rect(0, 0, 20, 20)), (2, 1, rect(0, 0, 20, 20))]).is_err());
    }
}
