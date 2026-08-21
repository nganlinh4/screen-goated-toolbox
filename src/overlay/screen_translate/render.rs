use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender};
use std::time::Duration;

use anyhow::{Context, Result};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetSystemMetrics, IsWindow, MSG, PM_REMOVE, PeekMessageW, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_SHOWNOACTIVATE, ShowWindow, TranslateMessage,
};

use super::contract::{DetectedTextRegion, TranslationDocument, TranslationRegion};
use super::render_scene::{PreparedBlock, PreparedScene, PreparedSource};
use crate::overlay::result::{RefineContext, ResultControlOptions, ResultWindowParams, WindowType};
use crate::overlay::selection::CapturedRegion;

const CONTROL_SCALE_PERCENT: u16 = 150;

struct LiveBlock {
    prepared: PreparedBlock,
    hwnd: Option<HWND>,
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
    let mut blocks: Vec<LiveBlock> = Vec::new();
    let mut translations = HashMap::new();
    let mut had_visible = false;
    let mut first_visible = Some(first_visible);
    loop {
        pump_messages();
        for block in &mut blocks {
            if block
                .hwnd
                .is_some_and(|hwnd| !unsafe { IsWindow(Some(hwnd)).as_bool() })
            {
                block.hwnd = None;
            }
        }
        if had_visible && blocks.iter().all(|block| block.hwnd.is_none()) {
            super::runtime::cancel_active();
            break;
        }
        if !super::runtime::is_current(job_id) {
            crate::overlay::result::close_chain_windows(&chain_id);
            break;
        }
        match receiver.recv_timeout(Duration::from_millis(8)) {
            Ok(RenderCommand::Region(region)) => {
                record_translations(region, &mut translations);
                ensure_blocks(&translations, &scene, &mut blocks, true);
                for block_index in 0..blocks.len() {
                    if refresh_block(
                        block_index,
                        &mut blocks,
                        &translations,
                        &scene,
                        origin,
                        &controls,
                        &chain_id,
                        &trace_id,
                    ) {
                        had_visible = true;
                        super::runtime::register_overlay(job_id, chain_id.clone());
                        if let Some(sender) = first_visible.take() {
                            let _ = sender.send(());
                        }
                    }
                }
            }
            Ok(RenderCommand::Complete(document)) => {
                for region in document.regions {
                    record_translations(region, &mut translations);
                }
                ensure_blocks(&translations, &scene, &mut blocks, false);
                for block_index in 0..blocks.len() {
                    if refresh_block(
                        block_index,
                        &mut blocks,
                        &translations,
                        &scene,
                        origin,
                        &controls,
                        &chain_id,
                        &trace_id,
                    ) {
                        super::runtime::register_overlay(job_id, chain_id.clone());
                        if let Some(sender) = first_visible.take() {
                            let _ = sender.send(());
                        }
                    }
                }
                let rendered = blocks.iter().filter(|block| block.hwnd.is_some()).count();
                let _ = completion.send(Ok(rendered));
                while blocks.iter().any(|block| {
                    block
                        .hwnd
                        .is_some_and(|hwnd| unsafe { IsWindow(Some(hwnd)).as_bool() })
                }) {
                    pump_messages();
                    std::thread::sleep(Duration::from_millis(8));
                }
                return;
            }
            Err(RecvTimeoutError::Disconnected)
                if blocks.iter().all(|block| block.hwnd.is_none()) =>
            {
                break;
            }
            Err(RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(8));
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn ensure_blocks(
    translations: &HashMap<u16, SegmentTranslation>,
    scene: &PreparedScene,
    blocks: &mut Vec<LiveBlock>,
    require_complete: bool,
) {
    for prepared in &scene.blocks {
        let resolved = prepared
            .member_ids
            .iter()
            .filter(|member_id| translations.contains_key(member_id))
            .count();
        if resolved == 0 || (require_complete && resolved != prepared.member_ids.len()) {
            continue;
        }
        if blocks
            .iter()
            .any(|block| block.prepared.component_id == prepared.component_id)
        {
            continue;
        }
        blocks.push(LiveBlock {
            prepared: prepared.clone(),
            hwnd: None,
            rendered_segments: None,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh_block(
    block_index: usize,
    blocks: &mut [LiveBlock],
    translations: &HashMap<u16, SegmentTranslation>,
    scene: &PreparedScene,
    origin: (i32, i32),
    controls: &TranslationControls,
    chain_id: &str,
    trace_id: &str,
) -> bool {
    if blocks[block_index]
        .hwnd
        .is_some_and(|hwnd| !unsafe { IsWindow(Some(hwnd)).as_bool() })
    {
        blocks[block_index].hwnd = None;
    }
    let prepared = &blocks[block_index].prepared;
    let Some(segments) = component_translation(prepared, scene, translations) else {
        return false;
    };
    if blocks[block_index].rendered_segments.as_ref() == Some(&segments)
        && blocks[block_index].hwnd.is_some()
    {
        return false;
    }
    if let Some(hwnd) = blocks[block_index].hwnd {
        crate::overlay::result::update_text_only_segments(hwnd, segments.clone());
        blocks[block_index].rendered_segments = Some(segments);
        return false;
    }
    let root = blocks.iter().find_map(|block| block.hwnd);
    let hwnd = create_region_window(
        origin,
        &blocks[block_index].prepared,
        segments.clone(),
        root.is_none(),
        controls,
        chain_id,
        trace_id,
    );
    if let Some(root) = root {
        crate::overlay::result::link_windows(root, hwnd);
    }
    blocks[block_index].hwnd = Some(hwnd);
    blocks[block_index].rendered_segments = Some(segments);
    root.is_none()
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

fn create_region_window(
    origin: (i32, i32),
    group: &PreparedBlock,
    translated_segments: Vec<String>,
    is_root: bool,
    controls: &TranslationControls,
    chain_id: &str,
    trace_id: &str,
) -> HWND {
    let pixels = group.layout;
    let target_rect = RECT {
        left: origin.0 + pixels.x as i32,
        top: origin.1 + pixels.y as i32,
        right: origin.0 + (pixels.x + pixels.width) as i32,
        bottom: origin.1 + (pixels.y + pixels.height) as i32,
    };
    let control_options = is_root.then(|| ResultControlOptions {
        anchor_rect: Some(controls.anchor),
        control_color: Some(controls.color.clone()),
        scale_percent: CONTROL_SCALE_PERCENT,
        group_actions: true,
        edit_enabled: false,
    });
    let hwnd = crate::overlay::result::create_text_only_result_window(
        ResultWindowParams {
            target_rect,
            win_type: WindowType::Primary,
            context: RefineContext::None,
            model_id: String::new(),
            provider: String::new(),
            streaming_enabled: false,
            start_editing: false,
            preset_prompt: String::new(),
            custom_bg_color: 0,
            initial_text: translated_segments.join("\n"),
            preset_id: None,
            is_chain_root: is_root,
            latency_trace_id: Some(trace_id.to_string()),
        },
        crate::overlay::result::TextOnlyResultOptions {
            backdrop_data_url: group.backdrop.clone(),
            foreground_color: group.foreground.clone(),
            chain_id: chain_id.to_string(),
            control_options,
            preferred_font_size: Some(group.preferred_font_size),
            source_vertical: group.vertical_text,
            source_regions: group.source_regions.clone(),
            source_segments: translated_segments,
            opacity_percent: Some(controls.opacity_percent),
        },
    );
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    crate::overlay::result::scene_compositor::sync_window(hwnd, true);
    hwnd
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
