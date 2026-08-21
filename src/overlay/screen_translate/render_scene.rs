use std::collections::HashMap;

use anyhow::Result;

use super::backdrop::{encode_data_url, reconstruct_shaped_blob};
use super::contract::DetectedTextRegion;
use super::geometry::{PixelRegion, normalized_region};
use crate::overlay::selection::CapturedRegion;

pub(super) struct PreparedSource {
    pub pixels: PixelRegion,
    pub source_text: String,
    pub foreground: String,
    foreground_rgb: Option<[u8; 3]>,
    pub background: Option<([u8; 3], u8)>,
}

#[derive(Clone)]
pub(super) struct PreparedBlock {
    pub component_id: u16,
    pub member_ids: Vec<u16>,
    pub layout: PixelRegion,
    pub backdrop: String,
    pub foreground: String,
    pub preferred_font_size: f32,
    pub vertical_text: bool,
    pub source_regions: Vec<crate::overlay::result::SourceReplacementRegion>,
    pub source_lane_member_ids: Vec<Vec<u16>>,
}

pub(super) struct PreparedScene {
    pub sources: HashMap<u16, PreparedSource>,
    pub blocks: Vec<PreparedBlock>,
}

pub(super) fn prepare_scene(
    job_id: u64,
    capture: &CapturedRegion,
    candidates: &[DetectedTextRegion],
) -> Result<PreparedScene> {
    let located = candidates
        .iter()
        .map(|candidate| {
            (
                candidate,
                normalized_region(candidate.bounds, capture.width, capture.height),
            )
        })
        .collect::<Vec<_>>();
    let all_regions = located
        .iter()
        .map(|(_, region)| *region)
        .collect::<Vec<_>>();
    let mut sources = HashMap::with_capacity(located.len());
    for (candidate, pixels) in &located {
        let background = candidate
            .appearance
            .map(|appearance| (appearance.background_rgb, appearance.background_confidence));
        let foreground_rgb = candidate
            .appearance
            .filter(|appearance| appearance.foreground_confidence >= 3)
            .and_then(|appearance| appearance.foreground_rgb);
        let foreground = foreground_rgb
            .map(super::appearance::color_hex)
            .unwrap_or_default();
        sources.insert(
            candidate.id,
            PreparedSource {
                pixels: *pixels,
                source_text: candidate.source_text.clone(),
                foreground,
                foreground_rgb,
                background,
            },
        );
    }

    let mut blocks = Vec::new();
    for member_ids in connected_components(&located) {
        if !super::runtime::is_current(job_id) {
            break;
        }
        let members = member_ids
            .iter()
            .filter_map(|id| sources.get(id))
            .collect::<Vec<_>>();
        let layout = union(members.iter().map(|source| source.pixels));
        let shape_regions = members
            .iter()
            .map(|source| source.pixels)
            .collect::<Vec<_>>();
        let background = members
            .iter()
            .filter_map(|source| source.background)
            .max_by_key(|(_, confidence)| *confidence);
        let preferred_font_size = super::text_metrics::preferred_font_size(
            &capture.image,
            members
                .iter()
                .map(|source| (source.pixels, source.background)),
        );
        let vertical_text = dominant_orientation_is_vertical(&shape_regions);
        let source_lanes = source_lanes(&member_ids, &members, layout, vertical_text);
        let source_regions = source_lanes
            .iter()
            .map(|lane| lane.region.clone())
            .collect();
        let source_lane_member_ids = source_lanes
            .into_iter()
            .map(|lane| lane.member_ids)
            .collect();
        let (backdrop, inferred_foreground) = reconstruct_shaped_blob(
            &capture.image,
            layout,
            &all_regions,
            &shape_regions,
            background,
        );
        let reliable_background = background.filter(|(_, confidence)| {
            *confidence >= super::appearance::RELIABLE_BACKGROUND_PERCENT
        });
        let foreground = reliable_background
            .and_then(|(background, _)| most_contrasting_foreground(&members, background))
            .map(super::appearance::color_hex)
            .unwrap_or(inferred_foreground);
        let component_id = member_ids[0];
        blocks.push(PreparedBlock {
            component_id,
            member_ids,
            layout,
            backdrop: encode_data_url(&backdrop)?,
            foreground,
            preferred_font_size,
            vertical_text,
            source_regions,
            source_lane_member_ids,
        });
    }
    Ok(PreparedScene { sources, blocks })
}

