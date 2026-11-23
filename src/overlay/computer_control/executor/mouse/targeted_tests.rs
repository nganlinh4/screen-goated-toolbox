use super::*;
use std::cell::Cell;
use windows::Win32::UI::Input::KeyboardAndMouse::{MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_MOVE};

#[test]
fn displacement_during_validation_rejects_action() {
    let cursor = Cell::new((40, 20));
    let result = admit_with(
        (40, 20),
        || false,
        || Ok(cursor.get()),
        || {
            cursor.set((41, 20));
            Ok(())
        },
    );
    assert!(result.is_err());
}

#[test]
fn displacement_or_topology_change_before_validation_never_calls_guard() {
    assert!(admit_with((1, 2), || false, || Ok((2, 2)), || panic!("displaced")).is_err());
    assert!(
        admit_with(
            (1, 2),
            || false,
            || Err(anyhow!("topology changed")),
            || panic!("stale desktop")
        )
        .is_err()
    );
}

#[test]
fn cancellation_or_stale_target_stops_before_dispatch() {
    let cancelled = Cell::new(false);
    assert!(
        !admit_with(
            (1, 2),
            || cancelled.get(),
            || Ok((1, 2)),
            || {
                cancelled.set(true);
                Ok(())
            }
        )
        .unwrap()
    );
    assert!(admit_with((1, 2), || false, || Ok((1, 2)), || Err(anyhow!("occluded"))).is_err());
}

#[test]
fn movement_record_retains_virtual_desktop_coordinates() {
    let geometry = Geometry {
        desktop: (-1000, -200, 2000, 1000),
        nominal: (0, 0),
    };
    let event = move_event(&geometry, (-500, 300)).unwrap();
    let mouse = unsafe { event.Anonymous.mi };
    assert!(
        mouse
            .dwFlags
            .contains(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK)
    );
    assert_eq!(i64::from(mouse.dx) * 2000 / 65536, 500);
    assert_eq!(i64::from(mouse.dy) * 1000 / 65536, 500);
    assert!(!mouse.dwFlags.contains(MOUSEEVENTF_LEFTDOWN));
}

#[test]
fn absolute_pixel_roundtrips_edges_and_negative_origins() {
    for width in [1, 1920, 65536] {
        for offset in [0, width / 2, width - 1] {
            let encoded = absolute_pixel(offset - 1000, -1000, width).unwrap();
            assert_eq!(
                i64::from(encoded) * i64::from(width) / 65536,
                i64::from(offset)
            );
        }
    }
    assert!(absolute_pixel(0, 0, 65537).is_err());
    assert!(absolute_pixel(-2, -1, 1920).is_err());
}

#[test]
fn partial_press_only_releases_if_down_may_have_been_inserted() {
    let releases = Cell::new(0);
    for inserted in [0, 1] {
        assert!(
            press_with(
                &[],
                |_| Err(super::super::super::input::SendInputError {
                    requested: 2,
                    inserted,
                    last_error: 0,
                }),
                || releases.set(releases.get() + 1)
            )
            .is_err()
        );
    }
    assert_eq!(releases.get(), 0);
    assert!(
        press_with(
            &[],
            |_| Err(super::super::super::input::SendInputError {
                requested: 2,
                inserted: 2,
                last_error: 0,
            }),
            || releases.set(releases.get() + 1)
        )
        .is_err()
    );
    assert_eq!(releases.get(), 1);
    press_with(
        &[],
        |_| Ok(()),
        || panic!("successful press is held for normal release"),
    )
    .unwrap();
}
