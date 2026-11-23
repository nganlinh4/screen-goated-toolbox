//! Cursor delivery is needed only by mounted interactive controls.
use super::super::protocol::SceneCard;
use super::{CARDS, HOST, topology};
use std::cell::Cell;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

#[derive(Clone, Copy, Default)]
struct CursorUpdates {
    previous: Option<(i32, i32)>,
}

impl CursorUpdates {
    fn invalidate(&mut self) {
        self.previous = None;
    }

    fn next(
        &mut self,
        enabled: bool,
        sample: impl FnOnce() -> Option<(i32, i32)>,
    ) -> Option<(i32, i32)> {
        if !enabled {
            self.invalidate();
            return None;
        }
        let Some(position) = sample() else {
            self.invalidate();
            return None;
        };
        if self.previous == Some(position) {
            return None;
        }
        self.previous = Some(position);
        Some(position)
    }
}

thread_local! {
    static UPDATES: Cell<CursorUpdates> = const { Cell::new(CursorUpdates { previous: None }) };
}

pub(super) fn invalidate() {
    UPDATES.with(|updates| updates.set(CursorUpdates::default()));
}

fn has_controls<'a>(
    mut cards: impl Iterator<Item = &'a SceneCard>,
    processing_control_ids: &[isize],
) -> bool {
    cards.any(|card| {
        !card.controls.hidden && (card.visible || processing_control_ids.contains(&card.id))
    })
}

pub(super) fn poll(enabled: bool) {
    let enabled = enabled && {
        let processing = super::super::processing::control_ids();
        has_controls(CARDS.lock().unwrap().values(), &processing)
    };
    poll_with(enabled, sample_cursor, submit_cursor);
}

fn poll_with(
    enabled: bool,
    sample: impl FnOnce() -> Option<(i32, i32)>,
    submit: impl FnOnce((i32, i32)) -> bool,
) {
    let position = UPDATES.with(|updates| {
        let mut current = updates.get();
        let position = current.next(enabled, sample);
        // Commit before invoking COM: a nested topology invalidation must survive.
        updates.set(current);
        position
    });
    let Some(position) = position else { return };
    if !submit(position) {
        invalidate();
    }
}

fn submit_cursor((x, y): (i32, i32)) -> bool {
    let script = format!(
        "window.updateCursorPosition?.({x}/(window.devicePixelRatio||1),{y}/(window.devicePixelRatio||1));"
    );
    HOST.with(|slot| {
        slot.try_borrow().ok().is_some_and(|host| {
            host.as_ref()
                .is_some_and(|host| host.submit_script(&script))
        })
    })
}

fn sample_cursor() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.ok()?;
    let origin = topology::origin();
    Some((
        point.x.saturating_sub(origin.x),
        point.y.saturating_sub(origin.y),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_ticks_do_not_sample_and_resume_republishes_the_same_position() {
        let mut updates = CursorUpdates::default();
        assert_eq!(updates.next(true, || Some((10, 20))), Some((10, 20)));
        for _ in 0..100 {
            assert_eq!(
                updates.next(false, || panic!("idle cursor was sampled")),
                None
            );
        }
        assert_eq!(updates.next(true, || Some((10, 20))), Some((10, 20)));
    }

    #[test]
    fn stationary_controls_do_not_receive_duplicate_scripts() {
        let mut updates = CursorUpdates::default();
        assert_eq!(updates.next(true, || Some((10, 20))), Some((10, 20)));
        for _ in 0..100 {
            assert_eq!(updates.next(true, || Some((10, 20))), None);
        }
        assert_eq!(updates.next(true, || Some((11, 20))), Some((11, 20)));
    }

    #[test]
    fn failed_samples_and_invalidated_coordinate_epochs_allow_retry() {
        let mut updates = CursorUpdates::default();
        assert_eq!(updates.next(true, || Some((10, 20))), Some((10, 20)));
        assert_eq!(updates.next(true, || None), None);
        assert_eq!(updates.next(true, || Some((10, 20))), Some((10, 20)));
        updates.invalidate();
        assert_eq!(updates.next(true, || Some((10, 20))), Some((10, 20)));
    }

    #[test]
    fn failed_submission_is_retried_and_idle_dispatch_never_reads_or_submits() {
        invalidate();
        let mut submissions = 0;
        poll_with(
            true,
            || Some((10, 20)),
            |_| {
                submissions += 1;
                false
            },
        );
        poll_with(
            true,
            || Some((10, 20)),
            |_| {
                submissions += 1;
                true
            },
        );
        poll_with(
            true,
            || Some((10, 20)),
            |_| panic!("duplicate cursor submission"),
        );
        assert_eq!(submissions, 2);
        poll_with(
            false,
            || panic!("idle cursor read"),
            |_| panic!("idle cursor submission"),
        );
    }

    #[test]
    fn nested_invalidation_during_submission_is_not_overwritten() {
        invalidate();
        poll_with(
            true,
            || Some((10, 20)),
            |_| {
                invalidate();
                true
            },
        );
        let mut submitted = false;
        poll_with(
            true,
            || Some((10, 20)),
            |_| {
                submitted = true;
                true
            },
        );
        assert!(submitted);
    }

    #[test]
    fn visible_controls_and_bound_processing_are_the_only_consumers() {
        let mut card: SceneCard = serde_json::from_value(serde_json::json!({
            "id": 7, "rect": {"x": 0, "y": 0, "width": 10, "height": 10},
            "body": "", "document": null, "refining": false,
            "background": "#fff", "opacity": 100, "visible": false
        }))
        .unwrap();
        assert!(!has_controls(std::iter::empty(), &[7]));
        assert!(!has_controls(std::iter::once(&card), &[]));
        assert!(!has_controls(std::iter::once(&card), &[8]));
        assert!(has_controls(std::iter::once(&card), &[7]));
        card.visible = true;
        assert!(has_controls(std::iter::once(&card), &[]));
        card.controls.hidden = true;
        assert!(!has_controls(std::iter::once(&card), &[7]));
    }
}
