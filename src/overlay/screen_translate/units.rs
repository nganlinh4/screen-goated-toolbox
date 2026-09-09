//! One source partition shared by request ownership, readiness and rendering.
use super::contract::{DetectedTextRegion, NormalizedBounds};
use super::geometry::normalized_region;
use sgt_screen_text_detector_protocol::stream::LayoutRegion;

mod boundaries;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct Unit {
    pub id: u16,
    pub members: Vec<u16>,
}

impl Unit {
    pub(super) fn replacement_region(
        &self,
        width: u32,
        height: u32,
        vertical: bool,
        wrap: bool,
    ) -> crate::overlay::result::SourceReplacementRegion {
        crate::overlay::result::SourceReplacementRegion {
            x: 0,
            y: 0,
            width,
            height,
            vertical,
            wrap,
        }
    }
}

pub(super) struct Plan {
    pub units: Vec<Unit>,
    pub candidates: Vec<DetectedTextRegion>,
    completed: Vec<bool>,
    source_indices: Vec<Vec<usize>>,
}

impl Plan {
    pub(super) fn new(
        image: &image::RgbaImage,
        sources: &mut [DetectedTextRegion],
        layout: &[LayoutRegion],
    ) -> Self {
        for source in sources.iter_mut() {
            source.appearance = super::appearance::analyze_region(
                image,
                normalized_region(source.bounds, image.width(), image.height()),
            );
        }
        let source_indices = boundaries::partition(image, sources, layout);
        let units = source_indices
            .iter()
            .map(|members| Unit {
                id: sources[members[0]].id,
                members: members.iter().map(|&index| sources[index].id).collect(),
            })
            .collect::<Vec<_>>();
        let candidates = source_indices
            .iter()
            .map(|members| {
                let mut candidate = sources[members[0]].clone();
                candidate.bounds = union(members.iter().map(|&index| sources[index].bounds));
                candidate.source_text.clear();
                candidate.source_alternatives.clear();
                candidate
            })
            .collect();
        Self {
            units,
            candidates,
            completed: vec![false; sources.len()],
            source_indices,
        }
    }

