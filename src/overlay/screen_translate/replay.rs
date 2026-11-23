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
    selection: Option<ReplaySelection>,
}

#[derive(Deserialize)]
struct ReplaySelection {
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct ReplayUnits {
    units: Vec<super::units::Unit>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplayRegion {
    id: u16,
    normalized_box_2d: [u16; 4],
    pixel_box: Option<[u32; 4]>,
    ocr_candidates: Vec<String>,
    selected_source_text: Option<String>,
    translated_text: Option<String>,
    group_member_ids: Option<Vec<u16>>,
    member_joins: Option<Vec<MemberJoin>>,
    semantic_role: Option<SemanticRole>,
    visual_style: Option<super::appearance::VisualSignature>,
}

pub(super) fn start(value: serde_json::Value, admission: super::LabSnapshot) {
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
        let owner = std::cell::Cell::new(None);
        let result = match (run_directory, output) {
            (Some(run_directory), Some(output)) => {
                replay(&run_directory, &output, done.as_deref(), admission, &owner)
            }
            _ => Err(anyhow::anyhow!("replay request is incomplete")),
        };
        if let (Err(error), Some(done)) = (result, done.filter(|path| path.is_absolute())) {
            crate::log_info!("[Screen Translate] replay failed: {error:#}");
            let value = serde_json::json!({"status": "error", "error": format!("{error:#}")});
            if let Ok(mut staged) =
                super::lab_output::StagedFile::new(&done, value.to_string().as_bytes())
            {
                if let Some((job_id, capture, generation)) = owner.get() {
                    let _ = super::subtitles::with_lab_generation(generation, || {
                        if let Some(claim) = super::claim_current_capture(capture) {
                            let _ =
                                super::runtime::with_current_primary(job_id, || staged.publish());
                            drop(claim);
                        }
                    });
                } else {
                    let _ = super::subtitles::admit_lab_effect(admission.subtitle, || {
                        let _claim = super::claim_lab_capture(admission)?;
                        super::runtime::with_lab_activity(admission.jobs, || staged.publish())
                    });
                }
            }
        }
    });
}

fn replay(
    run_directory: &Path,
    output: &Path,
    done: Option<&Path>,
    admission: super::LabSnapshot,
    owner: &std::cell::Cell<Option<(u64, u64, u64)>>,
) -> Result<usize> {
    if !run_directory.is_absolute() || !output.is_absolute() {
        bail!("replay paths must be absolute");
    }
    let source_path = ["source.png", "source.jpg"]
        .into_iter()
        .map(|name| run_directory.join(name))
        .find(|path| path.is_file())
        .context("replay source is missing")?;
    let record_path = run_directory.join("run.json");
    let image = image::open(&source_path)
        .with_context(|| format!("open replay source {}", source_path.display()))?
        .to_rgba8();
    let record: ReplayRecord = serde_json::from_slice(&std::fs::read(&record_path)?)?;
    if !matches!(record.status.as_str(), "complete" | "partial") {
        bail!("only a finalized production run can be replayed");
    }
    let recorded_size = record
        .selection
        .map(|selection| (selection.width, selection.height))
        .unwrap_or(image.dimensions());
    let candidates = candidates(&record.regions, recorded_size, image.dimensions())?;
    let document = document(&record.regions)?;
    let units_path = run_directory.join("units.json");
    let units = if units_path.exists() {
        let record: ReplayUnits = serde_json::from_slice(&std::fs::read(units_path)?)?;
        Some(Arc::from(record.units))
    } else {
        None
    };
    let (left, top) = super::capture::image_origin(image.width(), image.height());
    let capture = CapturedRegion {
        width: image.width(),
        height: image.height(),
        image,
        left,
        top,
    };
    let selection = (capture.left, capture.top, capture.width, capture.height);
    if !crate::overlay::result::scene_compositor::wait_until_ready(std::time::Duration::from_secs(
        15,
    )) {
        bail!("replay renderer did not become ready");
    }
    let (job_id, cancel, generation) =
        super::admit_lab_job(admission).context("replay request was superseded")?;
    crate::overlay::set_is_busy(false);
    let generation = generation.context("diagnostic request identity exhausted")?;
    owner.set(Some((job_id, admission.capture, generation)));
    let trace_id = format!("screen-translate-replay-{job_id}");
    crate::overlay::result::latency::begin(&trace_id);
    // Replayed translations are already available; paint attribution must
    // still wait for their cards rather than timing out silently.
    crate::overlay::result::latency::mark(&trace_id, "provider_first_output");
    let mut overlay = super::render::start(job_id, capture, candidates, &trace_id, units, None)?;
    for region in &document.regions {
        overlay.send(region.clone())?;
    }
    let rendered = overlay.complete(document)?;
    anyhow::ensure!(
        super::runtime::is_current(job_id) && !cancel.load(std::sync::atomic::Ordering::SeqCst),
        "replay request was superseded"
    );
    let painted = crate::overlay::result::latency::wait_for_window_phase_count(
        &trace_id,
        "final_painted",
        rendered,
        std::time::Duration::from_secs(3),
    );
    if painted < rendered {
        bail!("replay painted {painted} of {rendered} result cells");
    }
    anyhow::ensure!(
        super::runtime::is_current(job_id) && !cancel.load(std::sync::atomic::Ordering::SeqCst),
        "replay request was superseded"
    );
    let (image, _) = super::evidence_capture::capture_stable_selection(selection)?;
    let image = image::DynamicImage::ImageRgba8(image).to_rgb8();
    let mut encoded = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 88).write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    let mut staged_output = super::lab_output::StagedFile::new(output, &encoded)?;
    let mut staged_done = done
        .filter(|path| path.is_absolute())
        .map(|done| {
            let value = serde_json::json!({"status": "complete", "rendered": rendered});
            super::lab_output::StagedFile::new(done, value.to_string().as_bytes())
        })
        .transpose()?;
    super::subtitles::with_lab_generation(generation, || -> Result<()> {
        let _claim = super::claim_current_capture(admission.capture)
            .context("replay request was superseded")?;
        super::runtime::with_current_primary(job_id, || -> Result<()> {
            staged_output.publish()?;
            if let Some(done) = &mut staged_done {
                done.publish().context(
                    "replay image was published but the completion marker could not be published",
                )?;
            }
            Ok(())
        })
        .context("replay request was superseded")??;
        Ok(())
    })
    .context("replay request was superseded")??;
    Ok(rendered)
}

