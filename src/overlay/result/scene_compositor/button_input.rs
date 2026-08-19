use super::protocol::{ButtonAction, ChildEvent, DragOutcome, SceneCard, SceneRect};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, GetWindowRect, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER,
};

#[derive(Debug, PartialEq)]
pub(super) enum RendererInput {
    Unhandled,
    Handled,
    RefreshRegion,
    Event(ChildEvent),
    EventAndRefresh(ChildEvent),
}

#[derive(Clone, Copy)]
enum DragMode {
    One,
    Group,
    All,
}

struct ActiveDrag {
    id: isize,
    targets: Vec<isize>,
    click_outcome: DragOutcome,
}

static BUTTON_REGIONS: LazyLock<Mutex<Vec<SceneRect>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static ACTIVE_DRAG: Mutex<Option<ActiveDrag>> = Mutex::new(None);
static EXTERNAL_DRAG: AtomicBool = AtomicBool::new(false);

pub(super) fn handle_renderer_message(
    body: &str,
    host: HWND,
    cards: &HashMap<isize, SceneCard>,
) -> RendererInput {
    let Ok(message) = serde_json::from_str::<serde_json::Value>(body) else {
        return RendererInput::Unhandled;
    };
    let Some(action) = message.get("action").and_then(|value| value.as_str()) else {
        return RendererInput::Unhandled;
    };
    if action == "update_clickable_regions" {
        update_regions(&message);
        return RendererInput::RefreshRegion;
    }
    let id = message
        .get("hwnd")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<isize>().ok())
        .unwrap_or(0);
    if id == 0 {
        return RendererInput::RefreshRegion;
    }
    if action == "copy_selection" {
        let text = message
            .get("text")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if text.is_empty() || !cards.get(&id).is_some_and(|card| card.visible) {
            return RendererInput::Handled;
        }
        let captured = crate::overlay::utils::copy_to_clipboard(text, host);
        crate::debug_log::log_debug(&format!(
            "[ResultClipboard] id={id} status={} chars={}",
            if captured { "published" } else { "failed" },
            text.chars().count()
        ));
        return RendererInput::Handled;
    }
    match action {
        "interact" => RendererInput::Event(ChildEvent::Interaction { id }),
        "request_focus" => {
            super::activation::focus_renderer(host);
            RendererInput::RefreshRegion
        }
        "result_drag_start" => begin_drag(id, DragMode::One, cards),
        "result_group_drag_start" => begin_drag(id, DragMode::Group, cards),
        "result_all_drag_start" => begin_drag(id, DragMode::All, cards),
        "result_drag_finish" => finish_drag_from_message(&message),
        _ => button_action(id, action, &message)
            .map(RendererInput::Event)
            .unwrap_or(RendererInput::RefreshRegion),
    }
}

fn button_action(id: isize, name: &str, message: &serde_json::Value) -> Option<ChildEvent> {
    let text = || {
        message
            .get("text")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let action = match name {
        "copy" => ButtonAction::Copy,
        "undo" => ButtonAction::Undo,
        "redo" => ButtonAction::Redo,
        "edit" => ButtonAction::Edit,
        "download" => ButtonAction::Download,
        "back" => ButtonAction::Back,
        "forward" => ButtonAction::Forward,
        "speaker" => ButtonAction::Speaker,
        "set_opacity" => ButtonAction::SetOpacity {
            value: message
                .get("value")
                .and_then(|value| value.as_f64())
                .unwrap_or(100.0)
                .clamp(10.0, 100.0)
                .round() as u8,
        },
        "submit_refine" => ButtonAction::SubmitRefine { text: text() },
        "cancel_refine" => ButtonAction::CancelRefine,
        "history_up_refine" => ButtonAction::HistoryUpRefine { text: text() },
        "history_down_refine" => ButtonAction::HistoryDownRefine { text: text() },
        "mic" => ButtonAction::Mic,
        _ => return None,
    };
    Some(ChildEvent::ButtonAction { id, action })
}

fn update_regions(message: &serde_json::Value) {
    if is_dragging() {
        return;
    }
    let scale = message
        .get("scale")
        .and_then(|value| value.as_f64())
        .unwrap_or(1.0)
        .max(0.1);
    let regions = message
        .get("regions")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|region| {
            let number = |name| region.get(name).and_then(|value| value.as_f64());
            let x = number("x")?;
            let y = number("y")?;
            let width = number("w")?;
            let height = number("h")?;
            Some(SceneRect {
                x: (x * scale).floor() as i32,
                y: (y * scale).floor() as i32,
                width: (width * scale).ceil().max(1.0) as i32,
                height: (height * scale).ceil().max(1.0) as i32,
            })
        })
        .collect();
    *BUTTON_REGIONS.lock().unwrap() = regions;
}

