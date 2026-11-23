use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

#[path = "render_controller.rs"]
mod controller;
use super::render_mailbox::{self, Read, Receiver as CommandReceiver, Sender};
#[cfg(test)]
use controller::checked_controller;
use controller::{
    block_target_rect, close_source_overlay, create_controller_window, nearest_control_color,
    pump_messages, selection_anchor,
};

use super::contract::{DetectedTextRegion, TranslationDocument, TranslationRegion};
use super::render_scene::{PreparedBlock, PreparedScene};
use crate::overlay::selection::CapturedRegion;

const CONTROL_SCALE_PERCENT: u16 = 150;

struct LiveBlock {
    prepared: PreparedBlock,
    card: crate::overlay::result::scene_compositor::SourceCardHandle,
    rendered_segments: Option<Vec<String>>,
}

struct SegmentTranslation {
    source_text: String,
    translated_text: String,
}

struct TranslationControls {
    anchor: [i32; 4],
    color: String,
    opacity_percent: u8,
}

enum RenderCommand {
    Region(TranslationRegion),
    Complete(TranslationDocument),
    Move(i32, i32),
}

#[derive(Clone)]
pub(super) struct OverlayMotion(Sender<RenderCommand>);
impl OverlayMotion {
    pub(super) fn shift(&self, dx: i32, dy: i32) {
        let _ = self.0.send(RenderCommand::Move(dx, dy));
    }
}

/// Setup runs platform work (scene preparation, window creation, compositor
/// prewarm) that can stall; a job never waits on it past this bound.
const RENDERER_SETUP_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) struct TranslationOverlay {
    sender: Sender<RenderCommand>,
    completion: Receiver<Result<usize, String>>,
    ready: Receiver<Result<(), String>>,
    is_ready: bool,
    setup_deadline: Instant,
}

impl TranslationOverlay {
    pub(super) fn motion(&self) -> OverlayMotion {
        OverlayMotion(self.sender.clone())
    }
    pub(super) fn check_ready(&mut self) -> Result<()> {
        if !self.is_ready {
            match self.ready.try_recv() {
                Ok(Ok(())) => self.is_ready = true,
                Ok(Err(error)) => anyhow::bail!(error),
                // Dropping this overlay makes a late setup close its windows.
                Err(TryRecvError::Empty) if Instant::now() >= self.setup_deadline => {
                    anyhow::bail!(
                        "screen translation renderer setup did not finish within {} seconds",
                        RENDERER_SETUP_TIMEOUT.as_secs()
                    )
                }
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => {
                    anyhow::bail!("screen translation renderer stopped before setup")
                }
            }
        }
        match self.completion.try_recv() {
            Ok(Err(error)) => anyhow::bail!(error),
            Ok(Ok(_)) => anyhow::bail!("screen translation renderer completed unexpectedly"),
            Err(TryRecvError::Disconnected) => {
                anyhow::bail!("screen translation renderer stopped early")
            }
            Err(TryRecvError::Empty) => Ok(()),
        }
    }