struct SourceLane {
    member_ids: Vec<u16>,
    region: crate::overlay::result::SourceReplacementRegion,
}

#[derive(Clone, Copy)]
struct LaneMember {
    id: u16,
    pixels: PixelRegion,
}

struct LaneBand {
    members: Vec<LaneMember>,
    cross_start: u32,
    cross_end: u32,
    center_sum_twice: u64,
    minimum_cross_size: u32,
}

impl LaneBand {
    fn center_twice(&self) -> u32 {
        u32::try_from(self.center_sum_twice / self.members.len().max(1) as u64).unwrap_or(u32::MAX)
    }

    fn push(&mut self, member: LaneMember, vertical: bool) {
        let (start, size) = cross_axis(member.pixels, vertical);
        self.cross_start = self.cross_start.min(start);
        self.cross_end = self.cross_end.max(start.saturating_add(size));
        self.center_sum_twice = self
            .center_sum_twice
            .saturating_add(u64::from(center_twice(start, size)));
        self.minimum_cross_size = self.minimum_cross_size.min(size);
        self.members.push(member);
    }
}

fn source_lanes(
    member_ids: &[u16],
    sources: &[&PreparedSource],
    layout: PixelRegion,
    vertical: bool,
) -> Vec<SourceLane> {
    let mut members = member_ids
        .iter()
        .copied()
        .zip(sources.iter())
        .map(|(id, source)| LaneMember {
            id,
            pixels: source.pixels,
        })
        .collect::<Vec<_>>();
    members.sort_by_key(|member| {
        let (cross_start, cross_size) = cross_axis(member.pixels, vertical);
        let (along_start, _) = along_axis(member.pixels, vertical);
        (center_twice(cross_start, cross_size), along_start)
    });

    let mut bands: Vec<LaneBand> = Vec::new();
    for member in members {
        let (cross_start, cross_size) = cross_axis(member.pixels, vertical);
        let member_center = center_twice(cross_start, cross_size);
        let matching = bands
            .iter()
            .enumerate()
            .filter_map(|(index, band)| {
                let distance = member_center.abs_diff(band.center_twice());
                let tolerance = cross_size
                    .min(band.minimum_cross_size)
                    .saturating_div(3)
                    .max(2)
                    .saturating_mul(2);
                (distance <= tolerance).then_some((index, distance))
            })
            .min_by_key(|(_, distance)| *distance)
            .map(|(index, _)| index);
        if let Some(index) = matching {
            bands[index].push(member, vertical);
        } else {
            bands.push(LaneBand {
                members: vec![member],
                cross_start,
                cross_end: cross_start.saturating_add(cross_size),
                center_sum_twice: u64::from(member_center),
                minimum_cross_size: cross_size,
            });
        }
    }
    bands.sort_by_key(LaneBand::center_twice);
    for index in 0..bands.len().saturating_sub(1) {
        let (left, right) = bands.split_at_mut(index + 1);
        let before = &mut left[index];
        let after = &mut right[0];
        if before.cross_end <= after.cross_start {
            continue;
        }
        let boundary = before.center_twice().saturating_add(after.center_twice()) / 4;
        before.cross_end = before
            .cross_end
            .min(boundary.max(before.cross_start.saturating_add(1)));
        after.cross_start = after
            .cross_start
            .max(boundary.min(after.cross_end.saturating_sub(1)));
    }

    let mut lanes = Vec::new();
    for mut band in bands {
        band.members
            .sort_by_key(|member| along_axis(member.pixels, vertical).0);
        let mut run = Vec::new();
        let mut run_end = 0;
        for member in band.members {
            let (start, size) = along_axis(member.pixels, vertical);
            if !run.is_empty() && start > run_end {
                lanes.push(source_lane_from_run(
                    &run,
                    band.cross_start,
                    band.cross_end,
                    layout,
                    vertical,
                ));
                run.clear();
            }
            run_end = run_end.max(start.saturating_add(size));
            run.push(member);
        }
        if !run.is_empty() {
            lanes.push(source_lane_from_run(
                &run,
                band.cross_start,
                band.cross_end,
                layout,
                vertical,
            ));
        }
    }
    lanes.sort_by_key(|lane| {
        if vertical {
            (lane.region.x, lane.region.y)
        } else {
            (lane.region.y, lane.region.x)
        }
    });
    lanes
}

