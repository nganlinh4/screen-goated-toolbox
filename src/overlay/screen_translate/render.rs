use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender};
use std::time::Duration;

use anyhow::{Context, Result};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetSystemMetrics, IsWindow, MSG, PM_REMOVE, PeekMessageW, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_SHOWNOACTIVATE, ShowWindow, TranslateMessage,
};

use super::backdrop::{encode_data_url, reconstruct_blob_image_with_background};
use super::contract::{DetectedTextRegion, TranslationDocument, TranslationRegion};
use super::geometry::{MIN_READABLE_HEIGHT, MIN_READABLE_WIDTH, PixelRegion, normalized_region};
use super::layout::{LayoutBlock, LayoutInput, plan_blocks};
use crate::overlay::result::{RefineContext, ResultControlOptions, ResultWindowParams, WindowType};
use crate::overlay::selection::CapturedRegion;

struct PreparedSource {
    pixels: PixelRegion,
    foreground: String,
    source_text: String,
    background: Option<([u8; 3], u8)>,
}

struct PreparedBlock {
    member_ids: Vec<u16>,
    layout: PixelRegion,
    backdrop: String,
    foreground: String,
    preferred_font_size: f32,
}

struct PreparedScene {
    sources: HashMap<u16, PreparedSource>,
    blocks: Vec<PreparedBlock>,
    member_to_block: HashMap<u16, usize>,
}

struct LiveBlock {
    prepared: PreparedBlock,
    hwnd: Option<HWND>,
    rendered_text: Option<String>,
}