    pub(super) fn wait_ready(&mut self, cancel: &AtomicBool) -> Result<()> {
        while !self.is_ready {
            if cancel.load(Ordering::Acquire) {
                anyhow::bail!("screen translation cancelled");
            }
            self.check_ready()?;
            if !self.is_ready {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        Ok(())
    }

    pub(super) fn send(&mut self, region: TranslationRegion) -> Result<()> {
        self.check_ready()?;
        if self.sender.send(RenderCommand::Region(region)).is_err() {
            if let Ok(Err(error)) = self.completion.try_recv() {
                anyhow::bail!(error);
            }
            anyhow::bail!("screen translation renderer stopped early");
        }
        Ok(())
    }

    pub(super) fn complete(self, document: TranslationDocument) -> Result<usize> {
        if self.sender.send(RenderCommand::Complete(document)).is_err() {
            if let Ok(Err(error)) = self.completion.try_recv() {
                return Err(anyhow::Error::msg(error));
            }
            anyhow::bail!("screen translation renderer stopped early");
        }
        self.completion
            .recv()
            .context("screen translation renderer stopped before completion")?
            .map_err(anyhow::Error::msg)
    }
}

pub(super) fn start(
    job_id: u64,
    capture: CapturedRegion,
    candidates: std::sync::Arc<[DetectedTextRegion]>,
    trace_id: &str,
    units: Option<std::sync::Arc<[super::units::Unit]>>,
    processing_id: Option<u64>,
) -> Result<TranslationOverlay> {
    let origin = (capture.left, capture.top);
    let (command_sender, command_receiver) = render_mailbox::channel()?;
    let (completion_sender, completion_receiver) = std::sync::mpsc::sync_channel(1);
    let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
    let trace_id = trace_id.to_string();
    std::thread::Builder::new()
        .name("sgt-screen-translate-overlay".to_string())
        .spawn(move || {
            run_overlay_thread(
                job_id,
                origin,
                capture,
                candidates,
                trace_id,
                command_receiver,
                completion_sender,
                ready_sender,
                units,
                processing_id,
            );
        })
        .context("screen translation overlay thread could not start")?;
    Ok(TranslationOverlay {
        sender: command_sender,
        completion: completion_receiver,
        ready: ready_receiver,
        is_ready: false,
        setup_deadline: Instant::now() + RENDERER_SETUP_TIMEOUT,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_overlay_thread(
    job_id: u64,
    origin: (i32, i32),
    capture: CapturedRegion,
    candidates: std::sync::Arc<[DetectedTextRegion]>,
    trace_id: String,
    mut receiver: CommandReceiver<RenderCommand>,
    completion: SyncSender<Result<usize, String>>,
    ready: SyncSender<Result<(), String>>,
    units: Option<std::sync::Arc<[super::units::Unit]>>,
    processing_id: Option<u64>,
) {
    let Some(_wake_registration) = super::runtime::register_renderer_wake(job_id, &receiver.wake)
    else {
        return;
    };
    let appearance_started = std::time::Instant::now();
    let mut candidates = candidates.to_vec();
    for candidate in &mut candidates {
        if candidate.appearance.is_some() {
            continue;
        }
        let pixels = candidate.bounds.pixels();
        candidate.appearance = super::appearance::analyze_region(&capture.image, pixels);
    }
    crate::log_info!(
        "[Screen Translate] trace={trace_id} foreground_analysis_ms={:.1}",
        appearance_started.elapsed().as_secs_f64() * 1000.0
    );
    let scene =
        match super::render_scene::prepare_scene(job_id, &capture, &candidates, units.as_deref()) {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = ready.send(Err(error.to_string()));
                let _ = completion.send(Err(error.to_string()));
                return;
            }
        };
    if !super::runtime::is_current(job_id) {
        return;
    }
    crate::overlay::result::latency::mark(&trace_id, "backdrops_ready");
    let controls = TranslationControls {
        anchor: selection_anchor(origin, (capture.width, capture.height)),
        color: nearest_control_color(scene.sources.values(), capture.width),
        opacity_percent: crate::APP
            .lock()
            .map(|app| app.config.screen_translate.overlay_opacity.clamp(10, 100))
            .unwrap_or(100),
    };
    let chain_id = format!("screen-translate-{job_id}");
    let controller = match create_controller_window(
        origin,
        &controls,
        &chain_id,
        &trace_id,
        !super::runtime::is_subtitle(job_id),
    ) {
        Ok(controller) => controller,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            let _ = completion.send(Err(error.to_string()));
            return;
        }
    };
    if let Some(id) = processing_id {
        crate::overlay::result::scene_compositor::ProcessingGlow::bind_controls(
            id,
            controller.0 as isize,
        );
    }
    let specs = scene
        .blocks
        .iter()
        .map(
            |prepared| crate::overlay::result::scene_compositor::SourceCardSpec {
                target_rect: block_target_rect(origin, prepared),
                image_rect: crate::overlay::result::scene_compositor::protocol::SceneRect {
                    x: prepared.layout.x as i32,
                    y: prepared.layout.y as i32,
                    width: prepared.layout.width as i32,
                    height: prepared.layout.height as i32,
                },
                backdrop_data_url: prepared.backdrop.clone(),
                foreground_color: prepared.foreground.clone(),
                preferred_font_size: prepared.preferred_font_size,
                source_vertical: prepared.vertical_text,
                source_regions: prepared.source_regions.clone(),
            },
        )
        .collect::<Vec<_>>();
    let (group, cards) = crate::overlay::result::scene_compositor::prewarm_source_group(
        controller,
        specs,
        controls.opacity_percent,
        &trace_id,
        Some(std::sync::Arc::new(capture.image)),
    );
    crate::overlay::result::scene_compositor::register_source_group_retirement_wake(
        group,
        std::sync::Arc::clone(&receiver.wake),
    );
    if ready.send(Ok(())).is_err() || !super::runtime::is_current(job_id) {
        close_source_overlay(group, controller);
        return;
    }
    let mut blocks = scene
        .blocks
        .iter()
        .cloned()
        .zip(cards)
        .map(|(prepared, card)| LiveBlock {
            prepared,
            card,
            rendered_segments: None,
        })
        .collect::<Vec<_>>();
    crate::overlay::result::latency::mark(&trace_id, "scene_prewarmed");
    let mut translations = HashMap::new();
    let mut completed = false;
    loop {
        pump_messages();
        if retire_closed_group(
            crate::overlay::result::scene_compositor::source_group_is_alive(group),
            || crate::overlay::result::scene_compositor::remove_source_group(group),
            || super::runtime::cancel_job(job_id),
        ) {
            break;
        }
        if !super::runtime::is_current(job_id) {
            close_source_overlay(group, controller);
            break;
        }
        match receiver.read() {
            Ok(Read::Item(RenderCommand::Move(dx, dy))) => {
                crate::overlay::result::scene_compositor::move_source_group(group, dx, dy);
            }
            Ok(Read::Item(RenderCommand::Region(region))) if !completed => {
                record_translations(region, &mut translations);
                super::runtime::publish_overlay(job_id, &chain_id, || {
                    refresh_blocks(&mut blocks, &translations, &scene, &trace_id, true)
                });
            }
            Ok(Read::Item(RenderCommand::Complete(document))) if !completed => {
                for region in document.regions {
                    record_translations(region, &mut translations);
                }
                super::runtime::publish_overlay(job_id, &chain_id, || {
                    refresh_blocks(&mut blocks, &translations, &scene, &trace_id, false)
                });
                let rendered = blocks
                    .iter()
                    .filter(|block| block.rendered_segments.is_some())
                    .count();
                crate::overlay::result::latency::expect_final_cells(&trace_id, rendered);
                let _ = completion.send(Ok(rendered));
                // An empty result releases its hidden controller, just like
                // an aborted translation.
                if rendered == 0 {
                    close_source_overlay(group, controller);
                    super::runtime::cancel_job(job_id);
                    return;
                }
                completed = true;
            }
            Ok(Read::Disconnected) if !completed => {
                close_source_overlay(group, controller);
                break;
            }
            Ok(_) => {}
            Err(error) => {
                fail_after_setup(
                    &error.to_string(),
                    |error| {
                        let _ = completion.send(Err(error.to_string()));
                    },
                    || close_source_overlay(group, controller),
                    |cause| super::runtime::fail_job(job_id, cause),
                );
                break;
            }
        }
    }
}

/// A renderer that fails after setup reports its cause, then ends the job as
/// failed (not user-cancelled): in-flight OCR and provider work stop, and the
/// job owner shows the cause.
fn fail_after_setup(
    error: &str,
    report: impl FnOnce(&str),
    close: impl FnOnce(),
    end_job: impl FnOnce(&str),
) {
    crate::log_info!("[Screen Translate] renderer failed after setup: {error}");
    report(error);
    close();
    end_job(error);
}

fn retire_closed_group(alive: bool, remove: impl FnOnce(), cancel: impl FnOnce()) -> bool {
    if alive {
        return false;
    }
    remove();
    cancel();
    true
}

#[allow(clippy::too_many_arguments)]
fn refresh_blocks(
    blocks: &mut [LiveBlock],
    translations: &HashMap<u16, SegmentTranslation>,
    scene: &PreparedScene,
    trace_id: &str,
    require_complete: bool,
) -> bool {
    let mut first_visible = false;
    for block in blocks.iter_mut() {
        let Some(segments) =
            component_translation(&block.prepared, scene, translations, require_complete)
        else {
            continue;
        };
        if block.rendered_segments.as_ref() == Some(&segments) {
            continue;
        }
        first_visible |= crate::overlay::result::scene_compositor::reveal_source_card(
            block.card,
            segments.clone(),
            trace_id,
        );
        block.rendered_segments = Some(segments);
    }
    first_visible
}

fn record_translations(
    region: TranslationRegion,
    translations: &mut HashMap<u16, SegmentTranslation>,
) {
    for ((member_id, selection), translated_text) in region
        .member_ids
        .into_iter()
        .zip(region.selections)
        .zip(region.translated_segments)
    {
        translations.insert(
            member_id,
            SegmentTranslation {
                source_text: selection.source_text,
                translated_text,
            },
        );
    }
}

fn component_translation(
    block: &PreparedBlock,
    scene: &PreparedScene,
    translations: &HashMap<u16, SegmentTranslation>,
    require_complete: bool,
) -> Option<Vec<String>> {
    let resolved = block
        .member_ids
        .iter()
        .filter(|id| translations.contains_key(id))
        .count();
    if resolved == 0 || (require_complete && resolved != block.member_ids.len()) {
        return None;
    }
    if block.member_ids.iter().any(|id| {
        !translations.contains_key(id)
            && scene
                .sources
                .get(id)
                .is_none_or(|source| source.source_text.is_empty())
    }) {
        return None;
    }
    let changed = block.member_ids.iter().any(|member_id| {
        translations.get(member_id).is_some_and(|translation| {
            should_render_segment(&translation.source_text, &translation.translated_text)
        })
    });
    if !changed {
        return None;
    }
    let segments = block
        .source_lane_member_ids
        .iter()
        .map(|member_ids| {
            member_ids
                .iter()
                .map(|member_id| {
                    translations
                        .get(member_id)
                        .map(|translation| translation.translated_text.as_str())
                        .or_else(|| {
                            scene
                                .sources
                                .get(member_id)
                                .map(|source| source.source_text.as_str())
                        })
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>();
    (!segments.iter().all(|text| text.trim().is_empty())).then_some(segments)
}

fn should_render_segment(source: &str, translated: &str) -> bool {
    !super::contract::text_is_source_equivalent(source, translated)
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
