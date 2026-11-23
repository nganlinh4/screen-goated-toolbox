//! Structural target eligibility shared by enumeration and direct resolution.

use std::mem::size_of;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
};

use super::{WindowError, WindowResult};

trait Probe {
    fn pid(&self, hwnd: usize) -> u32;
    fn root(&self, hwnd: usize) -> usize;
    fn visible(&self, hwnd: usize) -> bool;
    fn cloaked(&self, hwnd: usize) -> Option<u32>;
}

struct Win32;

impl Probe for Win32 {
    fn pid(&self, hwnd: usize) -> u32 {
        let hwnd = HWND(hwnd as *mut _);
        let mut pid = 0;
        if unsafe { IsWindow(Some(hwnd)).as_bool() } {
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        }
        pid
    }

    fn root(&self, hwnd: usize) -> usize {
        unsafe { GetAncestor(HWND(hwnd as *mut _), GA_ROOT) }.0 as usize
    }

    fn visible(&self, hwnd: usize) -> bool {
        unsafe { IsWindowVisible(HWND(hwnd as *mut _)).as_bool() }
    }

    fn cloaked(&self, hwnd: usize) -> Option<u32> {
        let mut flags = 0u32;
        unsafe {
            DwmGetWindowAttribute(
                HWND(hwnd as *mut _),
                DWMWA_CLOAKED,
                (&mut flags as *mut u32).cast(),
                size_of::<u32>() as u32,
            )
        }
        .ok()
        .map(|()| flags)
    }
}

pub(super) fn inspect(hwnd: HWND, expected_pid: Option<u32>) -> WindowResult<u32> {
    inspect_with(hwnd.0 as usize, expected_pid, std::process::id(), &Win32)
}

fn inspect_with(
    raw: usize,
    expected_pid: Option<u32>,
    own_pid: u32,
    probe: &impl Probe,
) -> WindowResult<u32> {
    let actual_pid = probe.pid(raw);
    if let Some(expected_pid) = expected_pid
        && actual_pid != expected_pid
    {
        return Err(WindowError::StaleStableTarget {
            hwnd: raw,
            expected_pid,
            actual_pid,
        });
    }
    let unavailable = |reason| WindowError::IneligibleTarget { hwnd: raw, reason };
    if actual_pid == 0 {
        return Err(unavailable("owner_unavailable"));
    }
    if actual_pid == own_pid {
        return Err(unavailable("own_process"));
    }
    if raw == 0 || probe.root(raw) != raw {
        return Err(unavailable("not_top_level"));
    }
    if !probe.visible(raw) {
        return Err(unavailable("hidden"));
    }
    match probe.cloaked(raw) {
        Some(0) => {}
        Some(_) => return Err(unavailable("cloaked")),
        None => return Err(unavailable("visibility_unavailable")),
    }
    // Recheck ownership after the separate native queries. This is not a lease
    // against same-process HWND reuse or later changes before a window effect.
    let after_pid = probe.pid(raw);
    if after_pid != actual_pid {
        return Err(WindowError::StaleStableTarget {
            hwnd: raw,
            expected_pid: actual_pid,
            actual_pid: after_pid,
        });
    }
    Ok(actual_pid)
}

#[cfg(test)]
mod tests;
