use super::protocol::{ChildEvent, DragOutcome, SceneCard, SceneRect};
use std::collections::HashMap;
use std::sync::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_MBUTTON, VK_RBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, WM_LBUTTONUP, WM_MBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONUP,
};

#[path = "button_geometry.rs"]
pub(crate) mod geometry;
use geometry::{place_resized_target, place_targets, resized_rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DragMode {
    One,
    Group,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DragTarget {
    pub id: isize,
    pub start_rect: RECT,
    pub live_native: bool,
    pub native_backed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ResizeEdge {
    pub north: bool,
    pub south: bool,
    pub east: bool,
    pub west: bool,
}

#[derive(Clone, Debug)]
pub(super) enum GestureKind {
    Drag,
    Resize {
        edge: ResizeEdge,
        start_rect: RECT,
        live_native: bool,
        native_backed: bool,
    },
}

#[derive(Clone, Debug)]
pub(super) struct ActiveGesture {
    pub gesture_id: u64,
    pub owning_card: isize,
    pub kind: GestureKind,
    pub initiating_button: u32,
    pub targets: Vec<DragTarget>,
    pub click_outcome: DragOutcome,
    pub start_cursor: POINT,
    pub last_preview: (i32, i32),
}

pub(super) struct ActiveResize {
    pub id: isize,
    pub edge: ResizeEdge,
    pub start_rect: RECT,
    pub native_backed: bool,
}

static ACTIVE_GESTURE: Mutex<Option<ActiveGesture>> = Mutex::new(None);

pub(super) fn is_gesture_active() -> bool {
    ACTIVE_GESTURE.lock().unwrap().is_some()
}

pub(super) fn active_gesture_id() -> Option<u64> {
    ACTIVE_GESTURE
        .lock()
        .unwrap()
        .as_ref()
        .map(|g| g.gesture_id)
}

pub(super) fn begin_drag(
    id: isize,
    mode: DragMode,
    gesture_id: u64,
    initiating_button: u32,
    start_cursor: POINT,
    input_hwnd: HWND,
    cards: &HashMap<isize, SceneCard>,
) -> Option<ChildEvent> {
    if is_gesture_active() {
        cancel_active_gesture();
    }
    let card = cards.get(&id)?;
    if !card.visible {
        return None;
    }

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
            let target_card = cards.get(&target)?;
            let native_backed = target > 0;
            let start_rect = if native_backed {
                let mut rect = RECT::default();
                unsafe { GetWindowRect(HWND(target as *mut std::ffi::c_void), &mut rect) }.ok()?;
                rect
            } else {
                RECT {
                    left: target_card.control_rect.x,
                    top: target_card.control_rect.y,
                    right: target_card
                        .control_rect
                        .x
                        .saturating_add(target_card.control_rect.width),
                    bottom: target_card
                        .control_rect
                        .y
                        .saturating_add(target_card.control_rect.height),
                }
            };
            Some(DragTarget {
                id: target,
                start_rect,
                live_native: target_card.external_navigation,
                native_backed,
            })
        })
        .collect::<Vec<_>>();

    if targets.is_empty() {
        return None;
    }

    let click_outcome = match mode {
        DragMode::One => DragOutcome::CloseOne,
        DragMode::Group => DragOutcome::CloseGroup,
        DragMode::All => DragOutcome::CloseAll,
    };

    let active = ActiveGesture {
        gesture_id,
        owning_card: id,
        kind: GestureKind::Drag,
        initiating_button,
        targets,
        click_outcome,
        start_cursor,
        last_preview: (0, 0),
    };

    *ACTIVE_GESTURE.lock().unwrap() = Some(active);
    start_observation(input_hwnd);
    Some(ChildEvent::DragStarted { gesture_id })
}

pub(super) fn begin_resize(
    id: isize,
    edge_str: &str,
    gesture_id: u64,
    initiating_button: u32,
    start_cursor: POINT,
    input_hwnd: HWND,
    cards: &HashMap<isize, SceneCard>,
) -> Option<ChildEvent> {
    if is_gesture_active() {
        cancel_active_gesture();
    }
    let card = cards
        .get(&id)
        .filter(|card| card.visible && !card.source_replacement)?;
    let edge = ResizeEdge::parse(edge_str)?;

    let native_backed = id > 0;
    let start_rect = if native_backed {
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(HWND(id as *mut std::ffi::c_void), &mut rect) }.is_err() {
            return None;
        }
        rect
    } else {
        RECT {
            left: card.rect.x,
            top: card.rect.y,
            right: card.rect.x.saturating_add(card.rect.width),
            bottom: card.rect.y.saturating_add(card.rect.height),
        }
    };

    let active = ActiveGesture {
        gesture_id,
        owning_card: id,
        kind: GestureKind::Resize {
            edge,
            start_rect,
            live_native: card.external_navigation,
            native_backed,
        },
        initiating_button,
        targets: vec![DragTarget {
            id,
            start_rect,
            live_native: card.external_navigation,
            native_backed,
        }],
        click_outcome: DragOutcome::Moved,
        start_cursor,
        last_preview: (0, 0),
    };

    *ACTIVE_GESTURE.lock().unwrap() = Some(active);
    start_observation(input_hwnd);
    Some(ChildEvent::DragStarted { gesture_id })
}

