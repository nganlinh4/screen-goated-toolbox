use super::super::{contract::DetectedTextRegion, geometry::PixelRegion};

pub(super) fn connected_components(
    located: &[(&DetectedTextRegion, PixelRegion)],
) -> Vec<Vec<u16>> {
    let mut assigned = vec![false; located.len()];
    let mut components = Vec::new();
    for start in 0..located.len() {
        if assigned[start] {
            continue;
        }
        assigned[start] = true;
        let mut pending = vec![start];
        let mut members = Vec::new();
        while let Some(index) = pending.pop() {
            members.push(located[index].0.id);
            for candidate in 0..located.len() {
                if !assigned[candidate] && touches(located[index].1, located[candidate].1) {
                    assigned[candidate] = true;
                    pending.push(candidate);
                }
            }
        }
        members.sort_unstable();
        components.push(members);
    }
    components.sort_by_key(|members| members[0]);
    components
}

pub(super) fn touches(left: PixelRegion, right: PixelRegion) -> bool {
    left.x <= right.x.saturating_add(right.width)
        && right.x <= left.x.saturating_add(left.width)
        && left.y <= right.y.saturating_add(right.height)
        && right.y <= left.y.saturating_add(left.height)
}