fn source_lane_from_run(
    run: &[LaneMember],
    cross_start: u32,
    cross_end: u32,
    layout: PixelRegion,
    vertical: bool,
) -> SourceLane {
    let along_start = run
        .iter()
        .map(|member| along_axis(member.pixels, vertical).0)
        .min()
        .unwrap_or(0);
    let along_end = run
        .iter()
        .map(|member| {
            let (start, size) = along_axis(member.pixels, vertical);
            start.saturating_add(size)
        })
        .max()
        .unwrap_or(along_start.saturating_add(1));
    let (x, y, width, height) = if vertical {
        (
            cross_start,
            along_start,
            cross_end.saturating_sub(cross_start).max(1),
            along_end.saturating_sub(along_start).max(1),
        )
    } else {
        (
            along_start,
            cross_start,
            along_end.saturating_sub(along_start).max(1),
            cross_end.saturating_sub(cross_start).max(1),
        )
    };
    SourceLane {
        member_ids: run.iter().map(|member| member.id).collect(),
        region: crate::overlay::result::SourceReplacementRegion {
            x: x.saturating_sub(layout.x),
            y: y.saturating_sub(layout.y),
            width,
            height,
            vertical,
        },
    }
}

fn cross_axis(region: PixelRegion, vertical: bool) -> (u32, u32) {
    if vertical {
        (region.x, region.width)
    } else {
        (region.y, region.height)
    }
}

fn along_axis(region: PixelRegion, vertical: bool) -> (u32, u32) {
    if vertical {
        (region.y, region.height)
    } else {
        (region.x, region.width)
    }
}

fn center_twice(start: u32, size: u32) -> u32 {
    start.saturating_mul(2).saturating_add(size)
}

fn most_contrasting_foreground(
    sources: &[&PreparedSource],
    background: [u8; 3],
) -> Option<[u8; 3]> {
    sources
        .iter()
        .filter_map(|source| source.foreground_rgb)
        .max_by_key(|foreground| luminance(*foreground).abs_diff(luminance(background)))
}

fn luminance(rgb: [u8; 3]) -> u32 {
    (299 * u32::from(rgb[0]) + 587 * u32::from(rgb[1]) + 114 * u32::from(rgb[2])) / 1000
}

