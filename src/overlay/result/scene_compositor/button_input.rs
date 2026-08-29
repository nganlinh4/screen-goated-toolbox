use super::protocol::{ButtonAction, ChildEvent, DragOutcome, SceneCard, SceneRect};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, GetWindowRect, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
};

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

#[derive(Clone, Copy)]
enum DragMode {
    One,
    Group,
    All,
}

struct ActiveDrag {
    id: isize,
    targets: Vec<DragTarget>,
    click_outcome: DragOutcome,
}

#[derive(Clone, Copy)]
struct DragTarget {
    id: isize,
    start_rect: RECT,
    live_native: bool,
}

#[derive(Clone, Copy)]
struct ResizeEdge {
    north: bool,
    south: bool,
    east: bool,
    west: bool,
}

struct ActiveResize {
    id: isize,
    edge: ResizeEdge,
    start_rect: RECT,
    live_native: bool,
}

static BUTTON_REGIONS: LazyLock<Mutex<Vec<SceneRect>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static ACTIVE_DRAG: Mutex<Option<ActiveDrag>> = Mutex::new(None);
static ACTIVE_RESIZE: Mutex<Option<ActiveResize>> = Mutex::new(None);
static EXTERNAL_DRAG: AtomicBool = AtomicBool::new(false);
static AWAITING_DRAG_SETTLE: AtomicBool = AtomicBool::new(false);
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
        "result_drag_start" => begin_drag(id, DragMode::One, cards),
        "result_group_drag_start" => begin_drag(id, DragMode::Group, cards),
        "result_all_drag_start" => begin_drag(id, DragMode::All, cards),
        "result_drag_preview" => preview_drag_from_message(&message),
        "result_drag_finish" => finish_drag_from_message(&message),
        "result_resize_start" => begin_resize(id, &message, cards),
        "result_resize_preview" => preview_resize_from_message(&message),
        "result_resize_finish" => finish_resize_from_message(&message),
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
    if ACTIVE_RESIZE.lock().unwrap().is_some() {
        return RendererInput::RefreshRegion;
    }
    let Some(card) = cards.get(&id) else {
        return RendererInput::RefreshRegion;
    };
    let mut target_ids = match mode {
        DragMode::One => vec![id],
        DragMode::Group => card.controls.group_ids.clone(),
        DragMode::All => cards.keys().copied().collect(),
    };
    if target_ids.is_empty() {
        target_ids.push(id);
    }
    let targets = target_ids
        .into_iter()
        .filter_map(|target| {
            let mut start_rect = RECT::default();
            unsafe { GetWindowRect(HWND(target as *mut std::ffi::c_void), &mut start_rect) }
                .ok()?;
            Some(DragTarget {
                id: target,
                start_rect,
                live_native: cards
                    .get(&target)
                    .is_some_and(|card| card.external_navigation),
            })
        })
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return RendererInput::RefreshRegion;
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
    let was_active = EXTERNAL_DRAG.swap(active, Ordering::SeqCst);
    if active {
        BUTTON_REGIONS.lock().unwrap().clear();
    } else if was_active {
        AWAITING_DRAG_SETTLE.store(true, Ordering::SeqCst);
    }
}

pub(super) fn settle_drag() {
    EXTERNAL_DRAG.store(false, Ordering::SeqCst);
    AWAITING_DRAG_SETTLE.store(false, Ordering::SeqCst);
}

pub(super) fn is_dragging() -> bool {
    EXTERNAL_DRAG.load(Ordering::SeqCst)
        || AWAITING_DRAG_SETTLE.load(Ordering::SeqCst)
        || ACTIVE_DRAG.lock().unwrap().is_some()
        || ACTIVE_RESIZE.lock().unwrap().is_some()
}

unsafe fn place_targets(targets: &[DragTarget], dx: i32, dy: i32, live_only: bool) {
    unsafe {
        let selected = targets
            .iter()
            .filter(|target| !live_only || target.live_native)
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return;
        }
        let Ok(mut batch) = BeginDeferWindowPos(selected.len() as i32) else {
            return;
        };
        for target in selected {
            let hwnd = HWND(target.id as *mut std::ffi::c_void);
            let (x, y) = translated_origin(target.start_rect, dx, dy);
            batch = DeferWindowPos(
                batch,
                hwnd,
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .unwrap_or(batch);
        }
        let _ = EndDeferWindowPos(batch);
    }
}

