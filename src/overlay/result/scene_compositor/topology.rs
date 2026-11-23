//! Coordinate conversion at the compositor child's desktop-origin boundary.
use super::super::protocol::{HostCommand, SceneCard, SceneControls, SceneGeometry, SceneRect};
use std::cell::Cell;
use std::collections::HashMap;

#[cfg(test)]
#[path = "topology_tests.rs"]
mod interleaving_tests;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Origin {
    pub(super) x: i32,
    pub(super) y: i32,
}

thread_local! {
    static ORIGIN: Cell<Origin> = const { Cell::new(Origin { x: 0, y: 0 }) };
    static SIZE: Cell<(i32, i32)> = const { Cell::new((0, 0)) };
}

pub(super) fn origin() -> Origin {
    ORIGIN.with(Cell::get)
}

pub(super) fn set_origin(origin: Origin) {
    ORIGIN.with(|value| value.set(origin));
}

pub(super) fn set_size(width: i32, height: i32) {
    SIZE.with(|size| size.set((width, height)));
}

pub(in crate::overlay::result::scene_compositor) fn matches_display(
    hwnd: windows::Win32::Foundation::HWND,
) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let desktop = Origin {
        x: unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) },
        y: unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) },
    };
    let size = (
        unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) },
        unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) },
    );
    if origin() != desktop || SIZE.with(Cell::get) != size {
        return false;
    }
    let x = super::super::compositor_host_x(desktop.x, size.0);
    let input = super::INPUT_SURFACE_HWND.load(std::sync::atomic::Ordering::SeqCst);
    input != 0
        && native_bounds_match(hwnd, x, desktop.y, size.0, size.1)
        && native_bounds_match(
            windows::Win32::Foundation::HWND(input as *mut _),
            x,
            desktop.y,
            size.0,
            size.1,
        )
}

pub(in crate::overlay::result::scene_compositor) fn native_bounds_match(
    hwnd: windows::Win32::Foundation::HWND,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> bool {
    let mut rect = windows::Win32::Foundation::RECT::default();
    let queried =
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect).is_ok() };
    queried && matches_requested_bounds(rect, x, y, width, height)
}

/// Off-screen acceptance keeps the ordinary desktop coordinate system even
/// though its native host is parked outside the desktop.
pub(super) fn coordinate_origin(
    physical: Origin,
    virtual_origin: Origin,
    offscreen: bool,
) -> Origin {
    if offscreen { virtual_origin } else { physical }
}

