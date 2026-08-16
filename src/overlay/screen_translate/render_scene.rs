use std::collections::HashMap;

use anyhow::{Context, Result};

use super::backdrop::{encode_data_url, reconstruct_blob_image_with_background};
use super::contract::DetectedTextRegion;
use super::geometry::{MIN_READABLE_HEIGHT, MIN_READABLE_WIDTH, PixelRegion, normalized_region};
use crate::overlay::selection::CapturedRegion;

pub(super) struct PreparedSource {
    pub pixels: PixelRegion,
    pub foreground: String,
    pub background: Option<([u8; 3], u8)>,
}

pub(super) struct PreparedBlock {
    pub member_ids: Vec<u16>,
    pub layout: PixelRegion,
    pub backdrop: String,
    pub foreground: String,
    pub preferred_font_size: f32,
}

pub(super) struct PreparedScene {
    pub sources: HashMap<u16, PreparedSource>,
    image: image::RgbaImage,
    masks: Vec<PixelRegion>,
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
                candidate.id,
                normalized_region(candidate.bounds, capture.width, capture.height),
                candidate.appearance,
            )
        })
        .filter(|(_, region, _)| {
            region.width >= MIN_READABLE_WIDTH && region.height >= MIN_READABLE_HEIGHT
        })
        .collect::<Vec<_>>();
    let masks = located
        .iter()
        .map(|(_, region, _)| *region)
        .collect::<Vec<_>>();
    let mut sources = HashMap::with_capacity(located.len());
    for (id, pixels, appearance) in located {
        if !super::runtime::is_current(job_id) {
            break;
        }
        let background = appearance
            .map(|appearance| (appearance.background_rgb, appearance.background_confidence));
        let foreground = appearance
            .filter(|appearance| appearance.foreground_confidence >= 3)
            .and_then(|appearance| appearance.foreground_rgb)
            .map(super::appearance::color_hex)
            .unwrap_or_default();
        sources.insert(
            id,
            PreparedSource {
                pixels,
                foreground,
                background,
            },
        );
    }
    Ok(PreparedScene {
        sources,
        image: capture.image.clone(),
        masks,
    })
}

pub(super) fn prepare_block(
    member_ids: &[u16],
    translated_text: &str,
    scene: &PreparedScene,
) -> Result<PreparedBlock> {
    let members = member_ids
        .iter()
        .map(|id| {
            scene
                .sources
                .get(id)
                .context("translation block member was not prepared")
        })
        .collect::<Result<Vec<_>>>()?;
    let left = members.iter().map(|source| source.pixels.x).min().unwrap();
    let top = members.iter().map(|source| source.pixels.y).min().unwrap();
    let right = members
        .iter()
        .map(|source| source.pixels.x.saturating_add(source.pixels.width))
        .max()
        .unwrap();
    let bottom = members
        .iter()
        .map(|source| source.pixels.y.saturating_add(source.pixels.height))
        .max()
        .unwrap();
    let source_layout = PixelRegion {
        x: left,
        y: top,
        width: right.saturating_sub(left).max(1),
        height: bottom.saturating_sub(top).max(1),
    };
    let background = members
        .iter()
        .filter_map(|source| source.background)
        .max_by_key(|(_, confidence)| *confidence);
    let preferred_font_size = super::render_expansion::preferred_font_size(
        &scene.image,
        members
            .iter()
            .map(|source| (source.pixels, source.background)),
    );
    let member_regions = members
        .iter()
        .map(|source| source.pixels)
        .collect::<Vec<_>>();
    let layout = super::render_expansion::expand_vertical_surface(
        &scene.image,
        source_layout,
        &member_regions,
        &scene.masks,
        background,
        translated_text,
        preferred_font_size,
    );
    let (backdrop, inferred_foreground) =
        reconstruct_blob_image_with_background(&scene.image, layout, &scene.masks, background);
    let foreground = members
        .iter()
        .find_map(|source| (!source.foreground.is_empty()).then(|| source.foreground.clone()))
        .unwrap_or(inferred_foreground);
    Ok(PreparedBlock {
        member_ids: member_ids.to_vec(),
        layout,
        backdrop: encode_data_url(&backdrop)?,
        foreground,
        preferred_font_size,
    })
}
