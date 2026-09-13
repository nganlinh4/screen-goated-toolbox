//! One source partition shared by request ownership, readiness and rendering.
use super::contract::{DetectedTextRegion, NormalizedBounds};
use super::geometry::normalized_region;
use sgt_screen_text_detector_protocol::stream::LayoutRegion;

mod boundaries;
mod rows;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct Unit {
    pub id: u16,
    pub members: Vec<u16>,
}

impl Unit {
    pub(super) fn replacement_region(
        &self,
        layout: super::geometry::PixelRegion,
        footprint: &[super::geometry::PixelRegion],
        vertical: bool,
        wrap: bool,
    ) -> crate::overlay::result::SourceReplacementRegion {
        crate::overlay::result::SourceReplacementRegion {
            x: 0,
            y: 0,
            width: layout.width,
            height: layout.height,
            vertical,
            wrap,
            footprint: footprint
                .iter()
                .map(|r| {
                    [
                        r.x.saturating_sub(layout.x),
                        r.y.saturating_sub(layout.y),
                        r.width,
                        r.height,
                    ]
                })
                .collect(),
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
    #[test]
    fn replacement_preserves_line_footprint_without_filling_gaps() {
        use super::super::geometry::PixelRegion;
        let layout = PixelRegion {
            x: 100,
            y: 200,
            width: 300,
            height: 60,
        };
        let lines = [
            PixelRegion {
                height: 24,
                ..layout
            },
            PixelRegion {
                x: 110,
                y: 236,
                width: 150,
                height: 24,
            },
        ];
        let unit = super::Unit {
            id: 1,
            members: vec![1, 2],
        };
        let region = unit.replacement_region(layout, &lines, false, true);
        assert_eq!(region.footprint, vec![[0, 0, 300, 24], [10, 36, 150, 24]]);
        assert_eq!((region.width, region.height), (300, 60));
    }
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
    fn shared_centered_paragraph_keeps_short_opening_and_closing_lines() {
        use sgt_screen_text_detector_protocol::stream::LayoutKind;
        let image = image::RgbaImage::from_pixel(1000, 1000, image::Rgba([255; 4]));
        let mut sources = vec![source(1, 10, 120), source(2, 35, 180), source(3, 60, 130)];
        sources[0].bounds.left = 100;
        sources[1].bounds.left = 40;
        sources[2].bounds.left = 90;
        let layout = [LayoutRegion {
            bounds: [30.0, 0.0, 190.0, 90.0],
            kind: LayoutKind::Text,
        }];
        let plan = Plan::new(&image, &mut sources, &layout);
        assert_eq!(plan.units[0].members, [1, 2, 3]);
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

    #[test]
    fn cross_line_contrast_preserves_hierarchy_without_splitting_modest_ink_variation() {
        use sgt_screen_text_detector_protocol::stream::LayoutKind;
        for background in [0_u8, 255] {
            for (contrasts, expected) in [([110_u8, 230], 2), ([180, 225], 1)] {
                let mut image = image::RgbaImage::from_pixel(
                    1000,
                    1000,
                    image::Rgba([background, background, background, 255]),
                );
                for (top, contrast) in [12, 42].into_iter().zip(contrasts) {
                    let ink = background.abs_diff(contrast);
                    for x in (12..190).step_by(12) {
                        for dx in 0..4 {
                            for y in top..top + 14 {
                                image.put_pixel(x + dx, y, image::Rgba([ink, ink, ink, 255]));
                            }
                        }
                    }
                }
                let mut sources = vec![source(1, 10, 210), source(2, 40, 210)];
                let layout = [LayoutRegion {
                    bounds: [0.0, 0.0, 250.0, 80.0],
                    kind: LayoutKind::Text,
                }];
                assert_eq!(
                    Plan::new(&image, &mut sources, &layout).units.len(),
                    expected
                );
            }
        }
    }

    #[test]
    fn table_rows_never_join_into_a_paragraph() {
        use sgt_screen_text_detector_protocol::stream::LayoutKind;
        let image = image::RgbaImage::from_pixel(1000, 1000, image::Rgba([255; 4]));
        let mut sources = vec![source(1, 10, 510), source(2, 40, 490)];
        let layout = [LayoutRegion {
            bounds: [0.0, 0.0, 600.0, 100.0],
            kind: LayoutKind::Table,
        }];
        let plan = Plan::new(&image, &mut sources, &layout);
        assert_eq!(
            plan.units
                .iter()
                .map(|unit| unit.members.clone())
                .collect::<Vec<_>>(),
            [vec![1], vec![2]]
        );
    }

    #[test]
    fn borderless_grid_requires_a_continuous_surface_but_admits_thin_rules() {
        for stripe_width in [0, 2, 100] {
            let mut image = image::RgbaImage::from_pixel(1000, 1000, image::Rgba([255; 4]));
            for x in 90..90 + stripe_width {
                for y in 0..100 {
                    image.put_pixel(x, y, image::Rgba([40, 60, 80, 255]));
                }
            }
            let mut sources = Vec::new();
            for row in 0..3 {
                let mut a = source(row * 2 + 1, 10 + row * 30, 70);
                a.bounds.left = 20;
                let mut b = source(row * 2 + 2, 10 + row * 30, 250);
                b.bounds.left = 200;
                sources.extend([a, b]);
            }
            let plan = Plan::new(&image, &mut sources, &[]);
            if stripe_width <= 2 {
                assert_eq!(plan.units.len(), 6);
            } else {
                assert_eq!(
                    plan.units
                        .iter()
                        .map(|unit| unit.members.len())
                        .collect::<Vec<_>>(),
                    [3, 3]
                );
            }
        }
    }
}
