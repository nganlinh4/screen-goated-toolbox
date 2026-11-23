//! Bind coordinate-based pointer effects to the cursor position observed after motion.
use super::*;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Geometry {
    desktop: (i32, i32, i32, i32),
    nominal: (i32, i32),
}

impl Geometry {
    pub(super) fn capture(x: f64, y: f64) -> Result<Self> {
        if !(0.0..=1000.0).contains(&x) || !(0.0..=1000.0).contains(&y) {
            return Err(anyhow!(
                "pointer coordinates must be within the observed desktop"
            ));
        }
        let desktop = super::super::virtual_desktop();
        if desktop.2 <= 0 || desktop.3 <= 0 {
            return Err(anyhow!("virtual desktop is unavailable"));
        }
        let nominal = (
            desktop
                .0
                .saturating_add((x / 1000.0 * f64::from(desktop.2 - 1)).round() as i32),
            desktop
                .1
                .saturating_add((y / 1000.0 * f64::from(desktop.3 - 1)).round() as i32),
        );
        Ok(Self { desktop, nominal })
    }

    pub(super) fn anchor(&self) -> Result<(i32, i32)> {
        let current = self.observe()?;
        // Humanized movement intentionally jitters by up to 2.5 pixels; allow
        // rounding at the edge, but reject a visibly displaced cursor.
        if (i64::from(current.0) - i64::from(self.nominal.0)).abs() > 5
            || (i64::from(current.1) - i64::from(self.nominal.1)).abs() > 5
        {
            return Err(anyhow!("pointer moved away from the requested target"));
        }
        Ok(current)
    }

    fn observe(&self) -> Result<(i32, i32)> {
        if super::super::virtual_desktop() != self.desktop {
            return Err(anyhow!("desktop geometry changed before pointer action"));
        }
        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point)? };
        Ok((point.x, point.y))
    }
}

fn admit_with(
    anchor: (i32, i32),
    cancelled: impl Fn() -> bool,
    mut observe: impl FnMut() -> Result<(i32, i32)>,
    verify: impl FnOnce() -> Result<()>,
) -> Result<bool> {
    if cancelled() {
        return Ok(false);
    }
    if observe()? != anchor {
        return Err(anyhow!("pointer moved before target validation"));
    }
    verify()?;
    if cancelled() {
        return Ok(false);
    }
    if observe()? != anchor {
        return Err(anyhow!("pointer moved during target validation"));
    }
    Ok(true)
}

pub(super) fn admit(
    geometry: &Geometry,
    anchor: (i32, i32),
    args: &Value,
    cancel: &AtomicBool,
) -> Result<bool> {
    admit_with(
        anchor,
        || cancel.load(Ordering::Relaxed),
        || geometry.observe(),
        || super::super::verify_expected_pointer_target(args, anchor),
    )
}

fn absolute_pixel(pixel: i32, origin: i32, size: i32) -> Result<i32> {
    let offset = i64::from(pixel) - i64::from(origin);
    let size = i64::from(size);
    if !(1..=65536).contains(&size) || offset < 0 || offset >= size {
        return Err(anyhow!("pointer is outside the observed desktop"));
    }
    let ceil = |value: i64| (value + size - 1) / size;
    let low = ceil(offset * 65536);
    let high = ceil((offset + 1) * 65536) - 1;
    if low > high || high > 65535 {
        return Err(anyhow!(
            "pointer pixel cannot be represented in absolute input"
        ));
    }
    Ok(((low + high) / 2) as i32)
}

fn move_event(geometry: &Geometry, anchor: (i32, i32)) -> Result<INPUT> {
    let (vx, vy, width, height) = geometry.desktop;
    let x = absolute_pixel(anchor.0, vx, width)?;
    let y = absolute_pixel(anchor.1, vy, height)?;
    Ok(super::super::mouse_input(
        x,
        y,
        0,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    ))
}

pub(super) fn press(
    geometry: &Geometry,
    anchor: (i32, i32),
    down: MOUSE_EVENT_FLAGS,
    up: MOUSE_EVENT_FLAGS,
) -> Result<()> {
    let inputs = [
        move_event(geometry, anchor)?,
        super::super::mouse_input(0, 0, 0, down),
    ];
    press_with(&inputs, super::super::send, || {
        let _ = super::super::release(&[super::super::mouse_input(0, 0, 0, up)]);
    })
}

fn press_with(
    inputs: &[INPUT],
    send: impl FnOnce(&[INPUT]) -> std::result::Result<(), super::super::input::SendInputError>,
    release: impl FnOnce(),
) -> Result<()> {
    if let Err(error) = send(inputs) {
        // The button-down is the second event. A reported inserted prefix of
        // zero or one cannot have pressed it and must not release a user's key.
        if error.inserted >= 2 {
            release();
        }
        return Err(error.into());
    }
    Ok(())
}

pub(super) fn scroll(
    geometry: &Geometry,
    anchor: (i32, i32),
    flag: MOUSE_EVENT_FLAGS,
    data: i32,
) -> Result<()> {
    super::super::send(&[
        move_event(geometry, anchor)?,
        super::super::mouse_input(0, 0, data, flag),
    ])?;
    Ok(())
}

#[cfg(test)]
#[path = "targeted_tests.rs"]
mod tests;
