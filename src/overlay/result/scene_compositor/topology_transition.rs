//! Hide input while the renderer crosses a desktop-coordinate epoch.
use super::super::protocol::{SceneControlUpdate, SceneGeometry};
use super::{
    CARDS, ChildEvent, HOST, HostCommand, INPUT_SURFACE_HWND, RENDERER_READY, command_delivery,
    topology,
};
use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Default)]
struct Transition {
    epoch: u64,
    started: Option<Instant>,
    applying: bool,
    pending: bool,
    acknowledgements: Vec<ChildEvent>,
}

thread_local! {
    static STATE: RefCell<Transition> = RefCell::new(Transition { epoch: 1, ..Transition::default() });
}

pub(in crate::overlay::result::scene_compositor) fn active() -> bool {
    STATE.with(|state| state.borrow().started.is_some())
}

pub(in crate::overlay::result::scene_compositor) fn begin(hwnd: HWND) {
    super::cursor_poll::invalidate();
    super::super::visual_region::set_tooltip(None);
    let first = STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.started.is_some() {
            state.pending = true;
            false
        } else {
            state.started = Some(Instant::now());
            state.applying = false;
            true
        }
    });
    if !first {
        return;
    }
    super::emit_event(ChildEvent::TopologyHold { active: true });
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
        let input = INPUT_SURFACE_HWND.load(Ordering::SeqCst);
        if input != 0 {
            let _ = ShowWindow(HWND(input as *mut _), SW_HIDE);
        }
    }
}

pub(super) fn hold_ack(event: ChildEvent) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.acknowledgements.len() >= 1024 {
            crate::crash_report::exit(1, "renderer-topology-ack-backlog");
        }
        state.acknowledgements.push(event);
    });
}

pub(super) fn accepts_epoch(value: &serde_json::Value) -> bool {
    STATE.with(|state| {
        value.get("topology_epoch").and_then(|v| v.as_str())
            == Some(state.borrow().epoch.to_string().as_str())
    })
}

pub(super) fn tick(hwnd: HWND) {
    let proceed = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(started) = state.started else {
            return false;
        };
        if started.elapsed() > Duration::from_secs(5) {
            crate::crash_report::exit(1, "renderer-topology-timeout");
        }
        if state.applying
            || !RENDERER_READY.load(Ordering::SeqCst)
            || !command_delivery::is_idle()
            || super::super::region::is_updating()
            || HOST.with(|slot| slot.try_borrow().is_err())
        {
            return false;
        }
        state.applying = true;
        true
    });
    if !proceed {
        return;
    }
    super::super::pointer_input::cancel();
    if let Some(event) = super::super::gesture::cancel_active_gesture() {
        super::emit_event(event);
    }
    super::super::button_input::clear_regions();
    if !super::set_renderer_visible(hwnd, true) {
        crate::crash_report::exit(1, "renderer-topology-visibility-failed");
    }
    apply(hwnd).unwrap_or_else(|| crate::crash_report::exit(1, "renderer-topology-rebase-failed"));
}

fn apply(hwnd: HWND) -> Option<()> {
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) };
    let x = super::super::compositor_host_x(unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) }, width);
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE,
        )
        .ok()?;
    }
    let mut rect = RECT::default();
    unsafe {
        GetWindowRect(hwnd, &mut rect).ok()?;
    }
    if !topology::matches_requested_bounds(rect, x, y, width, height) {
        return None;
    }
    let new = topology::coordinate_origin(
        topology::Origin {
            x: rect.left,
            y: rect.top,
        },
        topology::Origin {
            x: unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) },
            y,
        },
        super::super::acceptance_offscreen(),
    );
    let old = topology::origin();
    let dx = old.x.checked_sub(new.x)?;
    let dy = old.y.checked_sub(new.y)?;
    let (geometry, controls) = {
        let mut cards = CARDS.lock().unwrap();
        topology::rebase_cards(&mut cards, old, new)?;
        let geometry = cards
            .values()
            .map(|card| SceneGeometry {
                id: card.id,
                rect: card.rect.clone(),
                control_rect: card.control_rect.clone(),
                visible: card.visible,
            })
            .collect();
        let controls = cards
            .values()
            .map(|card| SceneControlUpdate {
                id: card.id,
                controls: card.controls.clone(),
            })
            .collect();
        (geometry, controls)
    };
    let effects = super::super::processing::rebase_child(dx, dy)?;
    topology::set_origin(new);
    topology::set_size(width, height);
    HOST.with(|slot| {
        slot.borrow()
            .as_ref()?
            .update_display(width, height, 1.0)
            .ok()
    })?;
    let input = INPUT_SURFACE_HWND.load(Ordering::SeqCst);
    if input == 0 {
        return None;
    }
    super::super::input_surface::sync_input_surface_bounds(
        HWND(input as *mut _),
        rect.left,
        rect.top,
        width,
        height,
    )
    .ok()?;
    if !topology::native_bounds_match(HWND(input as *mut _), rect.left, rect.top, width, height) {
        return None;
    }
    let epoch = STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.epoch = state.epoch.checked_add(1)?;
        Some(state.epoch)
    })?;
    let mut scripts = super::super::control_surface::monitor_work_areas_script((new.x, new.y));
    for command in [
        HostCommand::Geometry { cards: geometry },
        HostCommand::Controls { cards: controls },
        HostCommand::Processing { effects },
    ] {
        scripts.push_str(&format!(
            "window.applyHostCommand({});",
            serde_json::to_string(&command).ok()?
        ));
    }
    super::evaluate_script(&format!(
        "window.__SGT_REBASE_TOPOLOGY__('{epoch}',function(){{{scripts}}});"
    ));
    Some(())
}

pub(super) fn ready(hwnd: HWND, epoch: &str) {
    let (again, acknowledgements) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.started.is_none() || !state.applying || epoch != state.epoch.to_string() {
            return (false, None);
        }
        if state.pending {
            state.pending = false;
            state.applying = false;
            state.started = Some(Instant::now());
            (true, None)
        } else {
            state.started = None;
            (false, Some(std::mem::take(&mut state.acknowledgements)))
        }
    });
    let Some(acknowledgements) = acknowledgements else {
        if again {
            tick(hwnd);
        }
        return;
    };
    if CARDS.lock().unwrap().values().any(|card| card.visible)
        || super::super::processing::is_visible()
    {
        super::super::visual_region::mark_first_scene_painted();
    }
    super::super::region::update(hwnd);
    if active() {
        for event in acknowledgements {
            hold_ack(event);
        }
        return;
    }
    super::emit_event(ChildEvent::TopologyHold { active: false });
    for event in acknowledgements {
        super::emit_event(event);
    }
    unsafe {
        let _ = PostMessageW(Some(hwnd), super::WM_DRAIN_COMMANDS, WPARAM(0), LPARAM(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn region_and_user_messages_require_the_current_epoch() {
        STATE.with(|state| state.borrow_mut().epoch = 12);
        assert!(accepts_epoch(&json!({"topology_epoch": "12"})));
        assert!(!accepts_epoch(&json!({"topology_epoch": "11"})));
        assert!(!accepts_epoch(&json!({"topology_epoch": 12})));
        assert!(!accepts_epoch(&json!({})));
    }
}
