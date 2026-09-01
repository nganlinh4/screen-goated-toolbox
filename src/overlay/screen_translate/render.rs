use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender};
use std::time::Duration;

use anyhow::{Context, Result};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetSystemMetrics, MSG, PM_REMOVE, PeekMessageW, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, TranslateMessage, WM_CLOSE,
};

use super::contract::{DetectedTextRegion, TranslationDocument, TranslationRegion};
use super::render_scene::{PreparedBlock, PreparedScene, PreparedSource};
use crate::overlay::result::{RefineContext, ResultControlOptions, ResultWindowParams, WindowType};
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
}

pub(super) struct TranslationOverlay {
    sender: Sender<RenderCommand>,
    completion: Receiver<Result<usize, String>>,
}

impl TranslationOverlay {
    pub(super) fn send(&mut self, region: TranslationRegion) {
        let _ = self.sender.send(RenderCommand::Region(region));
    }

    pub(super) fn complete(self, document: TranslationDocument) -> Result<usize> {
        self.sender
            .send(RenderCommand::Complete(document))
            .context("screen translation renderer stopped early")?;
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
) -> Result<(TranslationOverlay, Receiver<()>)> {
    let origin = (capture.left, capture.top);
    let (command_sender, command_receiver) = std::sync::mpsc::channel();
    let (visible_sender, visible_receiver) = std::sync::mpsc::sync_channel(1);
    let (completion_sender, completion_receiver) = std::sync::mpsc::sync_channel(1);
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
                visible_sender,
                completion_sender,
            );
        })
        .context("screen translation overlay thread could not start")?;
    Ok((
        TranslationOverlay {
            sender: command_sender,
            completion: completion_receiver,
        },
        visible_receiver,
    ))
}

