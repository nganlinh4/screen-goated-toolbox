//! Browser application, not native receipt, completes a scene revision.
use super::super::{button_input, child_commands, input_surface, processing, visual_region};
use super::{CARDS, COMMANDS, ChildEvent, HostCommand, INPUT_SURFACE_HWND, RENDERER_READY};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU64};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

static FIRST_PAINT_REQUESTED: AtomicBool = AtomicBool::new(false);
static FIRST_PAINT_TOKEN: AtomicU64 = AtomicU64::new(0);
thread_local! {
    static BATCHES: RefCell<(u64, HashSet<u64>)> = RefCell::new((0, HashSet::new()));
    static DRAINING: Cell<bool> = const { Cell::new(false) };
    static DRAIN_AGAIN: Cell<bool> = const { Cell::new(false) };
}

struct DrainGuard;
impl Drop for DrainGuard {
    fn drop(&mut self) {
        DRAINING.with(|value| value.set(false));
        if DRAIN_AGAIN.with(|value| value.replace(false)) {
            super::post_host(super::WM_DRAIN_COMMANDS);
        }
    }
}

fn register_batch() -> u64 {
    BATCHES.with(|batches| {
        let mut batches = batches.borrow_mut();
        if batches.1.len() >= 1024 {
            crate::crash_report::exit(1, "renderer-batch-backlog");
        }
        batches.0 = batches
            .0
            .checked_add(1)
            .unwrap_or_else(|| crate::crash_report::exit(1, "renderer-batch-overflow"));
        let id = batches.0;
        batches.1.insert(id);
        id
    })
}

pub(super) fn complete_batch(id: u64) {
    BATCHES.with(|batches| {
        batches.borrow_mut().1.remove(&id);
    });
}

pub(super) fn is_idle() -> bool {
    !DRAINING.with(Cell::get) && BATCHES.with(|batches| batches.borrow().1.is_empty())
}

pub(super) fn current_paint_token(token: u64) -> bool {
    token == FIRST_PAINT_TOKEN.load(Ordering::SeqCst)
}

fn first_paint_token(visible: bool) -> u64 {
    if !visible {
        FIRST_PAINT_REQUESTED.store(false, Ordering::SeqCst);
        FIRST_PAINT_TOKEN.fetch_add(1, Ordering::SeqCst);
    } else if !visual_region::first_scene_painted()
        && !FIRST_PAINT_REQUESTED.swap(true, Ordering::SeqCst)
    {
        return FIRST_PAINT_TOKEN.fetch_add(1, Ordering::SeqCst) + 1;
    }
    0
}

pub(super) fn drain(hwnd: HWND) {
    if !RENDERER_READY.load(Ordering::SeqCst) || super::topology_transition::active() {
        return;
    }
    if !super::topology::matches_display(hwnd) {
        super::topology_transition::begin(hwnd);
        return;
    }
    if DRAINING.with(|value| value.replace(true)) {
        DRAIN_AGAIN.with(|value| value.set(true));
        return;
    }
    let _drain_guard = DrainGuard;
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
        let Some(command) = super::topology::localize(command, super::topology::origin()) else {
            crate::crash_report::exit(1, "renderer-coordinate-overflow");
        };
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
    let cards = CARDS.lock().unwrap().clone();
    if input_surface::is_refine_text_active()
        && !cards
            .values()
            .any(|card| button_input::keeps_refine_focus(card.visible, &card.controls))
    {
        input_surface::restore_nonactivating(input_hwnd);
    }
    let paint_token =
        first_paint_token(cards.values().any(|card| card.visible) || processing::is_visible());
    let buttons = button_input::interactive_regions();
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) };
    let x = super::super::compositor_host_x(unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) }, width);
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    if !visual_region::update(hwnd, &cards, &buttons, width, height) {
        visual_region::hide(input_hwnd);
        crate::crash_report::exit(1, "renderer-visual-region-failed");
    }
    let applied =
        input_surface::update_input_regions(input_hwnd, &cards, &buttons, x, y, width, height);
    if !visual_region::first_scene_painted() {
        visual_region::hide(input_hwnd);
    }
    if visual_region::first_scene_painted()
        && applied.blocks_visuals(&cards)
        && !processing::is_visible()
    {
        visual_region::hide(hwnd);
    } else if !visual_region::stack_below_input(hwnd, input_hwnd) {
        visual_region::hide(hwnd);
        visual_region::hide(input_hwnd);
        crate::crash_report::exit(1, "renderer-window-stack-failed");
    }
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
    let batch_id = register_batch();
    super::evaluate_script(&format!(
        "window.__SGT_APPLY_SCENE_BATCH__(function(){{{scripts}}},{acknowledgement},{paint_token},'{batch_id}');"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_visible_current_editor_keeps_native_refine_focus() {
        let mut controls = super::super::super::protocol::SceneControls {
            is_editing: true,
            edit_session: Some("1".into()),
            ..Default::default()
        };
        assert!(button_input::keeps_refine_focus(true, &controls));
        assert!(!button_input::keeps_refine_focus(false, &controls));
        controls.hidden = true;
        assert!(!button_input::keeps_refine_focus(true, &controls));
        controls.hidden = false;
        controls.edit_session = None;
        assert!(!button_input::keeps_refine_focus(true, &controls));
        controls.edit_session = Some("0".into());
        assert!(!button_input::keeps_refine_focus(true, &controls));
    }

    #[test]
    fn batch_barrier_waits_for_every_out_of_order_completion() {
        assert!(is_idle());
        let first = register_batch();
        let second = register_batch();
        assert_ne!(first, second);
        complete_batch(second);
        assert!(!is_idle());
        complete_batch(second);
        assert!(!is_idle());
        complete_batch(first);
        assert!(is_idle());
    }

    #[test]
    fn native_drain_blocks_barrier_even_before_a_batch_is_registered() {
        DRAINING.with(|value| value.set(true));
        let guard = DrainGuard;
        assert!(!is_idle());
        drop(guard);
        assert!(is_idle());
    }
}
