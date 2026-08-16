use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use image::ImageEncoder as _;
use serde::Deserialize;

use super::contract::{
    DetectedTextRegion, MemberJoin, NormalizedBounds, SemanticRole, TranslationDocument,
    TranslationRegion, TranslationSelection,
};
use crate::overlay::selection::CapturedRegion;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplayRecord {
    status: String,
    regions: Vec<ReplayRegion>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplayRegion {
    id: u16,
    normalized_box_2d: [u16; 4],
    ocr_candidates: Vec<String>,
    selected_source_text: Option<String>,
    translated_text: Option<String>,
    group_member_ids: Option<Vec<u16>>,
    member_joins: Option<Vec<MemberJoin>>,
    semantic_role: Option<SemanticRole>,
    visual_style: Option<super::appearance::VisualSignature>,
}

pub(super) fn start(value: serde_json::Value) {
    let run_directory = value
        .get("runDirectory")
        .and_then(|item| item.as_str())
        .map(PathBuf::from);
    let output = value
        .get("output")
        .and_then(|item| item.as_str())
        .map(PathBuf::from);
    let done = value
        .get("done")
        .and_then(|item| item.as_str())
        .map(PathBuf::from);
    std::thread::spawn(move || {
        let result = match (run_directory, output) {
            (Some(run_directory), Some(output)) => replay(&run_directory, &output),
            _ => Err(anyhow::anyhow!("replay request is incomplete")),
        };
        if let Some(done) = done.filter(|path| path.is_absolute()) {
            let value = match result {
                Ok(rendered) => serde_json::json!({"status": "complete", "rendered": rendered}),
                Err(error) => serde_json::json!({"status": "error", "error": format!("{error:#}")}),
            };
            let _ = std::fs::write(done, value.to_string());
        }
    });
}

fn replay(run_directory: &Path, output: &Path) -> Result<usize> {
    if !run_directory.is_absolute() || !output.is_absolute() {
        bail!("replay paths must be absolute");
    }
    let source_path = run_directory.join("source.jpg");
    let record_path = run_directory.join("run.json");
    let image = image::open(&source_path)
        .with_context(|| format!("open replay source {}", source_path.display()))?
        .to_rgba8();
    let record: ReplayRecord = serde_json::from_slice(&std::fs::read(&record_path)?)?;
    if record.status != "complete" {
        bail!("only a completed production run can be replayed");
    }
    let candidates = candidates(&record.regions)?;
    let document = document(&record.regions)?;
    let capture = CapturedRegion {
        width: image.width(),
        height: image.height(),
        image,
        left: 420,
        top: 160,
    };
    let selection = (capture.left, capture.top, capture.width, capture.height);
    let (job_id, _) = super::runtime::begin_job();
    let trace_id = format!("screen-translate-replay-{job_id}");
    crate::overlay::result::latency::begin(&trace_id);
    let (mut overlay, _) = super::render::start(job_id, capture, candidates, &trace_id)?;
    for region in &document.regions {
        overlay.send(region.clone());
    }
    let rendered = overlay.complete(document)?;
    let _ = crate::overlay::result::latency::wait_for_window_phase_count(
        &trace_id,
        "final_painted",
        rendered,
        std::time::Duration::from_secs(3),
    );
    let (image, _) = super::evidence_capture::capture_stable_selection(selection)?;
    let image = image::DynamicImage::ImageRgba8(image).to_rgb8();
    let file = std::fs::File::create(output)?;
    image::codecs::jpeg::JpegEncoder::new_with_quality(file, 88).write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(rendered)
}

fn candidates(regions: &[ReplayRegion]) -> Result<Arc<[DetectedTextRegion]>> {
    let candidates = regions
        .iter()
        .map(|region| {
            let source_alternatives = region.ocr_candidates.clone();
            let source_text = source_alternatives.first().cloned().unwrap_or_default();
            if source_text.is_empty() {
                bail!("replay candidate has no OCR text");
            }
            Ok(DetectedTextRegion {
                id: region.id,
                bounds: region.normalized_box_2d.into(),
                source_text,
                source_alternatives,
                appearance: region.visual_style,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Arc::from(candidates))
}

fn document(regions: &[ReplayRegion]) -> Result<TranslationDocument> {
    let by_id = regions
        .iter()
        .map(|region| (region.id, region))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut translated = Vec::new();
    for region in regions {
        let Some(member_ids) = region.group_member_ids.clone() else {
            continue;
        };
        if member_ids.is_empty() {
            bail!("replay group has no members");
        }
        if !seen.insert(member_ids.clone()) {
            continue;
        }
        let members = member_ids
            .iter()
            .map(|id| {
                by_id
                    .get(id)
                    .copied()
                    .context("replay group member is missing")
            })
            .collect::<Result<Vec<_>>>()?;
        let selections = members
            .iter()
            .map(|member| TranslationSelection {
                region_id: member.id,
                candidate_id: format!("r{}c0", member.id),
                source_text: member
                    .selected_source_text
                    .clone()
                    .unwrap_or_else(|| member.ocr_candidates[0].clone()),
                bounds: member.normalized_box_2d.into(),
            })
            .collect::<Vec<_>>();
        let translated_segments = members
            .iter()
            .map(|member| {
                member
                    .translated_text
                    .clone()
                    .context("replay translation is missing")
            })
            .collect::<Result<Vec<_>>>()?;
        let bounds = union_bounds(&selections);
        translated.push(TranslationRegion {
            id: member_ids[0],
            member_ids,
            member_joins: region.member_joins.clone().unwrap_or_default(),
            semantic_role: region.semantic_role.unwrap_or(SemanticRole::Standalone),
            source_text: selections
                .iter()
                .map(|selection| selection.source_text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            selections,
            translated_segments,
            bounds,
            background_color: None,
            text_color: None,
        });
    }
    Ok(TranslationDocument {
        regions: translated,
    })
}

fn union_bounds(selections: &[TranslationSelection]) -> NormalizedBounds {
    NormalizedBounds {
        left: selections
            .iter()
            .map(|item| item.bounds.left)
            .min()
            .unwrap_or(0),
        top: selections
            .iter()
            .map(|item| item.bounds.top)
            .min()
            .unwrap_or(0),
        right: selections
            .iter()
            .map(|item| item.bounds.right)
            .max()
            .unwrap_or(1),
        bottom: selections
            .iter()
            .map(|item| item.bounds.bottom)
            .max()
            .unwrap_or(1),
    }
}
