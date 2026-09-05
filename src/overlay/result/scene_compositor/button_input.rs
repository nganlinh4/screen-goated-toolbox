use super::protocol::{ButtonAction, ChildEvent, SceneCard, SceneRect};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LBUTTON, VK_MBUTTON, VK_RBUTTON};

pub(super) use super::gesture::DragMode;
#[cfg(test)]
pub(super) use super::gesture::ResizeEdge;
#[cfg(test)]
pub(super) use super::gesture::geometry::{resized_rect, translated_origin};

#[derive(Debug, PartialEq)]
pub(super) enum RendererInput {
    Unhandled,
    Handled,
    RefreshRegion,
    FocusRefine { id: isize },
    ReleaseRefineFocus,
    Event(ChildEvent),
    EventAndRefresh(ChildEvent),
}

static BUTTON_REGIONS: LazyLock<Mutex<Vec<SceneRect>>> = LazyLock::new(|| Mutex::new(Vec::new()));

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
    if action == "restore_clickable_regions" {
        settle_drag(gesture_id(&message));
        update_regions(&message);
        return RendererInput::RefreshRegion;
    }
    if action == "update_clickable_regions" {
        update_regions(&message);
        return RendererInput::RefreshRegion;
    }
    if action == "release_refine_focus" {
        return RendererInput::ReleaseRefineFocus;
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
        "request_refine_focus" => RendererInput::FocusRefine { id },
        "result_drag_start" => begin_drag(id, DragMode::One, &message, cards, host),
        "result_group_drag_start" => begin_drag(id, DragMode::Group, &message, cards, host),
        "result_all_drag_start" => begin_drag(id, DragMode::All, &message, cards, host),
        "result_drag_preview" => preview_drag_from_message(&message),
        "result_drag_finish" => finish_drag_from_message(&message),
        "result_resize_start" => begin_resize(id, &message, cards, host),
        "result_resize_preview" => preview_resize_from_message(&message),
        "result_resize_finish" => finish_resize_from_message(&message),
        _ => button_action(id, action, &message)
            .map(RendererInput::Event)
            .unwrap_or(RendererInput::RefreshRegion),
    }
}

fn gesture_id(message: &serde_json::Value) -> Option<u64> {
    message
        .get("gesture_id")
        .and_then(|value| value.as_u64())
        .filter(|id| *id != 0)
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
        "update_refine_draft" => ButtonAction::UpdateRefineDraft { text: text() },
        "submit_refine" => ButtonAction::SubmitRefine { text: text() },
        "cancel_refine" => ButtonAction::CancelRefine,
        "history_up_refine" => ButtonAction::HistoryUpRefine { text: text() },
        "history_down_refine" => ButtonAction::HistoryDownRefine { text: text() },
        "mic" => ButtonAction::Mic,
        _ => return None,
    };
    Some(ChildEvent::ButtonAction { id, action })
}

