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
        let source_regions = members
            .iter()
            .map(|source| crate::overlay::result::SourceReplacementRegion {
                x: source.pixels.x.saturating_sub(layout.x),
                y: source.pixels.y.saturating_sub(layout.y),
                width: source.pixels.width,
                height: source.pixels.height,
                vertical: source.pixels.height > source.pixels.width.saturating_mul(3) / 2,
            })
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
        });
    }
    Ok(PreparedScene { sources, blocks })
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
}