struct TranslationControls {
    anchor: [i32; 4],
    color: String,
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
    let prepared = match prepare_scene(job_id, &capture, &candidates) {
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
        color: nearest_control_color(prepared.sources.values(), capture.width),
    };
    let chain_id = format!("screen-translate-{job_id}");
    let PreparedScene {
        sources,
        blocks,
        member_to_block,
    } = prepared;
    let mut blocks = blocks
        .into_iter()
        .map(|prepared| LiveBlock {
            prepared,
            hwnd: None,
            rendered_text: None,
        })
        .collect::<Vec<_>>();
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
                let id = region.id;
                translations.insert(id, region);
                if let Some(&block_index) = member_to_block.get(&id)
                    && refresh_block(
                        block_index,
                        &mut blocks,
                        &sources,
                        &translations,
                        origin,
                        &controls,
                        &chain_id,
                        &trace_id,
                    )
                {
                    had_visible = true;
                    super::runtime::register_overlay(job_id, chain_id.clone());
                    if let Some(sender) = first_visible.take() {
                        let _ = sender.send(());
                    }
                }
            }
            Ok(RenderCommand::Complete(document)) => {
                translations.extend(
                    document
                        .regions
                        .into_iter()
                        .map(|region| (region.id, region)),
                );
                for block_index in 0..blocks.len() {
                    if refresh_block(
                        block_index,
                        &mut blocks,
                        &sources,
                        &translations,
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

#[allow(clippy::too_many_arguments)]
fn refresh_block(
    block_index: usize,
    blocks: &mut [LiveBlock],
    sources: &HashMap<u16, PreparedSource>,
    translations: &HashMap<u16, TranslationRegion>,
    origin: (i32, i32),
    controls: &TranslationControls,
    chain_id: &str,
    trace_id: &str,
) -> bool {
    let block = &blocks[block_index].prepared;
    if !block
        .member_ids
        .iter()
        .all(|id| translations.contains_key(id))
    {
        return false;
    }
    if !block
        .member_ids
        .iter()
        .any(|id| translations.get(id).is_some_and(should_render))
    {
        return false;
    }
    let text = block_text(block, sources, translations);
    if blocks[block_index].rendered_text.as_deref() == Some(text.as_str()) {
        return false;
    }
    if let Some(hwnd) = blocks[block_index].hwnd {
        crate::overlay::result::update_window_text(hwnd, &text);
        blocks[block_index].rendered_text = Some(text);
        return false;
    }
    let root = blocks.iter().find_map(|block| block.hwnd);
    let hwnd = create_region_window(
        origin,
        block,
        text.clone(),
        root.is_none(),
        controls,
        chain_id,
        trace_id,
    );
    if let Some(root) = root {
        crate::overlay::result::link_windows(root, hwnd);
    }
    blocks[block_index].hwnd = Some(hwnd);
    blocks[block_index].rendered_text = Some(text);
    root.is_none()
}

fn block_text(
    block: &PreparedBlock,
    sources: &HashMap<u16, PreparedSource>,
    translations: &HashMap<u16, TranslationRegion>,
) -> String {
    block
        .member_ids
        .iter()
        .filter_map(|id| {
            translations
                .get(id)
                .map(|region| region.translated_text.as_str())
                .or_else(|| sources.get(id).map(|source| source.source_text.as_str()))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn should_render(region: &TranslationRegion) -> bool {
    !super::contract::text_is_source_equivalent(&region.source_text, &region.translated_text)
}

fn create_region_window(
    origin: (i32, i32),
    group: &PreparedBlock,
    translated_text: String,
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
        scale_percent: 200,
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
            initial_text: translated_text,
            preset_id: None,
            is_chain_root: is_root,
            latency_trace_id: Some(trace_id.to_string()),
        },
        group.backdrop.clone(),
        group.foreground.clone(),
        chain_id.to_string(),
        control_options,
        Some(group.preferred_font_size),
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

fn prepare_scene(
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
                candidate.source_text.clone(),
            )
        })
        .filter(|(_, region, _, _)| {
            region.width >= MIN_READABLE_WIDTH && region.height >= MIN_READABLE_HEIGHT
        })
        .collect::<Vec<_>>();
    let masks = located
        .iter()
        .map(|(_, region, _, _)| *region)
        .collect::<Vec<_>>();
    let layout_inputs = located
        .iter()
        .map(|(id, pixels, appearance, _)| LayoutInput {
            id: *id,
            pixels: *pixels,
            appearance: *appearance,
        })
        .collect::<Vec<_>>();
    let mut prepared = HashMap::with_capacity(located.len());
    for (id, pixels, appearance, source_text) in located {
        if !super::runtime::is_current(job_id) {
            break;
        }
        let background = appearance
            .map(|appearance| (appearance.background_rgb, appearance.background_confidence));
        let (_, inferred_foreground) =
            reconstruct_blob_image_with_background(&capture.image, pixels, &masks, background);
        let foreground = appearance
            .filter(|appearance| appearance.foreground_confidence >= 3)
            .and_then(|appearance| appearance.foreground_rgb)
            .map(super::appearance::color_hex)
            .unwrap_or(inferred_foreground);
        prepared.insert(
            id,
            PreparedSource {
                pixels,
                foreground,
                source_text,
                background,
            },
        );
    }
    let blocks = plan_blocks(&layout_inputs)
        .into_iter()
        .map(|block| prepare_block(block, &prepared, &capture.image, &masks))
        .collect::<Result<Vec<_>>>()?;
    let member_to_block = blocks
        .iter()
        .enumerate()
        .flat_map(|(index, block)| block.member_ids.iter().map(move |id| (*id, index)))
        .collect();
    Ok(PreparedScene {
        sources: prepared,
        blocks,
        member_to_block,
    })
}

fn prepare_block(
    block: LayoutBlock,
    sources: &HashMap<u16, PreparedSource>,
    image: &image::RgbaImage,
    masks: &[PixelRegion],
) -> Result<PreparedBlock> {
    let background = block
        .member_ids
        .iter()
        .filter_map(|id| sources.get(id)?.background)
        .max_by_key(|(_, confidence)| *confidence);
    let (backdrop, inferred_foreground) =
        reconstruct_blob_image_with_background(image, block.pixels, masks, background);
    let foreground = block
        .member_ids
        .iter()
        .find_map(|id| sources.get(id).map(|source| source.foreground.clone()))
        .unwrap_or(inferred_foreground);
    Ok(PreparedBlock {
        member_ids: block.member_ids,
        layout: block.pixels,
        backdrop: encode_data_url(&backdrop)?,
        foreground,
        preferred_font_size: (block.pixels.height as f32).clamp(8.0, 200.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_anchor_is_relative_to_the_virtual_desktop() {
        assert_eq!(
            relative_selection_anchor((-1200, 300), (640, 480), (-1920, -200)),
            [720, 500, 640, 480]
        );
    }

    #[test]
    fn source_equivalent_regions_do_not_require_visual_replacement() {
        let region = TranslationRegion {
            id: 1,
            member_ids: vec![1],
            selections: Vec::new(),
            semantic_role: super::super::contract::SemanticRole::Value,
            source_text: "example.com/path".to_string(),
            translated_text: "example.com/path".to_string(),
            bounds: [0, 0, 10, 10].into(),
            background_color: None,
            text_color: None,
        };
        assert!(!should_render(&region));
    }

    #[test]
    fn grouped_text_reflows_without_preserving_ocr_line_breaks() {
        let block = PreparedBlock {
            member_ids: vec![1, 2],
            layout: PixelRegion {
                x: 0,
                y: 0,
                width: 100,
                height: 40,
            },
            backdrop: String::new(),
            foreground: String::new(),
            preferred_font_size: 12.0,
        };
        let sources = HashMap::from([
            (
                1,
                PreparedSource {
                    pixels: block.layout,
                    foreground: String::new(),
                    source_text: "first".to_string(),
                    background: None,
                },
            ),
            (
                2,
                PreparedSource {
                    pixels: block.layout,
                    foreground: String::new(),
                    source_text: "second".to_string(),
                    background: None,
                },
            ),
        ]);
        let translated = TranslationRegion {
            id: 1,
            member_ids: vec![1],
            selections: Vec::new(),
            semantic_role: super::super::contract::SemanticRole::Standalone,
            source_text: "first".to_string(),
            translated_text: "translated".to_string(),
            bounds: [0, 0, 10, 10].into(),
            background_color: None,
            text_color: None,
        };
        let mut translations = HashMap::from([(1, translated.clone())]);
        assert_eq!(
            block_text(&block, &sources, &translations),
            "translated second"
        );
        translations.clear();
        assert_eq!(block_text(&block, &sources, &translations), "first second");
    }
}
