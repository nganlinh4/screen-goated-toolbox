use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::time::Duration;

use anyhow::{Context, Result};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetSystemMetrics, IsWindow, MSG, PM_REMOVE, PeekMessageW, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_SHOWNOACTIVATE, ShowWindow, TranslateMessage,
};

use super::backdrop::reconstruct_blob;
use super::contract::{DetectedTextRegion, TranslationDocument, TranslationRegion};
use super::geometry::{MIN_READABLE_HEIGHT, MIN_READABLE_WIDTH, PixelRegion, normalized_region};
use super::inference::TranslationEvent;
use crate::overlay::result::{
    RefineContext, ResultDismissControl, ResultWindowParams, WINDOW_STATES, WindowType,
};
use crate::overlay::selection::CapturedRegion;

struct PreparedSource {
    pixels: PixelRegion,
    backdrop: String,
    foreground: String,
}

struct TranslationControls {
    always_visible: bool,
    anchor: [i32; 4],
    color: String,
}

enum RenderCommand {
    Region(TranslationRegion),
    Reset,
    Complete(TranslationDocument),
}

pub(super) struct TranslationOverlay {
    sender: SyncSender<RenderCommand>,
    completion: Receiver<usize>,
}

impl TranslationOverlay {
    pub(super) fn send(&mut self, event: TranslationEvent) {
        let command = match event {
            TranslationEvent::Region(region) => RenderCommand::Region(region),
            TranslationEvent::Reset => RenderCommand::Reset,
        };
        let _ = self.sender.send(command);
    }

    pub(super) fn complete(self, document: TranslationDocument) -> Result<usize> {
        self.sender
            .send(RenderCommand::Complete(document))
            .context("screen translation renderer stopped early")?;
        self.completion
            .recv()
            .context("screen translation renderer stopped before completion")
    }
}