#[allow(clippy::too_many_arguments)]
fn run_overlay_thread(
    job_id: u64,
    origin: (i32, i32),
    capture: CapturedRegion,
    candidates: std::sync::Arc<[DetectedTextRegion]>,
    trace_id: String,
    receiver: Receiver<RenderCommand>,
    first_visible: SyncSender<()>,
    completion: SyncSender<Result<usize, String>>,
) {
    let scene = match super::render_scene::prepare_scene(job_id, &capture, &candidates) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = completion.send(Err(error.to_string()));
            return;
        }
    };
    crate::overlay::result::latency::mark(&trace_id, "backdrops_ready");
    let virtual_origin = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        )
    };
    let controls = TranslationControls {
        anchor: relative_selection_anchor(origin, (capture.width, capture.height), virtual_origin),
        color: nearest_control_color(scene.sources.values(), capture.width),
        opacity_percent: crate::APP
            .lock()
            .map(|app| app.config.screen_translate.overlay_opacity.clamp(10, 100))
            .unwrap_or(100),
    };
    let chain_id = format!("screen-translate-{job_id}");
    let controller = create_controller_window(origin, &controls, &chain_id, &trace_id);
    let specs = scene
        .blocks
        .iter()
        .map(
            |prepared| crate::overlay::result::scene_compositor::SourceCardSpec {
                target_rect: block_target_rect(origin, prepared),
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
    );
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
    let mut had_visible = false;
    let mut first_visible = Some(first_visible);
    loop {
        pump_messages();
        if !crate::overlay::result::scene_compositor::source_group_is_alive(group) {
            crate::overlay::result::scene_compositor::remove_source_group(group);
            if had_visible {
                super::runtime::cancel_active();
            }
            break;
        }
        if !super::runtime::is_current(job_id) {
            close_source_overlay(group, controller);
            break;
        }
        match receiver.recv_timeout(Duration::from_millis(8)) {
            Ok(RenderCommand::Region(region)) => {
                record_translations(region, &mut translations);
                if refresh_blocks(&mut blocks, &translations, &scene, &trace_id, true) {
                    had_visible = true;
                    super::runtime::register_overlay(job_id, chain_id.clone());
                    if let Some(sender) = first_visible.take() {
                        let _ = sender.send(());
                    }
                }
            }
            Ok(RenderCommand::Complete(document)) => {
                for region in document.regions {
                    record_translations(region, &mut translations);
                }
                if refresh_blocks(&mut blocks, &translations, &scene, &trace_id, false) {
                    super::runtime::register_overlay(job_id, chain_id.clone());
                    if let Some(sender) = first_visible.take() {
                        let _ = sender.send(());
                    }
                }
                let rendered = blocks
                    .iter()
                    .filter(|block| block.rendered_segments.is_some())
                    .count();
                let _ = completion.send(Ok(rendered));
                while crate::overlay::result::scene_compositor::source_group_is_alive(group) {
                    pump_messages();
                    std::thread::sleep(Duration::from_millis(8));
                }
                crate::overlay::result::scene_compositor::remove_source_group(group);
                return;
            }
            Err(RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(8));
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
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
        let resolved = block
            .prepared
            .member_ids
            .iter()
            .filter(|member_id| translations.contains_key(member_id))
            .count();
        if resolved == 0 || (require_complete && resolved != block.prepared.member_ids.len()) {
            continue;
        }
        let Some(segments) = component_translation(&block.prepared, scene, translations) else {
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
) -> Option<Vec<String>> {
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

fn create_controller_window(
    origin: (i32, i32),
    controls: &TranslationControls,
    chain_id: &str,
    trace_id: &str,
) -> HWND {
    let hwnd = crate::overlay::result::create_deferred_result_window_shell(
        ResultWindowParams {
            target_rect: RECT {
                left: origin.0,
                top: origin.1,
                right: origin.0.saturating_add(1),
                bottom: origin.1.saturating_add(1),
            },
            win_type: WindowType::Primary,
            context: RefineContext::None,
            model_id: String::new(),
            provider: String::new(),
            streaming_enabled: false,
            start_editing: false,
            preset_prompt: String::new(),
            custom_bg_color: 0,
            initial_text: String::new(),
            preset_id: None,
            is_chain_root: false,
            latency_trace_id: Some(trace_id.to_string()),
        },
        chain_id.to_string(),
    );
    crate::overlay::result::configure_deferred_text_only_result_window(
        hwnd,
        crate::overlay::result::TextOnlyResultOptions {
            backdrop_data_url: String::new(),
            foreground_color: controls.color.clone(),
            chain_id: chain_id.to_string(),
            control_options: Some(ResultControlOptions {
                anchor_rect: Some(controls.anchor),
                control_color: Some(controls.color.clone()),
                scale_percent: CONTROL_SCALE_PERCENT,
                group_actions: true,
                edit_enabled: false,
            }),
            preferred_font_size: None,
            source_vertical: false,
            source_regions: Vec::new(),
            source_segments: Vec::new(),
            opacity_percent: Some(controls.opacity_percent),
        },
        true,
    );
    hwnd
}

fn block_target_rect(origin: (i32, i32), group: &PreparedBlock) -> RECT {
    let pixels = group.layout;
    RECT {
        left: origin.0 + pixels.x as i32,
        top: origin.1 + pixels.y as i32,
        right: origin.0 + (pixels.x + pixels.width) as i32,
        bottom: origin.1 + (pixels.y + pixels.height) as i32,
    }
}

fn close_source_overlay(
    group: crate::overlay::result::scene_compositor::SourceGroupHandle,
    controller: HWND,
) {
    crate::overlay::result::scene_compositor::remove_source_group(group);
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(controller),
            WM_CLOSE,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn pump_messages() {
    let mut message = MSG::default();
    while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() } {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn nearest_control_color<'a>(
    regions: impl Iterator<Item = &'a PreparedSource>,
    capture_width: u32,
) -> String {
    regions
        .filter(|region| !region.foreground.is_empty())
        .min_by_key(|region| {
            let center_x = region.pixels.x.saturating_add(region.pixels.width / 2);
            let center_y = region.pixels.y.saturating_add(region.pixels.height / 2);
            let dx = u64::from(capture_width.saturating_sub(center_x));
            let dy = u64::from(center_y);
            dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
        })
        .map(|region| region.foreground.clone())
        .unwrap_or_else(|| "#FFFFFF".to_string())
}

fn relative_selection_anchor(
    origin: (i32, i32),
    size: (u32, u32),
    virtual_origin: (i32, i32),
) -> [i32; 4] {
    [
        origin.0.saturating_sub(virtual_origin.0),
        origin.1.saturating_sub(virtual_origin.1),
        i32::try_from(size.0).unwrap_or(i32::MAX),
        i32::try_from(size.1).unwrap_or(i32::MAX),
    ]
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
