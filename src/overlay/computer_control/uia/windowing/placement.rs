//! Window placement through asynchronous submission and bounded verification.
//!
//! Synchronous `ShowWindow`/`SetWindowPos` deliver their work to the window's
//! owning thread and wait for it, so a target that is not pumping messages
//! would block the single executor job beyond cancellation. Requests are posted
//! instead, and the resulting state is observed through style and geometry
//! reads that never wait on the target thread. A request that is not observed
//! within the budget can still apply later, so it reports an unconfirmed effect.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsIconic, IsZoomed, SW_MINIMIZE, SW_RESTORE, SWP_ASYNCWINDOWPOS, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SetWindowPos, ShowWindowAsync,
};

use super::{WindowError, WindowResult};

pub(super) const VERIFY_BUDGET: Duration = Duration::from_millis(1500);
const POLL_INTERVAL: Duration = Duration::from_millis(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WindowBounds {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Placement {
    Move { x: i32, y: i32 },
    Resize { width: i32, height: i32 },
}

impl Placement {
    fn operation(self) -> &'static str {
        match self {
            Self::Move { .. } => "move",
            Self::Resize { .. } => "resize",
        }
    }

    fn satisfied_by(self, bounds: WindowBounds) -> bool {
        match self {
            Self::Move { x, y } => bounds.x == x && bounds.y == y,
            Self::Resize { width, height } => bounds.width == width && bounds.height == height,
        }
    }
}

/// The window state reads and posted requests used by placement. Reads must
/// not wait on the window's thread.
pub(super) trait PlacementTarget {
    fn minimized(&self) -> bool;
    fn restored(&self) -> bool;
    fn bounds(&self) -> Option<WindowBounds>;
    fn submit_minimize(&self) -> bool;
    fn submit_restore(&self) -> bool;
    fn submit_placement(&self, placement: Placement) -> bool;
}

pub(super) struct Win32Window(pub(super) HWND);

impl PlacementTarget for Win32Window {
    fn minimized(&self) -> bool {
        unsafe { IsIconic(self.0).as_bool() }
    }

    fn restored(&self) -> bool {
        unsafe { !IsIconic(self.0).as_bool() && !IsZoomed(self.0).as_bool() }
    }

    fn bounds(&self) -> Option<WindowBounds> {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(self.0, &mut rect) }.ok()?;
        Some(WindowBounds {
            x: rect.left,
            y: rect.top,
            width: rect.right.checked_sub(rect.left)?,
            height: rect.bottom.checked_sub(rect.top)?,
        })
    }

    fn submit_minimize(&self) -> bool {
        unsafe { ShowWindowAsync(self.0, SW_MINIMIZE).as_bool() }
    }

    fn submit_restore(&self) -> bool {
        unsafe { ShowWindowAsync(self.0, SW_RESTORE).as_bool() }
    }

    fn submit_placement(&self, placement: Placement) -> bool {
        let (x, y, width, height, keep) = match placement {
            Placement::Move { x, y } => (x, y, 0, 0, SWP_NOSIZE),
            Placement::Resize { width, height } => (0, 0, width, height, SWP_NOMOVE),
        };
        let flags = keep | SWP_NOZORDER | SWP_ASYNCWINDOWPOS;
        unsafe { SetWindowPos(self.0, None, x, y, width, height, flags) }.is_ok()
    }
}

enum Unsettled {
    Cancelled,
    Expired,
}

fn settle(
    cancel: &AtomicBool,
    deadline: Instant,
    mut observed: impl FnMut() -> bool,
) -> Result<(), Unsettled> {
    loop {
        if observed() {
            return Ok(());
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(Unsettled::Cancelled);
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(Unsettled::Expired);
        }
        std::thread::sleep(POLL_INTERVAL.min(deadline - now));
    }
}

fn submitted_error(
    unsettled: Unsettled,
    operation: &'static str,
    target: &impl PlacementTarget,
) -> WindowError {
    match unsettled {
        Unsettled::Cancelled => WindowError::Cancelled {
            effect_may_have_occurred: true,
        },
        Unsettled::Expired => WindowError::Unconfirmed {
            operation,
            observed: target.bounds(),
        },
    }
}

fn cancelled_after_submission(cancel: &AtomicBool) -> WindowResult<()> {
    if cancel.load(Ordering::SeqCst) {
        return Err(WindowError::Cancelled {
            effect_may_have_occurred: true,
        });
    }
    Ok(())
}

/// Minimize and wait for the minimized state. `Ok(false)` means the system
/// did not accept the request.
pub(super) fn minimize(
    target: &impl PlacementTarget,
    cancel: &AtomicBool,
    budget: Duration,
) -> WindowResult<bool> {
    if cancel.load(Ordering::SeqCst) {
        return Err(WindowError::Cancelled {
            effect_may_have_occurred: false,
        });
    }
    let deadline = Instant::now() + budget;
    if !target.submit_minimize() {
        return Ok(target.minimized());
    }
    settle(cancel, deadline, || target.minimized())
        .map_err(|unsettled| submitted_error(unsettled, "minimize", target))?;
    cancelled_after_submission(cancel)?;
    Ok(true)
}

fn restore_until(
    target: &impl PlacementTarget,
    cancel: &AtomicBool,
    deadline: Instant,
    observed: impl FnMut() -> bool,
) -> WindowResult<()> {
    if !target.submit_restore() {
        return Err(WindowError::Unconfirmed {
            operation: "restore",
            observed: target.bounds(),
        });
    }
    settle(cancel, deadline, observed)
        .map_err(|unsettled| submitted_error(unsettled, "restore", target))?;
    cancelled_after_submission(cancel)
}

/// Restore a minimized or maximized window, then apply `placement` and wait
/// until its geometry is observed. A normal window is not re-activated.
pub(super) fn place(
    target: &impl PlacementTarget,
    placement: Placement,
    cancel: &AtomicBool,
    budget: Duration,
) -> WindowResult<bool> {
    if cancel.load(Ordering::SeqCst) {
        return Err(WindowError::Cancelled {
            effect_may_have_occurred: false,
        });
    }
    let deadline = Instant::now() + budget;
    // A window minimized from the maximized state restores to maximized, so
    // leaving the minimized state and leaving the maximized state are two
    // separately observed restore steps.
    if target.minimized() {
        restore_until(target, cancel, deadline, || !target.minimized())?;
    }
    if !target.restored() {
        restore_until(target, cancel, deadline, || target.restored())?;
    }
    if !target.submit_placement(placement) {
        return Ok(false);
    }
    settle(cancel, deadline, || {
        target
            .bounds()
            .is_some_and(|bounds| placement.satisfied_by(bounds))
    })
    .map_err(|unsettled| submitted_error(unsettled, placement.operation(), target))?;
    cancelled_after_submission(cancel)?;
    Ok(true)
}

#[cfg(test)]
#[path = "placement_tests.rs"]
mod tests;
