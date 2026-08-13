use super::protocol::{ButtonAction, ChildEvent, DragOutcome, SceneCard, SceneRect};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, GetCursorPos, GetWindowRect, IsWindow,
    SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
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
    start: POINT,
    last: POINT,
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
        "result_drag_start" => begin_drag(host, id, DragMode::One, cards),
        "result_group_drag_start" => begin_drag(host, id, DragMode::Group, cards),
        "result_all_drag_start" => begin_drag(host, id, DragMode::All, cards),
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
        "dismiss_chain" => ButtonAction::DismissChain,
        "copy_all" => ButtonAction::CopyAll,
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

fn begin_drag(
    host: HWND,
    id: isize,
    mode: DragMode,
    cards: &HashMap<isize, SceneCard>,
) -> RendererInput {
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
    let mut cursor = POINT::default();
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return RendererInput::RefreshRegion;
    }
    *ACTIVE_DRAG.lock().unwrap() = Some(ActiveDrag {
        id,
        targets,
        start: cursor,
        last: cursor,
        click_outcome,
    });
    BUTTON_REGIONS.lock().unwrap().clear();
    unsafe {
        let _ = SetCapture(host);
    }
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

pub(super) fn has_active_drag() -> bool {
    ACTIVE_DRAG.lock().unwrap().is_some()
}

pub(super) unsafe fn handle_mouse_move() -> bool {
    let mut active = ACTIVE_DRAG.lock().unwrap();
    let Some(drag) = active.as_mut() else {
        return false;
    };
    let mut cursor = POINT::default();
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return true;
    }
    let dx = cursor.x - drag.last.x;
    let dy = cursor.y - drag.last.y;
    if dx == 0 && dy == 0 {
        return true;
    }
    unsafe {
        move_targets(&drag.targets, dx, dy);
    }
    drag.last = cursor;
    true
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

pub(super) unsafe fn finish_drag() -> Option<ChildEvent> {
    let drag = ACTIVE_DRAG.lock().unwrap().take()?;
    unsafe {
        let _ = ReleaseCapture();
    }
    let mut cursor = POINT::default();
    let _ = unsafe { GetCursorPos(&mut cursor) };
    let dx = cursor.x - drag.start.x;
    let dy = cursor.y - drag.start.y;
    let outcome = if dx.saturating_mul(dx) + dy.saturating_mul(dy) < 25 {
        drag.click_outcome
    } else {
        DragOutcome::Moved
    };
    Some(ChildEvent::DragFinished {
        id: drag.id,
        targets: drag.targets,
        outcome,
    })
}

pub(super) unsafe fn recover_stale_drag(host: HWND) -> Option<ChildEvent> {
    let active = ACTIVE_DRAG.lock().unwrap();
    let drag = active.as_ref()?;
    let target = HWND(drag.id as *mut std::ffi::c_void);
    let valid = unsafe { IsWindow(Some(target)).as_bool() };
    let captured = unsafe { GetCapture() == host };
    let button_down = unsafe {
        GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0
            || GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0
            || GetAsyncKeyState(VK_MBUTTON.0 as i32) < 0
    };
    drop(active);
    if valid && captured && button_down {
        None
    } else {
        unsafe { finish_drag() }
    }
}

#[cfg(test)]
mod tests {
    use super::{button_action, interactive_regions, update_regions};
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
        assert_eq!(
            button_action(42, "dismiss_chain", &message),
            Some(ChildEvent::ButtonAction {
                id: 42,
                action: ButtonAction::DismissChain
            })
        );
        assert_eq!(
            button_action(42, "copy_all", &message),
            Some(ChildEvent::ButtonAction {
                id: 42,
                action: ButtonAction::CopyAll
            })
        );
    }

    #[test]
    fn selection_copy_is_not_a_parent_button_action() {
        let message = serde_json::json!({ "text": "selected result" });
        assert!(button_action(42, "copy_selection", &message).is_none());
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