fn translated_origin(rect: RECT, dx: i32, dy: i32) -> (i32, i32) {
    (rect.left.saturating_add(dx), rect.top.saturating_add(dy))
}

fn finish_drag_with_offset(dx: i32, dy: i32) -> Option<ChildEvent> {
    let drag = ACTIVE_DRAG.lock().unwrap().take()?;
    AWAITING_DRAG_SETTLE.store(true, Ordering::SeqCst);
    let moved_distance = dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy));
    let outcome = if moved_distance < 25 {
        unsafe { place_targets(&drag.targets, 0, 0, true) };
        drag.click_outcome
    } else {
        unsafe { place_targets(&drag.targets, dx, dy, false) };
        DragOutcome::Moved
    };
    let targets = drag.targets.iter().map(|target| target.id).collect();
    Some(ChildEvent::DragFinished {
        id: drag.id,
        targets,
        outcome,
    })
}

fn preview_drag_from_message(message: &serde_json::Value) -> RendererInput {
    let (dx, dy) = drag_offset(message);
    if let Some(drag) = ACTIVE_DRAG.lock().unwrap().as_ref() {
        unsafe { place_targets(&drag.targets, dx, dy, true) };
    }
    RendererInput::Handled
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

fn begin_resize(
    id: isize,
    message: &serde_json::Value,
    cards: &HashMap<isize, SceneCard>,
) -> RendererInput {
    if ACTIVE_DRAG.lock().unwrap().is_some() || ACTIVE_RESIZE.lock().unwrap().is_some() {
        return RendererInput::RefreshRegion;
    }
    let Some(card) = cards.get(&id).filter(|card| card.visible) else {
        return RendererInput::RefreshRegion;
    };
    let Some(edge) = message
        .get("edge")
        .and_then(|value| value.as_str())
        .and_then(ResizeEdge::parse)
    else {
        return RendererInput::RefreshRegion;
    };
    let mut start_rect = RECT::default();
    if unsafe { GetWindowRect(HWND(id as *mut std::ffi::c_void), &mut start_rect) }.is_err() {
        return RendererInput::RefreshRegion;
    }
    *ACTIVE_RESIZE.lock().unwrap() = Some(ActiveResize {
        id,
        edge,
        start_rect,
        live_native: card.external_navigation,
    });
    BUTTON_REGIONS.lock().unwrap().clear();
    RendererInput::EventAndRefresh(ChildEvent::DragStarted)
}

fn finish_resize_from_message(message: &serde_json::Value) -> RendererInput {
    let Some(resize) = ACTIVE_RESIZE.lock().unwrap().take() else {
        return RendererInput::RefreshRegion;
    };
    AWAITING_DRAG_SETTLE.store(true, Ordering::SeqCst);
    let (dx, dy) = drag_offset(message);
    unsafe { place_resized_target(&resize, dx, dy) };
    RendererInput::EventAndRefresh(ChildEvent::DragFinished {
        id: resize.id,
        targets: vec![resize.id],
        outcome: DragOutcome::Moved,
    })
}

fn preview_resize_from_message(message: &serde_json::Value) -> RendererInput {
    let (dx, dy) = drag_offset(message);
    if let Some(resize) = ACTIVE_RESIZE.lock().unwrap().as_ref()
        && resize.live_native
    {
        unsafe { place_resized_target(resize, dx, dy) };
    }
    RendererInput::Handled
}

impl ResizeEdge {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "n" => Self::new(true, false, false, false),
            "s" => Self::new(false, true, false, false),
            "e" => Self::new(false, false, true, false),
            "w" => Self::new(false, false, false, true),
            "ne" => Self::new(true, false, true, false),
            "nw" => Self::new(true, false, false, true),
            "se" => Self::new(false, true, true, false),
            "sw" => Self::new(false, true, false, true),
            _ => return None,
        })
    }

    const fn new(north: bool, south: bool, east: bool, west: bool) -> Self {
        Self {
            north,
            south,
            east,
            west,
        }
    }
}

