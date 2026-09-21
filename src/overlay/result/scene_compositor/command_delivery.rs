//! Browser application, not native receipt, completes a scene revision.
use super::super::{button_input, child_commands, input_surface, processing, visual_region};
use super::{CARDS, COMMANDS, ChildEvent, HostCommand, INPUT_SURFACE_HWND, RENDERER_READY};
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

pub(super) fn drain(hwnd: HWND) {
    if !RENDERER_READY.load(Ordering::SeqCst) {
        return;
    }
    let commands = COMMANDS.lock().unwrap().drain();
    if commands.is_empty() {
        return;
    }
    let mut scripts = String::new();
    let mut revision = 0;
    for command in commands {
        if let HostCommand::CaptureExclusion { enabled } = command {
            if let Err(error) = super::super::capture_exclusion::apply(hwnd, enabled) {
                super::emit_event(ChildEvent::CommandError {
                    command: "capture_exclusion".into(),
                    id: None,
                    error: error.to_string(),
                });
            }
            continue;
        }
        if command == HostCommand::Shutdown {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
        if let HostCommand::ApplyRevision { revision: next } = command {
            revision = revision.max(next);
            continue;
        }
        if !child_commands::apply(&command) {
            continue;
        }
        match serde_json::to_string(&command) {
            Ok(json) => scripts.push_str(&format!("window.applyHostCommand({json});")),
            Err(error) => {
                super::emit_event(ChildEvent::CommandError {
                    command: "scene_serialization".into(),
                    id: None,
                    error: error.to_string(),
                });
                return;
            }
        }
    }
    let input_val = INPUT_SURFACE_HWND.load(Ordering::SeqCst);
    let input_hwnd = if input_val != 0 {
        HWND(input_val as *mut _)
    } else {
        hwnd
    };
    let cards = CARDS.lock().unwrap();
    let buttons = button_input::interactive_regions();
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) };
    let x = super::super::compositor_host_x(unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) }, width);
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    if !visual_region::update(hwnd, &cards, &buttons, width, height) {
        visual_region::hide(input_hwnd);
        std::process::exit(1);
    }
    let applied =
        input_surface::update_input_regions(input_hwnd, &cards, &buttons, x, y, width, height);
    if applied.blocks_visuals(&cards) && !processing::is_visible() {
        visual_region::hide(hwnd);
    } else if !visual_region::stack_below_input(hwnd, input_hwnd) {
        visual_region::hide(hwnd);
        visual_region::hide(input_hwnd);
        std::process::exit(1);
    }
    drop(cards);
    let acknowledgement = if revision > 0 {
        serde_json::to_string(&ChildEvent::StateAcknowledged {
            revision,
            visible_cards: applied.visible_cards,
            input_rect_count: applied.input_region_count,
        })
        .expect("scene acknowledgement must serialize")
    } else {
        "null".to_string()
    };
    super::evaluate_script(&format!(
        "window.__SGT_APPLY_SCENE_BATCH__(function(){{{scripts}}},{acknowledgement});"
    ));
}