enum PreviewSpec {
    Drag(Vec<DragTarget>),
    Resize(ActiveResize),
}

pub(super) fn preview_gesture(gesture_id: u64, dx: i32, dy: i32) {
    let spec = {
        let mut lock = ACTIVE_GESTURE.lock().unwrap();
        let Some(active) = lock.as_mut().filter(|g| g.gesture_id == gesture_id) else {
            return;
        };
        if active.last_preview == (dx, dy) {
            return;
        }
        active.last_preview = (dx, dy);
        match &active.kind {
            GestureKind::Drag => Some(PreviewSpec::Drag(active.targets.clone())),
            GestureKind::Resize {
                edge,
                start_rect,
                live_native,
                native_backed,
            } => {
                if *live_native && *native_backed {
                    Some(PreviewSpec::Resize(ActiveResize {
                        id: active.owning_card,
                        edge: *edge,
                        start_rect: *start_rect,
                        native_backed: *native_backed,
                    }))
                } else {
                    None
                }
            }
        }
    };

    // Native window positioning executed outside the ACTIVE_GESTURE lock
    match spec {
        Some(PreviewSpec::Drag(targets)) => unsafe { place_targets(&targets, dx, dy, true) },
        Some(PreviewSpec::Resize(resize_spec)) => unsafe {
            place_resized_target(&resize_spec, dx, dy)
        },
        None => {}
    }
}

pub(super) fn finish_gesture_with_offset(gesture_id: u64, dx: i32, dy: i32) -> Option<ChildEvent> {
    let active = {
        let mut lock = ACTIVE_GESTURE.lock().unwrap();
        if !lock.as_ref().is_some_and(|g| g.gesture_id == gesture_id) {
            // Identity mismatch: stale finish cannot destroy an active newer gesture or uninstall its hook
            return None;
        }
        lock.take().unwrap()
    };
    stop_observation();
    super::button_input::await_settlement(gesture_id);

    match active.kind {
        GestureKind::Drag => {
            let moved_distance = dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy));
            let outcome = if moved_distance < 25 {
                unsafe { place_targets(&active.targets, 0, 0, true) };
                active.click_outcome
            } else {
                unsafe { place_targets(&active.targets, dx, dy, false) };
                DragOutcome::Moved
            };
            let targets = active.targets.iter().map(|target| target.id).collect();
            Some(ChildEvent::DragFinished {
                gesture_id,
                id: active.owning_card,
                targets,
                outcome,
                dx,
                dy,
            })
        }
        GestureKind::Resize {
            edge,
            start_rect,
            native_backed,
            ..
        } => {
            let rect = resized_rect(start_rect, edge, dx, dy);
            if !native_backed {
                Some(ChildEvent::ResizeFinished {
                    gesture_id,
                    id: active.owning_card,
                    rect: SceneRect {
                        x: rect.left,
                        y: rect.top,
                        width: rect.right.saturating_sub(rect.left).max(1),
                        height: rect.bottom.saturating_sub(rect.top).max(1),
                    },
                })
            } else {
                let resize_spec = ActiveResize {
                    id: active.owning_card,
                    edge,
                    start_rect,
                    native_backed,
                };
                unsafe { place_resized_target(&resize_spec, dx, dy) };
                Some(ChildEvent::DragFinished {
                    gesture_id,
                    id: active.owning_card,
                    targets: vec![active.owning_card],
                    outcome: DragOutcome::Moved,
                    dx: 0,
                    dy: 0,
                })
            }
        }
    }
}

pub(super) fn cancel_active_gesture() -> Option<ChildEvent> {
    let active = {
        let mut lock = ACTIVE_GESTURE.lock().unwrap();
        lock.take()?
    };
    stop_observation();

    match active.kind {
        GestureKind::Drag => {
            unsafe { place_targets(&active.targets, 0, 0, true) };
            Some(ChildEvent::DragFinished {
                gesture_id: active.gesture_id,
                id: active.owning_card,
                targets: active.targets.iter().map(|t| t.id).collect(),
                outcome: DragOutcome::Cancelled,
                dx: 0,
                dy: 0,
            })
        }
        GestureKind::Resize {
            edge, start_rect, ..
        } => {
            let rect = resized_rect(start_rect, edge, 0, 0);
            Some(ChildEvent::ResizeFinished {
                gesture_id: active.gesture_id,
                id: active.owning_card,
                rect: SceneRect {
                    x: rect.left,
                    y: rect.top,
                    width: rect.right.saturating_sub(rect.left).max(1),
                    height: rect.bottom.saturating_sub(rect.top).max(1),
                },
            })
        }
    }
}

