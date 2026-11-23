//! Ownership for the delayed second focus attempt.
use super::state::*;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

pub(super) struct DelayedFocus {
    pending: PendingText,
    due: Instant,
}

pub(super) fn schedule_retry(session: u64, owner: Option<EditorOwner>) {
    if !EDITOR_SESSIONS.is_current(session) || PASSIVE_CAPTURE_ENABLED.load(Ordering::SeqCst) {
        return;
    }
    let Some(operation) =
        super::create_text_operation(session, owner.as_ref().map(|owner| &owner.token), false)
    else {
        return;
    };
    let pending = PendingText {
        text: String::new(),
        action: EditorAction::Focus { retry: true },
        session,
        owner,
        operation: Some(operation.clone()),
    };
    let previous = DELAYED_FOCUS.with(|slot| {
        slot.borrow_mut().replace(DelayedFocus {
            pending,
            due: Instant::now() + Duration::from_millis(200),
        })
    });
    if let Some(previous) = previous.and_then(|entry| entry.pending.operation) {
        previous.cancel();
    }
    let hwnd = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd == 0 || unsafe { SetTimer(Some(HWND(hwnd as *mut _)), 3, 200, None) } == 0 {
        DELAYED_FOCUS.with(|slot| {
            slot.borrow_mut().take();
        });
        operation.fail_uncertain();
    }
}

pub(super) fn cancel(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(Some(hwnd), 3);
    }
    let previous = DELAYED_FOCUS.with(|slot| slot.borrow_mut().take());
    if let Some(operation) = previous.and_then(|entry| entry.pending.operation) {
        operation.cancel();
    }
}

pub(super) fn fire(hwnd: HWND) {
    let pending = DELAYED_FOCUS.with(|slot| take_if_due(&mut slot.borrow_mut(), Instant::now()));
    let Some(pending) = pending else { return };
    unsafe {
        let _ = KillTimer(Some(hwnd), 3);
    }
    let Some(operation) = pending.operation.clone() else {
        return;
    };
    let owner = pending.owner.as_ref().map(|owner| owner.token.clone());
    let admitted = if let Some(owner) = owner {
        owner.run_if_active(|| EDITOR_SESSIONS.enqueue(pending))
    } else {
        Some(EDITOR_SESSIONS.enqueue(pending))
    };
    if admitted == Some(true) {
        if !super::wake_editor_queue() {
            EDITOR_SESSIONS.remove_pending(operation.id());
            operation.fail_uncertain();
        }
    } else {
        operation.cancel();
    }
}

fn take_if_due(slot: &mut Option<DelayedFocus>, now: Instant) -> Option<PendingText> {
    if slot.as_ref().is_some_and(|entry| now >= entry.due) {
        slot.take().map(|entry| entry.pending)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_timer_message_does_not_consume_new_retry_before_its_deadline() {
        let now = Instant::now();
        let mut slot = Some(DelayedFocus {
            due: now + Duration::from_millis(200),
            pending: PendingText {
                text: String::new(),
                action: EditorAction::Focus { retry: true },
                session: 1,
                owner: None,
                operation: None,
            },
        });
        assert!(take_if_due(&mut slot, now + Duration::from_millis(10)).is_none());
        assert!(slot.is_some());
        assert!(take_if_due(&mut slot, now + Duration::from_millis(200)).is_some());
        assert!(slot.is_none());
    }
}