fn begin_drag(id: isize, mode: DragMode, cards: &HashMap<isize, SceneCard>) -> RendererInput {
    let Some(card) = cards.get(&id) else {
        return RendererInput::RefreshRegion;
    };
    let mut targets = match mode {
        DragMode::One => vec![id],
        DragMode::Group => card.controls.group_ids.clone(),
        DragMode::All => cards.keys().copied().collect(),
    };
    if targets.is_empty() {
        targets.push(id);
    }
    let click_outcome = match mode {
        DragMode::One => DragOutcome::CloseOne,
        DragMode::Group => DragOutcome::CloseGroup,
        DragMode::All => DragOutcome::CloseAll,
    };
    *ACTIVE_DRAG.lock().unwrap() = Some(ActiveDrag {
        id,
        targets,
        click_outcome,
    });
    BUTTON_REGIONS.lock().unwrap().clear();
    RendererInput::EventAndRefresh(ChildEvent::DragStarted)
}

pub(super) fn interactive_regions() -> Vec<SceneRect> {
    if is_dragging() {
        Vec::new()
    } else {
        BUTTON_REGIONS.lock().unwrap().clone()
    }
}

pub(super) fn set_external_drag(active: bool) {
    EXTERNAL_DRAG.store(active, Ordering::SeqCst);
    if active {
        BUTTON_REGIONS.lock().unwrap().clear();
    }
}

pub(super) fn is_dragging() -> bool {
    EXTERNAL_DRAG.load(Ordering::SeqCst) || ACTIVE_DRAG.lock().unwrap().is_some()
}

unsafe fn move_targets(targets: &[isize], dx: i32, dy: i32) {
    unsafe {
        let Ok(mut batch) = BeginDeferWindowPos(targets.len() as i32) else {
            return;
        };
        for target in targets {
            let hwnd = HWND(*target as *mut std::ffi::c_void);
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                continue;
            }
            batch = DeferWindowPos(
                batch,
                hwnd,
                None,
                rect.left + dx,
                rect.top + dy,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .unwrap_or(batch);
        }
        let _ = EndDeferWindowPos(batch);
    }
}

fn finish_drag_with_offset(dx: i32, dy: i32) -> Option<ChildEvent> {
    let drag = ACTIVE_DRAG.lock().unwrap().take()?;
    let outcome = if dx.saturating_mul(dx) + dy.saturating_mul(dy) < 25 {
        drag.click_outcome
    } else {
        unsafe { move_targets(&drag.targets, dx, dy) };
        DragOutcome::Moved
    };
    Some(ChildEvent::DragFinished {
        id: drag.id,
        targets: drag.targets,
        outcome,
    })
}

fn finish_drag_from_message(message: &serde_json::Value) -> RendererInput {
    let (dx, dy) = drag_offset(message);
    finish_drag_with_offset(dx, dy)
        .map(RendererInput::EventAndRefresh)
        .unwrap_or(RendererInput::RefreshRegion)
}

fn drag_offset(message: &serde_json::Value) -> (i32, i32) {
    let coordinate = |name| {
        message
            .get(name)
            .and_then(|value| value.as_i64())
            .unwrap_or_default()
            .clamp(i32::MIN as i64, i32::MAX as i64) as i32
    };
    (coordinate("dx"), coordinate("dy"))
}

#[cfg(test)]
mod tests {
    use super::{button_action, drag_offset, interactive_regions, update_regions};
    use crate::overlay::result::scene_compositor::protocol::{ButtonAction, ChildEvent, SceneRect};

    #[test]
    fn renderer_actions_become_typed_parent_events() {
        let message = serde_json::json!({ "value": 71 });
        assert_eq!(
            button_action(42, "set_opacity", &message),
            Some(ChildEvent::ButtonAction {
                id: 42,
                action: ButtonAction::SetOpacity { value: 71 }
            })
        );
        assert!(button_action(42, "unknown", &message).is_none());
    }

    #[test]
    fn selection_copy_is_not_a_parent_button_action() {
        let message = serde_json::json!({ "text": "selected result" });
        assert!(button_action(42, "copy_selection", &message).is_none());
    }

    #[test]
    fn compositor_drag_offset_is_physical_and_bounded() {
        assert_eq!(
            drag_offset(&serde_json::json!({ "dx": -720, "dy": 480 })),
            (-720, 480)
        );
        assert_eq!(
            drag_offset(&serde_json::json!({ "dx": i64::MAX, "dy": i64::MIN })),
            (i32::MAX, i32::MIN)
        );
        assert_eq!(drag_offset(&serde_json::json!({})), (0, 0));
    }

    #[test]
    fn css_hit_regions_are_converted_once_to_physical_pixels() {
        update_regions(&serde_json::json!({
            "scale": 1.5,
            "regions": [{ "x": 10.0, "y": 20.0, "w": 30.0, "h": 40.0 }]
        }));

        assert_eq!(
            interactive_regions(),
            vec![SceneRect {
                x: 15,
                y: 30,
                width: 45,
                height: 60,
            }]
        );
        update_regions(&serde_json::json!({ "scale": 1.0, "regions": [] }));
    }
}
