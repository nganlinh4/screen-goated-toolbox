use super::super::render_scene::{PreparedBlock, PreparedSource};
use super::CONTROL_SCALE_PERCENT;
use super::TranslationControls;
use crate::overlay::result::{RefineContext, ResultControlOptions, ResultWindowParams, WindowType};
use anyhow::Result;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage, WM_CLOSE,
};

pub(super) fn create_controller_window(
    origin: (i32, i32),
    controls: &TranslationControls,
    chain_id: &str,
    trace_id: &str,
    show_controls: bool,
) -> Result<HWND> {
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
    let hwnd = checked_controller(hwnd)?;
    crate::overlay::result::configure_deferred_text_only_result_window(
        hwnd,
        crate::overlay::result::TextOnlyResultOptions {
            backdrop_data_url: String::new(),
            foreground_color: controls.color.clone(),
            chain_id: chain_id.to_string(),
            control_options: show_controls.then_some(ResultControlOptions {
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
    Ok(hwnd)
}

pub(super) fn checked_controller(hwnd: HWND) -> Result<HWND> {
    anyhow::ensure!(
        !hwnd.is_invalid(),
        "screen translation controller could not be created"
    );
    Ok(hwnd)
}

pub(super) fn block_target_rect(origin: (i32, i32), group: &PreparedBlock) -> RECT {
    let pixels = group.layout;
    RECT {
        left: origin.0 + pixels.x as i32,
        top: origin.1 + pixels.y as i32,
        right: origin.0 + (pixels.x + pixels.width) as i32,
        bottom: origin.1 + (pixels.y + pixels.height) as i32,
    }
}

pub(super) fn close_source_overlay(
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

pub(super) fn pump_messages() {
    let mut message = MSG::default();
    while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() } {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

pub(super) fn nearest_control_color<'a>(
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

pub(super) fn selection_anchor(origin: (i32, i32), size: (u32, u32)) -> [i32; 4] {
    [
        origin.0,
        origin.1,
        i32::try_from(size.0).unwrap_or(i32::MAX),
        i32::try_from(size.1).unwrap_or(i32::MAX),
    ]
}
