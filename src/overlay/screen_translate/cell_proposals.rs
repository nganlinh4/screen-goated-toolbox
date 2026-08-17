use std::collections::HashSet;

use serde::Serialize;

use super::contract::{DetectedTextRegion, MemberJoin, NormalizedBounds};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CellProposal {
    pub member_ids_in_reading_order: Vec<u16>,
    pub member_joins: Vec<MemberJoin>,
}

pub(super) fn propose(candidates: &[DetectedTextRegion]) -> Vec<CellProposal> {
    let mut ordered = candidates.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|candidate| {
        (
            center(candidate.bounds.top, candidate.bounds.bottom),
            candidate.bounds.left,
        )
    });
    let mut used = HashSet::new();
    let mut proposals = Vec::new();
    for start in &ordered {
        if !used.insert(start.id) {
            continue;
        }
        let mut members = vec![start.id];
        let mut joins = Vec::new();
        let mut current = *start;
        while members.len() < 4
            && let Some((next, join)) = ordered
                .iter()
                .filter(|candidate| !used.contains(&candidate.id))
                .filter_map(|candidate| {
                    proposed_join(current, candidate).map(|join| (*candidate, join))
                })
                .filter(|(candidate, join)| {
                    let mut trial_members = members.clone();
                    trial_members.push(candidate.id);
                    let mut trial_joins = joins.clone();
                    trial_joins.push(*join);
                    super::cell_validation::validate_cell_members(
                        &trial_members,
                        &trial_joins,
                        candidates,
                    )
                    .is_ok()
                })
                .min_by_key(|(candidate, _)| reading_distance(current.bounds, candidate.bounds))
        {
            used.insert(next.id);
            members.push(next.id);
            joins.push(join);
            current = next;
        }
        proposals.push(CellProposal {
            member_ids_in_reading_order: members,
            member_joins: joins,
        });
    }
    proposals
}

fn proposed_join(first: &DetectedTextRegion, second: &DetectedTextRegion) -> Option<MemberJoin> {
    let a = first.bounds;
    let b = second.bounds;
    let aw = width(a);
    let ah = height(a);
    let bw = width(b);
    let bh = height(b);
    if ah > aw.saturating_mul(3) / 2
        || bh > bw.saturating_mul(3) / 2
        || ah.max(bh) > ah.min(bh).saturating_mul(2)
        || !backgrounds_compatible(first, second)
    {
        return None;
    }
    let vertical_overlap = overlap(a.top, a.bottom, b.top, b.bottom);
    let horizontal_gap = axis_gap(a.left, a.right, b.left, b.right);
    if vertical_overlap.saturating_mul(2) >= ah.min(bh)
        && b.left >= a.left
        && horizontal_gap <= ah.max(bh)
    {
        return Some(MemberJoin::SameLine);
    }
    let vertical_gap = axis_gap(a.top, a.bottom, b.top, b.bottom);
    let horizontal_overlap = overlap(a.left, a.right, b.left, b.right);
    let scale = ah.max(bh);
    let left_aligned = a.left.abs_diff(b.left).saturating_mul(2) <= scale;
    let right_aligned = a.right.abs_diff(b.right).saturating_mul(2) <= scale;
    let substantial_overlap = horizontal_overlap.saturating_mul(4) >= aw.min(bw).saturating_mul(3);
    (b.top > a.top
        && vertical_gap.saturating_mul(2) <= scale
        && (substantial_overlap || (left_aligned && right_aligned)))
        .then_some(MemberJoin::WrappedLine)
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

fn reading_distance(a: NormalizedBounds, b: NormalizedBounds) -> u32 {
    u32::from(center(a.top, a.bottom).abs_diff(center(b.top, b.bottom))) * 4
        + u32::from(a.left.abs_diff(b.left))
}

fn center(start: u16, end: u16) -> u16 {
    start.saturating_add(end.saturating_sub(start) / 2)
}

fn width(bounds: NormalizedBounds) -> u16 {
    bounds.right.saturating_sub(bounds.left).max(1)
}

fn height(bounds: NormalizedBounds) -> u16 {
    bounds.bottom.saturating_sub(bounds.top).max(1)
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
    fn aligned_wrapped_lines_are_proposed_as_one_cell() {
        let proposals = propose(&[
            candidate(1, [10, 10, 30, 220]),
            candidate(2, [35, 12, 55, 210]),
            candidate(3, [60, 11, 80, 180]),
        ]);
        assert_eq!(proposals[0].member_ids_in_reading_order, [1, 2, 3]);
        assert_eq!(
            proposals[0].member_joins,
            [MemberJoin::WrappedLine, MemberJoin::WrappedLine]
        );
    }

    #[test]
    fn distant_columns_are_separate_proposals() {
        let proposals = propose(&[
            candidate(1, [10, 10, 30, 120]),
            candidate(2, [35, 10, 55, 120]),
            candidate(3, [10, 700, 30, 820]),
        ]);
        assert_eq!(proposals.len(), 2);
    }

    #[test]
    fn repeated_interface_rows_do_not_chain_into_a_paragraph() {
        let proposals = propose(&[
            candidate(1, [10, 10, 30, 180]),
            candidate(2, [46, 10, 66, 180]),
            candidate(3, [82, 10, 102, 180]),
        ]);
        assert_eq!(proposals.len(), 3);
    }

    #[test]
    fn proposals_are_bounded_even_for_dense_long_passages() {
        let candidates = (0..9)
            .map(|index| candidate(index + 1, [10 + index * 22, 10, 30 + index * 22, 180]))
            .collect::<Vec<_>>();
        assert!(
            propose(&candidates)
                .iter()
                .all(|proposal| proposal.member_ids_in_reading_order.len() <= 4)
        );
    }

    #[test]
    fn a_proposal_never_encloses_an_unclaimed_region() {
        let proposals = propose(&[
            candidate(1, [10, 10, 30, 220]),
            candidate(2, [35, 80, 55, 120]),
            candidate(3, [60, 10, 80, 220]),
        ]);
        assert!(!proposals.iter().any(|proposal| {
            proposal.member_ids_in_reading_order.contains(&1)
                && proposal.member_ids_in_reading_order.contains(&3)
                && !proposal.member_ids_in_reading_order.contains(&2)
        }));
    }
}