fn candidates(
    regions: &[ReplayRegion],
    recorded_size: (u32, u32),
    image_size: (u32, u32),
) -> Result<Arc<[DetectedTextRegion]>> {
    let candidates = regions
        .iter()
        .map(|region| {
            let source_alternatives = region.ocr_candidates.clone();
            let source_text = source_alternatives.first().cloned().unwrap_or_default();
            Ok(DetectedTextRegion {
                id: region.id,
                bounds: if let Some([x, y, width_pixels, height_pixels]) = region.pixel_box {
                    super::source_bounds::SourceBounds::from_pixels(
                        super::geometry::PixelRegion {
                            x,
                            y,
                            width: width_pixels,
                            height: height_pixels,
                        },
                        recorded_size.0,
                        recorded_size.1,
                    )?
                } else {
                    super::source_bounds::SourceBounds::from_normalized(
                        region.normalized_box_2d.into(),
                        image_size.0,
                        image_size.1,
                    )?
                }
                .rescale(image_size.0, image_size.1)?,
                source_text,
                source_alternatives,
                recognition: Default::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_uses_recorded_pixels_and_rescales_the_recorded_selection_to_its_image() {
        let region: ReplayRegion = serde_json::from_value(serde_json::json!({
            "id": 1, "normalizedBox2d": [125, 131, 208, 143],
            "pixelBox": [1003, 30, 98, 20], "ocrCandidates": ["sample"],
        }))
        .unwrap();
        let result = candidates(&[region], (7680, 240), (3840, 120)).unwrap();
        assert_eq!(
            result[0].bounds.pixels(),
            super::super::geometry::PixelRegion {
                x: 501,
                y: 15,
                width: 50,
                height: 10
            }
        );
    }

    #[test]
    fn normalized_only_replay_input_is_imported_at_the_available_image_size() {
        let region: ReplayRegion = serde_json::from_value(serde_json::json!({
            "id": 1, "normalizedBox2d": [100, 100, 200, 200], "ocrCandidates": ["sample"],
        }))
        .unwrap();
        let result = candidates(&[region], (8000, 4000), (800, 400)).unwrap();
        assert_eq!(
            result[0].bounds.pixels(),
            super::super::geometry::PixelRegion {
                x: 80,
                y: 40,
                width: 80,
                height: 40
            }
        );
    }
}