fn resized_rect(mut rect: RECT, edge: ResizeEdge, dx: i32, dy: i32) -> RECT {
    if edge.west {
        rect.left = rect.left.saturating_add(dx).min(
            rect.right
                .saturating_sub(super::super::event_handler::MIN_WINDOW_WIDTH),
        );
    }
    if edge.east {
        rect.right = rect.right.saturating_add(dx).max(
            rect.left
                .saturating_add(super::super::event_handler::MIN_WINDOW_WIDTH),
        );
    }
    if edge.north {
        rect.top = rect.top.saturating_add(dy).min(
            rect.bottom
                .saturating_sub(super::super::event_handler::MIN_WINDOW_HEIGHT),
        );
    }
    if edge.south {
        rect.bottom = rect.bottom.saturating_add(dy).max(
            rect.top
                .saturating_add(super::super::event_handler::MIN_WINDOW_HEIGHT),
        );
    }
    rect
}

unsafe fn place_resized_target(resize: &ActiveResize, dx: i32, dy: i32) {
    unsafe {
        let hwnd = HWND(resize.id as *mut std::ffi::c_void);
        let rect = resized_rect(resize.start_rect, resize.edge, dx, dy);
        let _ = SetWindowPos(
            hwnd,
            None,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RendererInput, ResizeEdge, button_action, drag_offset, handle_renderer_message,
        interactive_regions, resized_rect, translated_origin, update_regions,
    };
    use crate::overlay::result::scene_compositor::protocol::{ButtonAction, ChildEvent, SceneRect};
    use windows::Win32::Foundation::RECT;

    #[test]
    fn refinement_focus_messages_use_an_explicit_child_contract() {
        let cards = std::collections::HashMap::new();
        assert_eq!(
            handle_renderer_message(
                r#"{"action":"request_refine_focus","hwnd":"42"}"#,
                windows::Win32::Foundation::HWND::default(),
                &cards,
            ),
            RendererInput::FocusRefine { id: 42 }
        );
        assert_eq!(
            handle_renderer_message(
                r#"{"action":"release_refine_focus","hwnd":"0"}"#,
                windows::Win32::Foundation::HWND::default(),
                &cards,
            ),
            RendererInput::ReleaseRefineFocus
        );
    }

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
            button_action(
                42,
                "update_refine_draft",
                &serde_json::json!({ "text": "shorter" })
            ),
            Some(ChildEvent::ButtonAction {
                id: 42,
                action: ButtonAction::UpdateRefineDraft {
                    text: "shorter".to_string()
                }
            })
        );
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
        let rect = RECT {
            left: 500,
            top: -200,
            right: 900,
            bottom: 100,
        };
        assert_eq!(translated_origin(rect, 75, -25), (575, -225));
        assert_eq!(
            translated_origin(rect, i32::MAX, i32::MIN),
            (i32::MAX, i32::MIN)
        );
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

    #[test]
    fn compositor_resize_edges_preserve_the_opposite_edge_and_native_minimum() {
        let source = include_str!("button_input.rs");
        let rect = RECT {
            left: 100,
            top: 200,
            right: 500,
            bottom: 500,
        };
        let resized = resized_rect(rect, ResizeEdge::parse("nw").unwrap(), 80, 50);
        assert_eq!((resized.left, resized.top), (180, 250));
        assert_eq!((resized.right, resized.bottom), (500, 500));

        let minimum = resized_rect(rect, ResizeEdge::parse("se").unwrap(), -10_000, -10_000);
        assert_eq!(
            minimum.right - minimum.left,
            crate::overlay::result::event_handler::MIN_WINDOW_WIDTH
        );
        assert_eq!(
            minimum.bottom - minimum.top,
            crate::overlay::result::event_handler::MIN_WINDOW_HEIGHT
        );
        assert!(ResizeEdge::parse("center").is_none());
        assert!(source.contains("result_resize_preview"));
        assert!(source.contains("resized_rect(resize.start_rect"));
    }
}