fn drag_offset(message: &serde_json::Value) -> (i32, i32) {
    let dx = message
        .get("dx")
        .and_then(|value| value.as_i64())
        .map(|value| value.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
        .unwrap_or(0);
    let dy = message
        .get("dy")
        .and_then(|value| value.as_i64())
        .map(|value| value.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
        .unwrap_or(0);
    (dx, dy)
}

fn update_regions(message: &serde_json::Value) {
    let scale = message
        .get("scale")
        .and_then(|value| value.as_f64())
        .unwrap_or(1.0);
    let Some(raw_regions) = message.get("regions").and_then(|value| value.as_array()) else {
        return;
    };
    let regions = raw_regions
        .iter()
        .filter_map(|raw| {
            let number = |key: &str| raw.get(key).and_then(|value| value.as_f64());
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

fn pointer_origin(message: &serde_json::Value, host: HWND, button: u32) -> Option<POINT> {
    let coordinate = |key| i32::try_from(message.get(key)?.as_i64()?).ok();
    super::pointer_input::claim(
        button,
        host,
        POINT {
            x: coordinate("origin_x")?,
            y: coordinate("origin_y")?,
        },
    )
}

fn begin_drag(
    id: isize,
    mode: DragMode,
    message: &serde_json::Value,
    cards: &HashMap<isize, SceneCard>,
    host: HWND,
) -> RendererInput {
    let Some(gesture_id) = gesture_id(message) else {
        return RendererInput::RefreshRegion;
    };
    if is_dragging() {
        return RendererInput::RefreshRegion;
    }
    let initiating_btn = match message.get("button").and_then(|v| v.as_u64()).unwrap_or(0) {
        1 => VK_MBUTTON.0 as u32,
        2 => VK_RBUTTON.0 as u32,
        _ => VK_LBUTTON.0 as u32,
    };
    let Some(origin) = pointer_origin(message, host, initiating_btn) else {
        return RendererInput::RefreshRegion;
    };
    if let Some(event) =
        super::gesture::begin_drag(id, mode, gesture_id, initiating_btn, origin, host, cards)
    {
        BUTTON_REGIONS.lock().unwrap().clear();
        RendererInput::EventAndRefresh(event)
    } else {
        RendererInput::RefreshRegion
    }
}

fn begin_resize(
    id: isize,
    message: &serde_json::Value,
    cards: &HashMap<isize, SceneCard>,
    host: HWND,
) -> RendererInput {
    let Some(gesture_id) = gesture_id(message) else {
        return RendererInput::RefreshRegion;
    };
    if is_dragging() {
        return RendererInput::RefreshRegion;
    }
    let Some(edge_str) = message.get("edge").and_then(|v| v.as_str()) else {
        return RendererInput::RefreshRegion;
    };
    let initiating_btn = match message.get("button").and_then(|v| v.as_u64()).unwrap_or(0) {
        1 => VK_MBUTTON.0 as u32,
        2 => VK_RBUTTON.0 as u32,
        _ => VK_LBUTTON.0 as u32,
    };
    let Some(origin) = pointer_origin(message, host, initiating_btn) else {
        return RendererInput::RefreshRegion;
    };
    if let Some(event) = super::gesture::begin_resize(
        id,
        edge_str,
        gesture_id,
        initiating_btn,
        origin,
        host,
        cards,
    ) {
        BUTTON_REGIONS.lock().unwrap().clear();
        RendererInput::EventAndRefresh(event)
    } else {
        RendererInput::RefreshRegion
    }
}

pub(super) fn interactive_regions() -> Vec<SceneRect> {
    if is_dragging() {
        Vec::new()
    } else {
        BUTTON_REGIONS.lock().unwrap().clone()
    }
}

static AWAITING_DRAG_SETTLE: AtomicU64 = AtomicU64::new(0);

pub(super) fn set_external_drag(_active: bool) {}

pub(super) fn captures_desktop_input() -> bool {
    super::gesture::is_gesture_active()
}

pub(super) fn settle_drag(gesture_id: Option<u64>) -> bool {
    gesture_id.is_none_or(|id| {
        AWAITING_DRAG_SETTLE
            .compare_exchange(id, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    })
}

pub(super) fn await_settlement(gesture_id: u64) {
    AWAITING_DRAG_SETTLE.store(gesture_id, Ordering::SeqCst);
}

pub(super) fn is_dragging() -> bool {
    super::gesture::is_gesture_active()
}

pub(super) fn reconcile_released_pointer() -> Option<ChildEvent> {
    super::pointer_input::reconcile();
    super::gesture::dispatch_pending_gesture(1);
    if super::gesture::initiating_button_is_down() {
        return None;
    }
    if super::gesture::is_gesture_active() {
        return super::gesture::cancel_active_gesture();
    }
    None
}

#[cfg(test)]
pub(super) fn offset_from_points(start: POINT, current: POINT) -> (i32, i32) {
    (
        current.x.saturating_sub(start.x),
        current.y.saturating_sub(start.y),
    )
}

pub(super) fn cancel_removed_card(id: isize) {
    super::gesture::cancel_removed_card(id);
}

pub(super) fn cancel_missing_cards(cards: &HashMap<isize, SceneCard>) {
    super::gesture::cancel_missing_cards(cards);
}

fn preview_drag_from_message(message: &serde_json::Value) -> RendererInput {
    let Some(gesture_id) = gesture_id(message) else {
        return RendererInput::Handled;
    };
    let (dx, dy) = drag_offset(message);
    super::gesture::preview_gesture(gesture_id, dx, dy);
    RendererInput::Handled
}

fn finish_drag_from_message(message: &serde_json::Value) -> RendererInput {
    let Some(gesture_id) = gesture_id(message) else {
        return RendererInput::RefreshRegion;
    };
    if message.get("cancelled").and_then(|value| value.as_bool()) == Some(true) {
        return if super::gesture::active_gesture_id() == Some(gesture_id) {
            super::gesture::cancel_active_gesture()
                .map(RendererInput::EventAndRefresh)
                .unwrap_or(RendererInput::RefreshRegion)
        } else {
            RendererInput::RefreshRegion
        };
    }
    let (dx, dy) = drag_offset(message);
    super::gesture::finish_gesture_with_offset(gesture_id, dx, dy)
        .map(RendererInput::EventAndRefresh)
        .unwrap_or(RendererInput::RefreshRegion)
}

fn preview_resize_from_message(message: &serde_json::Value) -> RendererInput {
    let Some(gesture_id) = gesture_id(message) else {
        return RendererInput::Handled;
    };
    let (dx, dy) = drag_offset(message);
    super::gesture::preview_gesture(gesture_id, dx, dy);
    RendererInput::Handled
}

fn finish_resize_from_message(message: &serde_json::Value) -> RendererInput {
    finish_drag_from_message(message)
}

#[cfg(test)]
#[path = "button_input_tests.rs"]
mod tests;
