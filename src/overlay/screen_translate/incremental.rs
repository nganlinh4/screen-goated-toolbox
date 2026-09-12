use super::contract::{
    DetectedTextRegion, NormalizedBounds, RecognitionEvidence, TranslationDocument,
    TranslationRegion,
};
use crate::overlay::selection::CapturedRegion;
use anyhow::{Context, Result, bail};
use image::ImageEncoder as _;
use sgt_screen_text_detector_protocol::{
    recognition::Reading,
    stream::{Event, Region},
};
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;
use windows::Win32::Foundation::RECT;

mod groups;

enum Message {
    Ocr(Event),
    OcrDone(Result<(), String>),
    Translated(TranslationRegion),
    BatchDone(Result<super::inference::TranslationOutcome, String>),
}

struct Batch {
    candidates: Vec<DetectedTextRegion>,
    scene: Vec<DetectedTextRegion>,
    prior_translations: Vec<TranslationRegion>,
}

pub(super) fn translate_region(
    job_id: u64,
    cancel: Arc<AtomicBool>,
    region: CapturedRegion,
) -> Result<()> {
    let trace_id = format!("screen-translate-{job_id}");
    crate::overlay::result::latency::begin(&trace_id);
    let config = crate::APP
        .lock()
        .map_err(|_| anyhow::anyhow!("configuration unavailable"))?
        .config
        .clone();
    let settings = config.screen_translate.clone();
    let model = settings.translation_model.clone();
    let processing = crate::overlay::result::scene_compositor::ProcessingGlow::show(RECT {
        left: region.left,
        top: region.top,
        right: region.left.saturating_add(i32::try_from(region.width)?),
        bottom: region.top.saturating_add(i32::try_from(region.height)?),
    })?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut png,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(
        region.image.as_raw(),
        region.width,
        region.height,
        image::ExtendedColorType::Rgba8,
    )?;
    crate::overlay::result::latency::mark(&trace_id, "capture_encoded");
    let width = region.width;
    let height = region.height;
    let mut evidence = Some(super::diagnostics::RunEvidence::begin(
        &trace_id,
        &region,
        &png,
        &settings.target_language,
        &model,
        &settings.translation_prompt,
    ));
    let no_text = crate::gui::locale::LocaleText::get(&config.ui_language)
        .screen_translate
        .screen_translate_no_text;
    crate::overlay::result::latency::mark(&trace_id, "ocr_dispatched");
    std::thread::scope(|scope| -> Result<()> {
        let work_cancel = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::sync_channel(32);
        let ocr_sender = sender.clone();
        let ocr_cancel = Arc::clone(&work_cancel);
        scope.spawn(move || {
            let result =
                super::detector::incremental::detect(&png, width, height, &ocr_cancel, |event| {
                    ocr_sender
                        .send(Message::Ocr(event))
                        .map_err(|_| anyhow::anyhow!("capture consumer closed"))
                });
            let _ = ocr_sender.send(Message::OcrDone(result.map_err(|e| e.to_string())));
        });
        let (requests, batches) = mpsc::sync_channel::<Batch>(1);
        let translate_cancel = Arc::clone(&work_cancel);
        let translate_trace = trace_id.clone();
        scope.spawn(move || {
            while let Ok(batch) = batches.recv() {
                if translate_cancel.load(Ordering::Acquire) {
                    break;
                }
                let result = super::inference::translate(
                    super::inference::TranslateInput {
                        trace_id: &translate_trace,
                        target_language: &settings.target_language,
                        translation_model: &model,
                        translation_prompt: &settings.translation_prompt,
                        candidates: &batch.candidates,
                        scene: &batch.scene,
                        prior_translations: &batch.prior_translations,
                    },
                    Arc::clone(&translate_cancel),
                    |region| {
                        let _ = sender.send(Message::Translated(region));
                    },
                );
                if sender
                    .send(Message::BatchDone(result.map_err(|e| e.to_string())))
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut capture = Some(region);
        let mut untranslated = Vec::new();
        let mut processing = super::processing::Progress::new(processing);
        let mut overlay = None;
        let mut candidates = Vec::new();
        let mut indices = HashMap::new();
        let mut groups = None;
        let mut plan: Option<super::units::Plan> = None;
        let mut ocr_done = false;
        let mut translating = false;
        let mut unresolved = 0;
        let mut first_reading = true;
        let mut first_translation = true;
        let mut queued = None;
        let mut drained = 0;
        let mut document = TranslationDocument {
            regions: Vec::new(),
        };
        let result = (|| -> Result<()> {
            loop {
                if cancel.load(Ordering::Acquire) || !super::runtime::is_current(job_id) {
                    bail!("screen translation cancelled");
                }
                match queued
                    .take()
                    .map(Ok)
                    .unwrap_or_else(|| receiver.recv_timeout(Duration::from_millis(20)))
                {
                    Ok(Message::Ocr(Event::Geometry {
                        regions, layout, ..
                    })) => {
                        crate::overlay::result::latency::mark(&trace_id, "geometry_received");
                        let capture = capture.take().context("duplicate capture geometry")?;
                        candidates = regions
                            .iter()
                            .map(|r| candidate(r, width, height))
                            .collect::<Result<Vec<_>>>()?;
                        processing.geometry(&candidates, width, height);
                        indices = candidates
                            .iter()
                            .enumerate()
                            .map(|(i, c)| (u32::from(c.id), i))
                            .collect();
                        let units =
                            super::units::Plan::new(&capture.image, &mut candidates, &layout);
                        processing.partition(&units.units);
                        evidence.as_ref().unwrap().units(&units.units, &layout);
                        groups = Some(groups::Groups::new(&units.candidates));
                        let render_units = Arc::from(units.units.clone());
                        crate::log_info!(
                            "[Screen Translate] trace={trace_id} source_regions={} translation_units={} layout_regions={}",
                            candidates.len(),
                            units.units.len(),
                            layout.len()
                        );
                        plan = Some(units);
                        if !candidates.is_empty() {
                            let renderer = super::render::start(
                                job_id,
                                capture,
                                Arc::from(candidates.clone()),
                                &trace_id,
                                Some(render_units),
                                Some(processing.id()),
                            )?;
                            overlay = Some(renderer);
                        }
                        crate::overlay::result::latency::mark(&trace_id, "geometry_ready");
                    }
                    Ok(Message::Ocr(Event::Reading { completion })) => {
                        if first_reading {
                            first_reading = false;
                            crate::overlay::result::latency::mark(&trace_id, "ocr_first_output");
                        }
                        let &index = indices
                            .get(&completion.region_id)
                            .context("unknown OCR region")?;
                        match completion.reading {
                            Reading::Text(text) => {
                                candidates[index].source_alternatives = vec![text.clone()];
                                candidates[index].source_text = text;
                            }
                            Reading::Unresolved(_) => unresolved += 1,
                        }
                        let groups = groups.as_mut().context("OCR preceded geometry")?;
                        let plan = plan.as_mut().context("OCR preceded unit partition")?;
                        if let Some(id) = plan.complete(index, &candidates) {
                            groups.completed(id);
                            if plan
                                .candidates
                                .iter()
                                .any(|c| c.id == id && c.source_text.is_empty())
                            {
                                processing.resolved(&[id]);
                            }
                        }
                        groups.observe_ready(&plan.candidates, std::time::Instant::now());
                    }
                    Ok(Message::Ocr(Event::Finished { .. })) => {}
                    Ok(Message::Ocr(_)) => bail!("unexpected capture event"),
                    Ok(Message::OcrDone(result)) => {
                        result.map_err(anyhow::Error::msg)?;
                        ocr_done = true;
                        crate::overlay::result::latency::mark(&trace_id, "detector_complete");
                        evidence.as_mut().unwrap().detected(&candidates, &[]);
                    }
                    Ok(Message::Translated(region)) => {
                        if first_translation {
                            first_translation = false;
                            crate::overlay::result::latency::mark(
                                &trace_id,
                                "provider_first_output",
                            );
                        }
                        if let Some(overlay) = overlay.as_mut() {
                            processing.resolved(&region.member_ids);
                            overlay.send(region);
                        }
                    }
                    Ok(Message::BatchDone(result)) => {
                        let translated = result.map_err(anyhow::Error::msg)?;
                        processing.resolved(&translated.unresolved);
                        untranslated.extend(translated.unresolved);
                        document.regions.extend(translated.document.regions);
                        translating = false;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        bail!("capture pipeline disconnected")
                    }
                }
                // Consume already-arrived completions before choosing a batch.
                // Cache hits can arrive as a burst; splitting that burst adds an
                // avoidable provider round trip. This never waits for new OCR.
                if !translating
                    && drained < super::contract::MAX_CANDIDATES
                    && let Ok(message) = receiver.try_recv()
                {
                    queued = Some(message);
                    drained += 1;
                    continue;
                }
                drained = 0;
                if !translating && let Some(groups) = groups.as_mut() {
                    let unit_candidates =
                        &plan.as_ref().context("unit partition missing")?.candidates;
                    let ready = groups.take_ready(unit_candidates, std::time::Instant::now());
                    if !ready.is_empty() {
                        crate::overlay::result::latency::mark(&trace_id, "translation_dispatched");
                        requests
                            .send(Batch {
                                candidates: ready,
                                scene: unit_candidates.clone(),
                                prior_translations: document.regions.clone(),
                            })
                            .context("translation worker stopped")?;
                        translating = true;
                    }
                }
                if ocr_done && !translating && groups.as_ref().is_some_and(groups::Groups::is_empty)
                {
                    break;
                }
            }
            crate::overlay::result::latency::mark(&trace_id, "translation_complete");
            let recorded = document.clone();
            let rendered = if let Some(overlay) = overlay.take() {
                overlay.complete(document)?
            } else {
                0
            };
            let mut warnings = Vec::new();
            if unresolved > 0 {
                warnings.push(format!(
                "{unresolved} detected region(s) could not be read; their units' original pixels were preserved"
            ));
            }
            if !untranslated.is_empty() {
                warnings.push(format!(
                "{} text unit(s) could not be translated; original pixels were preserved (units {:?})", untranslated.len(), untranslated
            ));
            }
            let warning = (!warnings.is_empty()).then(|| warnings.join("; "));
            if let Some(warning) = &warning {
                crate::log_info!("[Screen Translate] {warning}");
                if !untranslated.is_empty() {
                    crate::overlay::auto_copy_badge::show_notification(warning);
                }
            }
            if candidates.is_empty() {
                evidence.take().unwrap().no_text();
            } else {
                evidence
                    .take()
                    .unwrap()
                    .finish_with_warning(recorded, rendered, warning);
            }
            if candidates.is_empty() {
                crate::overlay::auto_copy_badge::show_notification(no_text);
            }
            crate::log_info!("[Screen Translate] ready regions={rendered}");
            Ok(())
        })();
        // Disconnect bounded queues before joining their producers on any exit.
        drop(requests);
        drop(receiver);
        if result.is_err() {
            work_cancel.store(true, Ordering::Release);
        } else {
            processing.finish();
        }
        if let Err(error) = &result
            && let Some(evidence) = evidence.take()
        {
            let mut evidence = evidence;
            evidence.detected(&candidates, &[]);
            evidence.fail("incremental_pipeline", error);
        }
        result
    })
}

fn candidate(region: &Region, width: u32, height: u32) -> Result<DetectedTextRegion> {
    let x = |value: f32| ((value / width as f32 * 1000.0).round() as u16).min(1000);
    let y = |value: f32| ((value / height as f32 * 1000.0).round() as u16).min(1000);
    let left = region.quad.iter().map(|p| x(p[0])).min().unwrap().min(999);
    let right = region
        .quad
        .iter()
        .map(|p| x(p[0]))
        .max()
        .unwrap()
        .max(left.saturating_add(1))
        .min(1000);
    let top = region.quad.iter().map(|p| y(p[1])).min().unwrap().min(999);
    let bottom = region
        .quad
        .iter()
        .map(|p| y(p[1]))
        .max()
        .unwrap()
        .max(top.saturating_add(1))
        .min(1000);
    Ok(DetectedTextRegion {
        id: region.id.try_into()?,
        bounds: NormalizedBounds {
            left,
            top,
            right,
            bottom,
        },
        source_text: String::new(),
        source_alternatives: Vec::new(),
        recognition: RecognitionEvidence {
            locator_confidence: region.confidence,
            ..Default::default()
        },
        appearance: None,
    })
}
