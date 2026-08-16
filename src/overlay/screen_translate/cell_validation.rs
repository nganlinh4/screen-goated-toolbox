use std::collections::HashSet;

use anyhow::{Result, bail};

use super::contract::{DetectedTextRegion, MemberJoin, NormalizedBounds};

pub(super) fn validate_cell_members(
    member_ids: &[u16],
    joins: &[MemberJoin],
    candidates: &[DetectedTextRegion],
) -> Result<NormalizedBounds> {
    if member_ids.is_empty() {
        bail!("translation cell has no members");
    }
    if joins.len() != member_ids.len().saturating_sub(1) {
        bail!("translation cell join count does not match its members");
    }
    let mut seen = HashSet::with_capacity(member_ids.len());
    let members = member_ids
        .iter()
        .map(|id| {
            if !seen.insert(*id) {
                bail!("translation cell contains a duplicate member");
            }
            candidates
                .iter()
                .find(|candidate| candidate.id == *id)
                .ok_or_else(|| anyhow::anyhow!("translation cell references an unknown member"))
        })
        .collect::<Result<Vec<_>>>()?;
    for ((left, right), join) in members.windows(2).map(|pair| (pair[0], pair[1])).zip(joins) {
        if !join_is_spatially_plausible(left, right, *join) {
            bail!("translation cell declares an implausible spatial join");
        }
    }
    let bounds = union_bounds(members.into_iter().map(|member| member.bounds));
    if candidates.iter().any(|candidate| {
        !seen.contains(&candidate.id)
            && candidate.bounds != bounds
            && substantially_enclosed(candidate.bounds, bounds)
    }) {
        bail!("translation cell encloses an unrelated text region");
    }
    Ok(bounds)
}

fn substantially_enclosed(candidate: NormalizedBounds, bounds: NormalizedBounds) -> bool {
    let width = candidate.right.saturating_sub(candidate.left).max(1);
    let height = candidate.bottom.saturating_sub(candidate.top).max(1);
    let intersection_width = candidate
        .right
        .min(bounds.right)
        .saturating_sub(candidate.left.max(bounds.left));
    let intersection_height = candidate
        .bottom
        .min(bounds.bottom)
        .saturating_sub(candidate.top.max(bounds.top));
    u32::from(intersection_width) * u32::from(intersection_height) * 2
        >= u32::from(width) * u32::from(height)
}

fn join_is_spatially_plausible(
    left: &DetectedTextRegion,
    right: &DetectedTextRegion,
    join: MemberJoin,
) -> bool {
    let a = left.bounds;
    let b = right.bounds;
    let aw = a.right.saturating_sub(a.left).max(1);
    let ah = a.bottom.saturating_sub(a.top).max(1);
    let bw = b.right.saturating_sub(b.left).max(1);
    let bh = b.bottom.saturating_sub(b.top).max(1);
    let horizontal_overlap = overlap(a.left, a.right, b.left, b.right);
    let vertical_overlap = overlap(a.top, a.bottom, b.top, b.bottom);
    let horizontal_gap = axis_gap(a.left, a.right, b.left, b.right);
    let vertical_gap = axis_gap(a.top, a.bottom, b.top, b.bottom);
    match join {
        MemberJoin::SameLine => {
            vertical_overlap.saturating_mul(2) >= ah.min(bh)
                && horizontal_gap <= ah.max(bh).saturating_mul(6)
        }
        MemberJoin::WrappedLine => {
            b.top >= a.top
                && vertical_gap <= ah.max(bh).saturating_mul(2)
                && (horizontal_overlap > 0
                    || a.left.abs_diff(b.left) <= ah.max(bh).saturating_mul(3))
        }
        MemberJoin::SameColumn => {
            horizontal_overlap.saturating_mul(2) >= aw.min(bw)
                && vertical_gap <= aw.max(bw).saturating_mul(6)
        }
        MemberJoin::SameBlock => {
            let scale = aw.min(ah).max(bw.min(bh)).max(1);
            horizontal_gap <= scale.saturating_mul(3)
                && vertical_gap <= scale.saturating_mul(3)
                && backgrounds_compatible(left, right)
        }
    }
}

fn backgrounds_compatible(left: &DetectedTextRegion, right: &DetectedTextRegion) -> bool {
    let (Some(left), Some(right)) = (left.appearance, right.appearance) else {
        return true;
    };
    if left.background_confidence < 60 || right.background_confidence < 60 {
        return true;
    }
    left.background_rgb
        .into_iter()
        .zip(right.background_rgb)
        .map(|(a, b)| a.abs_diff(b))
        .max()
        .unwrap_or(0)
        <= 54
}

fn overlap(a_start: u16, a_end: u16, b_start: u16, b_end: u16) -> u16 {
    a_end.min(b_end).saturating_sub(a_start.max(b_start))
}

fn axis_gap(a_start: u16, a_end: u16, b_start: u16, b_end: u16) -> u16 {
    if a_end < b_start {
        b_start - a_end
    } else {
        a_start.saturating_sub(b_end)
    }
}

fn union_bounds(bounds: impl Iterator<Item = NormalizedBounds>) -> NormalizedBounds {
    bounds.fold(
        NormalizedBounds {
            left: u16::MAX,
            top: u16::MAX,
            right: 0,
            bottom: 0,
        },
        |union, bounds| NormalizedBounds {
            left: union.left.min(bounds.left),
            top: union.top.min(bounds.top),
            right: union.right.max(bounds.right),
            bottom: union.bottom.max(bounds.bottom),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: u16, bounds: [u16; 4]) -> DetectedTextRegion {
        DetectedTextRegion {
            id,
            bounds: bounds.into(),
            source_text: id.to_string(),
            source_alternatives: vec![id.to_string()],
            appearance: None,
        }
    }

    #[test]
    fn wrapped_lines_may_form_an_unbounded_logical_cell() {
        let candidates = (0..8)
            .map(|index| candidate(index + 1, [index * 25, 20, index * 25 + 20, 220]))
            .collect::<Vec<_>>();
        let ids = (1..=8).collect::<Vec<_>>();
        let joins = vec![MemberJoin::WrappedLine; 7];
        assert!(validate_cell_members(&ids, &joins, &candidates).is_ok());
    }

    #[test]
    fn a_cross_column_wrapped_line_is_rejected() {
        let candidates = vec![
            candidate(1, [10, 10, 30, 120]),
            candidate(2, [40, 700, 60, 820]),
        ];
        assert!(validate_cell_members(&[1, 2], &[MemberJoin::WrappedLine], &candidates).is_err());
    }

    #[test]
    fn a_cell_cannot_claim_geometry_across_an_unrelated_region() {
        let candidates = vec![
            candidate(1, [10, 10, 60, 60]),
            candidate(2, [70, 10, 120, 60]),
            candidate(3, [130, 10, 180, 60]),
        ];
        assert!(validate_cell_members(&[1, 3], &[MemberJoin::SameLine], &candidates).is_err());
    }
}
