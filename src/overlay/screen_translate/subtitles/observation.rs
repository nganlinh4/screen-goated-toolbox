//! The ordinary OCR events and unit partition, retained with their exact source frame.
use super::super::{contract::DetectedTextRegion, detector, incremental, units};
use anyhow::{Context, Result};
use image::ImageEncoder as _;
use sgt_screen_text_detector_protocol::{recognition::Reading, stream::Event};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

#[derive(Clone)]
pub(super) struct Observation {
    pub events: Vec<Event>,
}
#[derive(Clone)]
pub(super) struct Group {
    pub candidate: DetectedTextRegion,
    pub sources: Arc<[DetectedTextRegion]>,
    pub unit: units::Unit,
}
pub(super) struct Frame {
    pub origin: (i32, i32),
    pub image: Arc<image::RgbaImage>,
    pub groups: Vec<Group>,
    pub uncertain: Vec<super::super::contract::NormalizedBounds>,
    pub observed_at: Instant,
}

pub(super) fn read(image: &image::RgbaImage, cancel: &AtomicBool) -> Result<Observation> {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut png,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    let mut events = Vec::new();
    detector::incremental::detect(&png, image.width(), image.height(), cancel, |event| {
        events.push(event);
        Ok(())
    })?;
    Ok(Observation { events })
}

pub(super) fn prepare(
    image: Arc<image::RgbaImage>,
    observation: &Observation,
    observed_at: Instant,
    origin: (i32, i32),
) -> Result<Arc<Frame>> {
    let (regions, layout) = observation
        .events
        .iter()
        .find_map(|event| match event {
            Event::Geometry {
                regions, layout, ..
            } => Some((regions, layout)),
            _ => None,
        })
        .context("OCR geometry missing")?;
    let mut sources = regions
        .iter()
        .map(|r| incremental::candidate(r, image.width(), image.height()))
        .collect::<Result<Vec<_>>>()?;
    let mut plan = units::Plan::new(&image, &mut sources, layout);
    for event in &observation.events {
        if let Event::Reading { completion } = event {
            let index = sources
                .iter()
                .position(|s| u32::from(s.id) == completion.region_id)
                .context("unknown OCR member")?;
            if let Reading::Text(text) = &completion.reading {
                sources[index].source_text = text.clone();
                sources[index].source_alternatives = vec![text.clone()];
            }
            plan.complete(index, &sources);
        }
    }
    let mut groups = Vec::new();
    let mut uncertain = Vec::new();
    for (mut candidate, unit) in plan.candidates.into_iter().zip(plan.units) {
        if candidate.source_text.is_empty() {
            uncertain.push(candidate.bounds);
            continue;
        }
        candidate.source_text = candidate
            .source_text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        candidate.source_alternatives = vec![candidate.source_text.clone()];
        let members = sources
            .iter()
            .filter(|s| unit.members.contains(&s.id))
            .cloned()
            .collect::<Vec<_>>();
        groups.push(Group {
            candidate,
            sources: Arc::from(members),
            unit,
        });
    }
    Ok(Arc::new(Frame {
        origin,
        image,
        groups,
        uncertain,
        observed_at,
    }))
}