pub(super) fn on_capture_lost() {
    if let Some(event) = cancel_active_gesture() {
        super::child::emit_event(event);
    }
}

pub(super) fn cancel_removed_card(id: isize) {
    let should_cancel = {
        let lock = ACTIVE_GESTURE.lock().unwrap();
        lock.as_ref()
            .is_some_and(|g| g.owning_card == id || g.targets.iter().any(|t| t.id == id))
    };
    if should_cancel {
        cancel_active_gesture();
    }
}

pub(super) fn cancel_missing_cards(cards: &HashMap<isize, SceneCard>) {
    let should_cancel = {
        let lock = ACTIVE_GESTURE.lock().unwrap();
        lock.as_ref().is_some_and(|g| {
            !cards.contains_key(&g.owning_card)
                || g.targets.iter().any(|t| !cards.contains_key(&t.id))
        })
    };
    if should_cancel {
        cancel_active_gesture();
    }
}

static PENDING_COALESCED_MOVE: Mutex<Option<(u64, i32, i32)>> = Mutex::new(None);
static PENDING_FINISH: Mutex<Option<(u64, i32, i32)>> = Mutex::new(None);
static INPUT_SURFACE_HWND_FOR_HOOK: std::sync::atomic::AtomicIsize =
    std::sync::atomic::AtomicIsize::new(0);
pub(super) const WM_APP_GESTURE_DISPATCH: u32 =
    windows::Win32::UI::WindowsAndMessaging::WM_APP + 42;

fn start_observation(hwnd: HWND) {
    INPUT_SURFACE_HWND_FOR_HOOK.store(hwnd.0 as isize, std::sync::atomic::Ordering::SeqCst);
    super::pointer_input::replay();
}

fn stop_observation() {
    PENDING_COALESCED_MOVE.lock().unwrap().take();
    PENDING_FINISH.lock().unwrap().take();
}

pub(super) fn observe_mouse_event(message: u32, pt: POINT) {
    let Ok(active) = ACTIVE_GESTURE.try_lock() else {
        return;
    };
    let Some(active) = active.as_ref() else {
        return;
    };
    let initiating_up = match active.initiating_button {
        x if x == VK_RBUTTON.0 as u32 => WM_RBUTTONUP,
        x if x == VK_MBUTTON.0 as u32 => WM_MBUTTONUP,
        _ => WM_LBUTTONUP,
    };
    let slot = if message == WM_MOUSEMOVE {
        &PENDING_COALESCED_MOVE
    } else if message == initiating_up {
        &PENDING_FINISH
    } else {
        return;
    };
    let Ok(mut pending) = slot.try_lock() else {
        return;
    };
    let needs_notification = pending.is_none();
    *pending = Some((
        active.gesture_id,
        pt.x.saturating_sub(active.start_cursor.x),
        pt.y.saturating_sub(active.start_cursor.y),
    ));
    if !needs_notification {
        return;
    }
    let hwnd_val = INPUT_SURFACE_HWND_FOR_HOOK.load(std::sync::atomic::Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            if windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(HWND(hwnd_val as *mut std::ffi::c_void)),
                WM_APP_GESTURE_DISPATCH,
                WPARAM(usize::from(message != WM_MOUSEMOVE)),
                LPARAM(0),
            )
            .is_err()
            {
                pending.take();
            }
        }
    }
}

pub(super) fn dispatch_pending_gesture(kind: usize) {
    if kind == 0 {
        let pending = PENDING_COALESCED_MOVE.lock().unwrap().take();
        if let Some((gesture_id, dx, dy)) = pending {
            preview_gesture(gesture_id, dx, dy);
        }
    } else {
        let pending = PENDING_FINISH.lock().unwrap().take();
        if let Some((gesture_id, dx, dy)) = pending
            && let Some(event) = finish_gesture_with_offset(gesture_id, dx, dy)
        {
            super::child::emit_event(event);
        }
    }
}

pub(super) fn initiating_button_is_down() -> bool {
    let active = ACTIVE_GESTURE.lock().unwrap();
    active.as_ref().is_some_and(|active| unsafe {
        windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(
            active.initiating_button as i32,
        ) < 0
    })
}

#[cfg(test)]
#[path = "gesture_tests.rs"]
mod tests;