    /// Returns a unit ID only when every source member has finished reading.
    pub(super) fn complete(&mut self, index: usize, sources: &[DetectedTextRegion]) -> Option<u16> {
        self.completed[index] = true;
        let unit_index = self
            .source_indices
            .iter()
            .position(|members| members.contains(&index))?;
        let members = &self.source_indices[unit_index];
        if !members.iter().all(|&member| self.completed[member]) {
            return None;
        }
        let candidate = &mut self.candidates[unit_index];
        if members
            .iter()
            .all(|&member| !sources[member].source_text.trim().is_empty())
        {
            candidate.source_text = members
                .iter()
                .map(|&member| sources[member].source_text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            candidate.source_alternatives = vec![candidate.source_text.clone()];
        }
        Some(candidate.id)
    }
}

pub(super) fn union(bounds: impl Iterator<Item = NormalizedBounds>) -> NormalizedBounds {
    bounds.fold(
        NormalizedBounds {
            left: 1000,
            top: 1000,
            right: 0,
            bottom: 0,
        },
        |a, b| NormalizedBounds {
            left: a.left.min(b.left),
            top: a.top.min(b.top),
            right: a.right.max(b.right),
            bottom: a.bottom.max(b.bottom),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(id: u16, top: u16, right: u16) -> DetectedTextRegion {
        DetectedTextRegion {
            id,
            bounds: [top, 10, top + 20, right].into(),
            source_text: String::new(),
            source_alternatives: Vec::new(),
            recognition: Default::default(),
            appearance: None,
        }
    }
    #[test]
    fn paragraph_owns_its_short_tail_and_waits_only_for_its_members() {
        let image = image::RgbaImage::from_pixel(1000, 1000, image::Rgba([255; 4]));
        let mut sources = vec![
            source(1, 10, 510),
            source(2, 40, 490),
            source(3, 70, 70),
            source(4, 200, 400),
        ];
        let mut plan = Plan::new(&image, &mut sources, &[]);
        assert_eq!(
            plan.units
                .iter()
                .map(|u| u.members.clone())
                .collect::<Vec<_>>(),
            vec![vec![1, 2, 3], vec![4]]
        );
        for (i, text) in ["A complete", "sentence with a", "tail."]
            .into_iter()
            .enumerate()
        {
            sources[i].source_text = text.into();
            assert_eq!(plan.complete(i, &sources), (i == 2).then_some(1));
        }
        assert_eq!(
            plan.candidates[0].source_text,
            "A complete sentence with a tail."
        );
        assert_eq!(plan.candidates[0].bounds.bottom, 90);
    }
    #[test]
    fn unreadable_member_preserves_the_entire_unit_source() {
        let image = image::RgbaImage::from_pixel(1000, 1000, image::Rgba([255; 4]));
        let mut sources = vec![source(1, 10, 510), source(2, 40, 490)];
        let mut plan = Plan::new(&image, &mut sources, &[]);
        sources[0].source_text = "partial".into();
        assert_eq!(plan.complete(0, &sources), None);
        assert_eq!(plan.complete(1, &sources), Some(1));
        assert!(plan.candidates[0].source_text.is_empty());
    }
    #[test]
    fn different_ink_heights_do_not_split_equal_sized_wrapped_lines() {
        let mut image = image::RgbaImage::from_pixel(1000, 1000, image::Rgba([255; 4]));
        for (top, height, right) in [(12, 15, 500), (42, 10, 180)] {
            for x in (12..right).step_by(20) {
                for y in top..top + height {
                    image.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
                }
            }
        }
        let mut sources = vec![source(1, 10, 510), source(2, 40, 190)];
        let plan = Plan::new(&image, &mut sources, &[]);
        assert_eq!(plan.units[0].members, [1, 2]);
    }
    #[test]
    fn a_short_unit_cannot_jump_across_a_wider_intervening_line() {
        use sgt_screen_text_detector_protocol::stream::LayoutKind;
        let image = image::RgbaImage::from_pixel(1000, 1000, image::Rgba([255; 4]));
        let mut sources = vec![source(1, 10, 210), source(2, 32, 910), source(3, 54, 110)];
        let layout = [LayoutRegion {
            bounds: [0.0, 0.0, 1000.0, 100.0],
            kind: LayoutKind::Text,
        }];
        let plan = Plan::new(&image, &mut sources, &layout);
        assert_eq!(plan.units[0].members, [1]);
        assert_eq!(plan.units[1].members, [2, 3]);
    }
    #[test]
    fn separators_prevent_cross_row_ownership() {
        let mut image = image::RgbaImage::from_pixel(1000, 1000, image::Rgba([255; 4]));
        for x in 0..600 {
            image.put_pixel(x, 35, image::Rgba([0, 0, 0, 255]));
        }
        let mut sources = vec![source(1, 10, 510), source(2, 40, 490)];
        let plan = Plan::new(&image, &mut sources, &[]);
        assert_eq!(plan.units.len(), 2);
    }
    #[test]
    fn distinct_layout_regions_are_a_boundary_not_a_text_filter() {
        use sgt_screen_text_detector_protocol::stream::LayoutKind;
        let image = image::RgbaImage::from_pixel(1000, 1000, image::Rgba([255; 4]));
        let mut sources = vec![source(1, 10, 510), source(2, 40, 490), source(3, 300, 400)];
        let layout = vec![
            LayoutRegion {
                bounds: [0.0, 0.0, 600.0, 32.0],
                kind: LayoutKind::Text,
            },
            LayoutRegion {
                bounds: [0.0, 38.0, 600.0, 65.0],
                kind: LayoutKind::Text,
            },
        ];
        let plan = Plan::new(&image, &mut sources, &layout);
        assert_eq!(
            plan.units
                .iter()
                .flat_map(|u| u.members.iter())
                .copied()
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(plan.units.len(), 3);
    }
}