pub(super) fn matches_requested_bounds(
    rect: windows::Win32::Foundation::RECT,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> bool {
    rect.left == x
        && rect.top == y
        && rect.right.checked_sub(rect.left) == Some(width)
        && rect.bottom.checked_sub(rect.top) == Some(height)
}

fn translate_rect(rect: &mut SceneRect, dx: i64, dy: i64) -> Option<()> {
    rect.x = i32::try_from(i64::from(rect.x) + dx).ok()?;
    rect.y = i32::try_from(i64::from(rect.y) + dy).ok()?;
    Some(())
}

fn translate_controls(controls: &mut SceneControls, dx: i64, dy: i64) -> Option<()> {
    if let Some(anchor) = &mut controls.control_anchor {
        anchor[0] = i32::try_from(i64::from(anchor[0]) + dx).ok()?;
        anchor[1] = i32::try_from(i64::from(anchor[1]) + dy).ok()?;
    }
    Some(())
}

fn translate_card(card: &mut SceneCard, dx: i64, dy: i64) -> Option<()> {
    translate_rect(&mut card.rect, dx, dy)?;
    translate_rect(&mut card.control_rect, dx, dy)?;
    translate_controls(&mut card.controls, dx, dy)
}

fn translate_geometry(card: &mut SceneGeometry, dx: i64, dy: i64) -> Option<()> {
    translate_rect(&mut card.rect, dx, dy)?;
    translate_rect(&mut card.control_rect, dx, dy)
}

/// Parent commands use physical screen coordinates; the child and page use host-local coordinates.
pub(super) fn localize(mut command: HostCommand, origin: Origin) -> Option<HostCommand> {
    let dx = -i64::from(origin.x);
    let dy = -i64::from(origin.y);
    match &mut command {
        HostCommand::Snapshot { cards } | HostCommand::UpsertBatch { cards } => {
            for card in cards {
                translate_card(card, dx, dy)?;
            }
        }
        HostCommand::Upsert { card } => translate_card(card, dx, dy)?,
        HostCommand::Geometry { cards } | HostCommand::DragSettled { cards, .. } => {
            for card in cards {
                translate_geometry(card, dx, dy)?;
            }
        }
        HostCommand::Controls { cards } => {
            for card in cards {
                translate_controls(&mut card.controls, dx, dy)?;
            }
        }
        HostCommand::Stream { card } => translate_controls(&mut card.controls, dx, dy)?,
        HostCommand::Finalize { card } => translate_controls(&mut card.controls, dx, dy)?,
        HostCommand::Processing { effects } => {
            for effect in effects {
                translate_rect(&mut effect.rect, dx, dy)?;
            }
        }
        _ => {}
    }
    Some(command)
}

/// Rebase cached local cards without exposing a partially translated state.
pub(super) fn rebase_cards(
    cards: &mut HashMap<isize, SceneCard>,
    old: Origin,
    new: Origin,
) -> Option<()> {
    let dx = i64::from(old.x) - i64::from(new.x);
    let dy = i64::from(old.y) - i64::from(new.y);
    let mut updated = cards.clone();
    for card in updated.values_mut() {
        translate_card(card, dx, dy)?;
    }
    *cards = updated;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::super::super::protocol::ProcessingVisual;
    use super::*;

    #[test]
    fn offscreen_host_keeps_desktop_coordinates_for_its_scene() {
        let physical = Origin { x: -9000, y: -100 };
        let desktop = Origin { x: -1920, y: -100 };
        assert_eq!(coordinate_origin(physical, desktop, true), desktop);
        assert_eq!(coordinate_origin(physical, desktop, false), physical);
    }

    #[test]
    fn native_bounds_must_match_the_requested_desktop() {
        let rect = windows::Win32::Foundation::RECT {
            left: -1920,
            top: -100,
            right: 1920,
            bottom: 980,
        };
        assert!(matches_requested_bounds(rect, -1920, -100, 3840, 1080));
        assert!(!matches_requested_bounds(rect, -1921, -100, 3840, 1080));
        assert!(!matches_requested_bounds(rect, -1920, -100, 3840, 1079));
    }

    #[test]
    fn absolute_geometry_is_localized_without_shifting_effect_cells() {
        let command = HostCommand::Processing {
            effects: vec![ProcessingVisual {
                id: 1,
                control_id: None,
                rect: SceneRect {
                    x: -1800,
                    y: 140,
                    width: 100,
                    height: 60,
                },
                cells: vec![[2, 3, 4, 5]],
                revision: 0,
                elapsed_ms: 0,
                closing: false,
                finish: false,
            }],
        };
        let HostCommand::Processing { effects } =
            localize(command, Origin { x: -1920, y: 100 }).unwrap()
        else {
            panic!("wrong command");
        };
        assert_eq!((effects[0].rect.x, effects[0].rect.y), (120, 40));
        assert_eq!(effects[0].cells, vec![[2, 3, 4, 5]]);
    }

    #[test]
    fn geometry_and_anchor_use_the_same_origin() {
        let controls = SceneControls {
            control_anchor: Some([-1800, 130, 30, 20]),
            ..Default::default()
        };
        let command = HostCommand::Controls {
            cards: vec![super::super::super::protocol::SceneControlUpdate { id: 2, controls }],
        };
        let HostCommand::Controls { cards } =
            localize(command, Origin { x: -1920, y: 100 }).unwrap()
        else {
            panic!("wrong command");
        };
        assert_eq!(cards[0].controls.control_anchor, Some([120, 30, 30, 20]));
    }
}