fn connected_components(located: &[(&DetectedTextRegion, PixelRegion)]) -> Vec<Vec<u16>> {
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

fn touches(left: PixelRegion, right: PixelRegion) -> bool {
    left.x <= right.x.saturating_add(right.width)
        && right.x <= left.x.saturating_add(left.width)
        && left.y <= right.y.saturating_add(right.height)
        && right.y <= left.y.saturating_add(left.height)
}

fn dominant_orientation_is_vertical(regions: &[PixelRegion]) -> bool {
    let (vertical_area, horizontal_area) = regions.iter().fold((0_u64, 0_u64), |areas, region| {
        let area = u64::from(region.width) * u64::from(region.height);
        if region.height > region.width.saturating_mul(3) / 2 {
            (areas.0.saturating_add(area), areas.1)
        } else {
            (areas.0, areas.1.saturating_add(area))
        }
    });
    vertical_area > horizontal_area
}

fn union(regions: impl Iterator<Item = PixelRegion>) -> PixelRegion {
    let regions = regions.collect::<Vec<_>>();
    let left = regions.iter().map(|region| region.x).min().unwrap_or(0);
    let top = regions.iter().map(|region| region.y).min().unwrap_or(0);
    let right = regions
        .iter()
        .map(|region| region.x.saturating_add(region.width))
        .max()
        .unwrap_or(left + 1);
    let bottom = regions
        .iter()
        .map(|region| region.y.saturating_add(region.height))
        .max()
        .unwrap_or(top + 1);
    PixelRegion {
        x: left,
        y: top,
        width: right.saturating_sub(left).max(1),
        height: bottom.saturating_sub(top).max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepared(pixels: PixelRegion) -> PreparedSource {
        PreparedSource {
            pixels,
            source_text: String::new(),
            foreground: String::new(),
            foreground_rgb: None,
            background: None,
        }
    }

    #[test]
    fn touching_rectangles_share_a_component_without_changing_their_bounds() {
        let first = PixelRegion {
            x: 10,
            y: 10,
            width: 20,
            height: 10,
        };
        let second = PixelRegion {
            x: 30,
            y: 15,
            width: 15,
            height: 10,
        };
        let separate = PixelRegion {
            x: 46,
            y: 15,
            width: 10,
            height: 10,
        };
        assert!(touches(first, second));
        assert!(!touches(second, separate));
        assert_eq!(
            first,
            PixelRegion {
                x: 10,
                y: 10,
                width: 20,
                height: 10
            }
        );
    }

    #[test]
    fn dominant_area_keeps_a_mixed_component_vertical() {
        let regions = [
            PixelRegion {
                x: 0,
                y: 0,
                width: 40,
                height: 240,
            },
            PixelRegion {
                x: 45,
                y: 210,
                width: 80,
                height: 20,
            },
        ];
        assert!(dominant_orientation_is_vertical(&regions));
    }

    #[test]
    fn foreground_contrast_is_measured_against_the_merged_background() {
        let background = [109, 7, 18];
        assert!(
            luminance([235, 210, 170]).abs_diff(luminance(background))
                > luminance([147, 16, 42]).abs_diff(luminance(background))
        );
    }

    #[test]
    fn overlapping_detector_rows_become_non_overlapping_text_lanes() {
        let sources = [
            prepared(PixelRegion {
                x: 10,
                y: 10,
                width: 40,
                height: 20,
            }),
            prepared(PixelRegion {
                x: 50,
                y: 10,
                width: 30,
                height: 20,
            }),
            prepared(PixelRegion {
                x: 10,
                y: 25,
                width: 70,
                height: 20,
            }),
        ];
        let references = sources.iter().collect::<Vec<_>>();
        let lanes = source_lanes(
            &[1, 2, 3],
            &references,
            PixelRegion {
                x: 10,
                y: 10,
                width: 70,
                height: 35,
            },
            false,
        );

        assert_eq!(lanes.len(), 2);
        assert_eq!(lanes[0].member_ids, [1, 2]);
        assert_eq!(lanes[1].member_ids, [3]);
        assert!(
            lanes[0].region.y + lanes[0].region.height <= lanes[1].region.y,
            "lane rectangles must not paint over each other"
        );
    }

    #[test]
    fn gaps_in_one_row_remain_distinct_text_lanes() {
        let sources = [
            prepared(PixelRegion {
                x: 0,
                y: 0,
                width: 20,
                height: 10,
            }),
            prepared(PixelRegion {
                x: 30,
                y: 0,
                width: 20,
                height: 10,
            }),
        ];
        let references = sources.iter().collect::<Vec<_>>();
        let lanes = source_lanes(
            &[1, 2],
            &references,
            PixelRegion {
                x: 0,
                y: 0,
                width: 50,
                height: 10,
            },
            false,
        );

        assert_eq!(lanes.len(), 2);
        assert_eq!(lanes[0].member_ids, [1]);
        assert_eq!(lanes[1].member_ids, [2]);
    }
}