pub(super) fn start(
    job_id: u64,
    capture: &CapturedRegion,
    candidates: &[DetectedTextRegion],
    trace_id: &str,
) -> Result<(TranslationOverlay, Receiver<()>)> {
    let prepared = prepare_sources(job_id, capture, candidates)?;
    crate::overlay::result::latency::mark(trace_id, "backdrops_ready");
    let origin = (capture.left, capture.top);
    let capture_size = (capture.width, capture.height);
    let virtual_origin = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        )
    };
    let controls = TranslationControls {
        always_visible: super::runtime::record_dismiss_button_impression(),
        anchor: relative_selection_anchor(origin, capture_size, virtual_origin),
        color: nearest_control_color(prepared.values(), capture.width),
    };
    let (command_sender, command_receiver) = std::sync::mpsc::sync_channel(32);
    let (visible_sender, visible_receiver) = std::sync::mpsc::sync_channel(1);
    let (completion_sender, completion_receiver) = std::sync::mpsc::sync_channel(1);
    let trace_id = trace_id.to_string();
    std::thread::Builder::new()
        .name("sgt-screen-translate-overlay".to_string())
        .spawn(move || {
            run_overlay_thread(
                job_id,
                origin,
                prepared,
                controls,
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
    prepared: HashMap<u16, PreparedSource>,
    controls: TranslationControls,
    trace_id: String,
    receiver: Receiver<RenderCommand>,
    first_visible: SyncSender<()>,
    completion: SyncSender<usize>,
) {
    let chain_id = format!("screen-translate-{job_id}");
    let mut windows = Vec::new();
    let mut visible_ids = HashSet::new();
    let mut first_visible = Some(first_visible);
    loop {
        pump_messages();
        windows.retain(|hwnd| unsafe { IsWindow(Some(*hwnd)).as_bool() });
        if !super::runtime::is_current(job_id) {
            crate::overlay::result::close_chain_windows(&chain_id);
            break;
        }
        match receiver.recv_timeout(Duration::from_millis(8)) {
            Ok(RenderCommand::Region(region)) => {
                if visible_ids.insert(region.id)
                    && let Some(source) = prepared.get(&region.id)
                {
                    let root = windows.is_empty();
                    let hwnd = create_region_window(
                        origin,
                        source,
                        region.translated_text,
                        root,
                        &controls,
                        &chain_id,
                        &trace_id,
                    );
                    windows.push(hwnd);
                    if root {
                        super::runtime::register_overlay(job_id, chain_id.clone());
                        if let Some(sender) = first_visible.take() {
                            let _ = sender.send(());
                        }
                    }
                }
            }
            Ok(RenderCommand::Reset) => {
                crate::overlay::result::close_chain_windows(&chain_id);
                windows.clear();
                visible_ids.clear();
            }
            Ok(RenderCommand::Complete(document)) => {
                for region in document.regions.iter().cloned() {
                    if visible_ids.insert(region.id)
                        && let Some(source) = prepared.get(&region.id)
                    {
                        let root = windows.is_empty();
                        let hwnd = create_region_window(
                            origin,
                            source,
                            region.translated_text,
                            root,
                            &controls,
                            &chain_id,
                            &trace_id,
                        );
                        windows.push(hwnd);
                        if root {
                            super::runtime::register_overlay(job_id, chain_id.clone());
                            if let Some(sender) = first_visible.take() {
                                let _ = sender.send(());
                            }
                        }
                    }
                }
                update_copy_control(
                    windows.first().copied(),
                    document
                        .regions
                        .iter()
                        .map(|region| region.translated_text.as_str())
                        .collect::<Vec<_>>()
                        .join("\r\n"),
                );
                let _ = completion.send(windows.len());
                while windows
                    .iter()
                    .any(|hwnd| unsafe { IsWindow(Some(*hwnd)).as_bool() })
                {
                    pump_messages();
                    std::thread::sleep(Duration::from_millis(8));
                }
                return;
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn create_region_window(
    origin: (i32, i32),
    source: &PreparedSource,
    translated_text: String,
    is_root: bool,
    controls: &TranslationControls,
    chain_id: &str,
    trace_id: &str,
) -> HWND {
    let pixels = source.pixels;
    let target_rect = RECT {
        left: origin.0 + pixels.x as i32,
        top: origin.1 + pixels.y as i32,
        right: origin.0 + (pixels.x + pixels.width) as i32,
        bottom: origin.1 + (pixels.y + pixels.height) as i32,
    };
    let dismiss_control = is_root.then(|| ResultDismissControl {
        always_visible: controls.always_visible,
        anchor_rect: Some(controls.anchor),
        copy_text: None,
        control_color: Some(controls.color.clone()),
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
        source.backdrop.clone(),
        source.foreground.clone(),
        chain_id.to_string(),
        dismiss_control,
    );
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    crate::overlay::result::scene_compositor::sync_window(hwnd, true);
    hwnd
}

fn update_copy_control(root: Option<HWND>, copy_text: String) {
    let Some(root) = root else {
        return;
    };
    if let Some(control) = WINDOW_STATES
        .lock()
        .unwrap()
        .get_mut(&(root.0 as isize))
        .and_then(|state| state.dismiss_control.as_mut())
    {
        control.copy_text = Some(copy_text);
    }
    crate::overlay::result::scene_compositor::sync_controls(root);
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

fn prepare_sources(
    job_id: u64,
    capture: &CapturedRegion,
    candidates: &[DetectedTextRegion],
) -> Result<HashMap<u16, PreparedSource>> {
    let located = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.id,
                normalized_region(candidate.bounds, capture.width, capture.height),
            )
        })
        .filter(|(_, region)| {
            region.width >= MIN_READABLE_WIDTH && region.height >= MIN_READABLE_HEIGHT
        })
        .collect::<Vec<_>>();
    let masks = located
        .iter()
        .map(|(_, region)| *region)
        .collect::<Vec<_>>();
    let mut prepared = HashMap::with_capacity(located.len());
    for (id, pixels) in located {
        if !super::runtime::is_current(job_id) {
            break;
        }
        let (backdrop, foreground) = reconstruct_blob(&capture.image, pixels, &masks)?;
        prepared.insert(
            id,
            PreparedSource {
                pixels,
                backdrop,
                foreground,
            },
        );
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dismiss_anchor_is_relative_to_the_virtual_desktop() {
        assert_eq!(
            relative_selection_anchor((-1200, 300), (640, 480), (-1920, -200)),
            [720, 500, 640, 480]
        );
    }
}
