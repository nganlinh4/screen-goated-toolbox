//! Win32 top-level window management: find / raise / minimize / resize / move a
//! window by an exact title, executable, or stable HWND/PID identity, including
//! windows without a UIA provider. Split from the UIA tree code (`uia.rs`) for the
//! file-size limit. Self-contained — only Win32 windowing + process APIs, no UIA.

use std::fmt;
use std::sync::atomic::AtomicBool;

use unicode_normalization::UnicodeNormalization;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};
use windows_core::BOOL;

const STABLE_TARGET_PREFIX: &str = "@hwnd:";
mod eligibility;
mod foreground;
mod placement;
use placement::{Placement, VERIFY_BUDGET, Win32Window, WindowBounds};

#[derive(Clone, Debug)]
struct WindowCandidate {
    hwnd: HWND,
    pid: u32,
    title: String,
    exe: String,
}

impl WindowCandidate {
    fn stable_target(&self) -> String {
        format!(
            "{STABLE_TARGET_PREFIX}{}:{}",
            self.hwnd.0 as usize, self.pid
        )
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct WindowDescriptor {
    title: String,
    executable: String,
    hwnd: usize,
    pid: u32,
    target: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WindowError {
    EmptyTarget,
    InvalidStableTarget {
        target: String,
    },
    NotFound {
        target: String,
    },
    StaleStableTarget {
        hwnd: usize,
        expected_pid: u32,
        actual_pid: u32,
    },
    IneligibleTarget {
        hwnd: usize,
        reason: &'static str,
    },
    Ambiguous {
        target: String,
        matches: Vec<String>,
    },
    InvalidSize {
        width: i32,
        height: i32,
    },
    Cancelled {
        effect_may_have_occurred: bool,
    },
    /// The request was posted but its state was not observed in time; it may
    /// still apply once the window's thread processes it.
    Unconfirmed {
        operation: &'static str,
        observed: Option<WindowBounds>,
    },
}

impl WindowError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::EmptyTarget | Self::InvalidStableTarget { .. } => "invalid_window_target",
            Self::InvalidSize { .. } => "invalid_window_size",
            Self::NotFound { .. } => "window_not_found",
            Self::StaleStableTarget { .. } => "stale_window_target",
            Self::IneligibleTarget { .. } => "ineligible_window_target",
            Self::Ambiguous { .. } => "ambiguous_window_target",
            Self::Cancelled { .. } => "cancelled",
            Self::Unconfirmed { .. } => "window_effect_unconfirmed",
        }
    }
}

impl fmt::Display for WindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTarget => write!(f, "window target is empty"),
            Self::InvalidStableTarget { target } => {
                write!(f, "invalid stable window target {target:?}")
            }
            Self::NotFound { target } => write!(f, "no window exactly matches {target:?}"),
            Self::StaleStableTarget {
                hwnd,
                expected_pid,
                actual_pid,
            } => write!(
                f,
                "stable window target @hwnd:{hwnd}:{expected_pid} is stale (current pid {actual_pid})"
            ),
            Self::IneligibleTarget { hwnd, reason } => write!(
                f,
                "window {hwnd} is not an eligible top-level target: {reason}"
            ),
            Self::Ambiguous { target, matches } => write!(
                f,
                "window target {target:?} is ambiguous; choose one stable target: {}",
                matches.join(", ")
            ),
            Self::InvalidSize { width, height } => {
                write!(f, "window size must be positive, got {width}x{height}")
            }
            Self::Cancelled { .. } => write!(f, "window operation cancelled"),
            Self::Unconfirmed {
                operation,
                observed,
            } => {
                write!(
                    f,
                    "window {operation} was requested but not observed within {} ms; it may still \
                     apply when the window responds. Re-observe before retrying",
                    VERIFY_BUDGET.as_millis()
                )?;
                match observed {
                    Some(bounds) => write!(
                        f,
                        " (observed x={}, y={}, width={}, height={})",
                        bounds.x, bounds.y, bounds.width, bounds.height
                    ),
                    None => write!(f, " (current bounds unavailable)"),
                }
            }
        }
    }
}

impl std::error::Error for WindowError {}

type WindowResult<T> = Result<T, WindowError>;

/// Bring the exactly resolved top-level window to the foreground
/// (restoring it if minimized), and VERIFY it actually took. General-purpose:
/// used at startup for `CC_UIA_WINDOW` scoping AND by the agent's `focus_window`
/// tool to switch to any app by name (e.g. when a window it opened is hidden
/// behind a fullscreen game). Resolution errors are distinct from a false result,
/// which means the switch could not be forced (e.g. an exclusive-fullscreen app owns the foreground —
/// which nothing short of minimizing it can move, elevated or not).
pub(crate) fn raise_window(target: &str) -> WindowResult<bool> {
    let before = unsafe { GetForegroundWindow() };
    let selected = find_top_window(target)?;
    let raised = foreground::raise(selected)?;
    let after = unsafe { GetForegroundWindow() };
    super::super::telemetry::event(
        "focus_window_result",
        "windowing",
        super::super::telemetry::Privacy::UserText,
        serde_json::json!({
            "requested_target": target,
            "selected_hwnd": selected.0 as usize,
            "foreground_before_hwnd": before.0 as usize,
            "foreground_after_hwnd": after.0 as usize,
            "raised": raised,
            "verified": selected.0 == after.0,
        }),
    );
    Ok(raised)
}

pub(crate) fn raise_window_with_target(target: &str) -> WindowResult<(bool, String)> {
    let stable = stable_window_target(target)?;
    raise_window(&stable).map(|raised| (raised, stable))
}

/// Executable basename owning `pid`; empty on failure.
fn process_exe_name(pid: u32) -> String {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 520];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(h);
        if !ok || size == 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..size as usize])
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .to_string()
    }
}

/// Titled, eligible top-level windows, independent of UIA provider availability.
fn enum_top_windows() -> Vec<WindowCandidate> {
    extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            // Exclude same-process windows before title retrieval, which can
            // synchronously send a message to a thread that is not pumping.
            let Ok(pid) = eligibility::inspect(hwnd, None) else {
                return BOOL(1);
            };
            let mut buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut buf);
            if len == 0 {
                return BOOL(1);
            }
            let title = String::from_utf16_lossy(&buf[..len as usize]);
            if title.is_empty() {
                return BOOL(1);
            }
            let out = &mut *(lparam.0 as *mut Vec<WindowCandidate>);
            out.push(WindowCandidate {
                hwnd,
                pid,
                title,
                exe: process_exe_name(pid),
            });
            BOOL(1)
        }
    }
    let mut out: Vec<WindowCandidate> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

/// Resolve a stable `@hwnd:<handle>:<pid>` identity, exact normalized title,
/// exact normalized executable basename, or executable stem. Multiple exact
/// matches are an error; the caller must use one of their stable identities.
pub(crate) fn find_top_window(target: &str) -> WindowResult<HWND> {
    let result = match parse_stable_target(target)? {
        Some((raw, expected_pid)) => resolve_stable_target(raw, expected_pid),
        None => {
            let candidates = enum_top_windows();
            resolve_named_candidate(target, &candidates).and_then(|candidate| {
                resolve_stable_target(candidate.hwnd.0 as usize, candidate.pid)
            })
        }
    };
    super::super::telemetry::event(
        "window_resolution",
        "windowing",
        super::super::telemetry::Privacy::UserText,
        serde_json::json!({
            "requested_target": target,
            "selected_hwnd": result.as_ref().ok().map(|hwnd| hwnd.0 as usize),
            "error_code": result.as_ref().err().map(WindowError::code),
        }),
    );
    result
}

/// Resolve an exact title, executable, or stable identity once, then carry the
/// HWND/PID identity for the lifetime of the task. Window titles are mutable
/// document state and must not be re-matched after navigation.
pub(crate) fn stable_window_target(target: &str) -> WindowResult<String> {
    if let Some((raw, expected_pid)) = parse_stable_target(target)? {
        resolve_stable_target(raw, expected_pid)?;
        return Ok(format!("{STABLE_TARGET_PREFIX}{raw}:{expected_pid}"));
    }
    let candidates = enum_top_windows();
    let candidate = resolve_named_candidate(target, &candidates)?;
    resolve_stable_target(candidate.hwnd.0 as usize, candidate.pid)?;
    Ok(candidate.stable_target())
}

pub(crate) fn window_identity(target: &str) -> WindowResult<(u64, u64)> {
    let hwnd = find_top_window(target)?;
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return Err(WindowError::NotFound {
            target: target.to_string(),
        });
    }
    Ok((hwnd.0 as usize as u64, u64::from(pid)))
}

fn normalize_window_key(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_stable_target(target: &str) -> WindowResult<Option<(usize, u32)>> {
    let trimmed = target.trim();
    let Some(encoded) = trimmed.strip_prefix(STABLE_TARGET_PREFIX) else {
        return Ok(None);
    };
    let Some((raw, pid)) = encoded.split_once(':') else {
        return Err(WindowError::InvalidStableTarget {
            target: target.to_string(),
        });
    };
    let parsed = raw
        .parse::<usize>()
        .ok()
        .zip(pid.parse::<u32>().ok())
        .filter(|(raw, pid)| *raw != 0 && *pid != 0);
    parsed
        .map(Some)
        .ok_or_else(|| WindowError::InvalidStableTarget {
            target: target.to_string(),
        })
}

fn resolve_stable_target(raw: usize, expected_pid: u32) -> WindowResult<HWND> {
    let hwnd = HWND(raw as *mut _);
    eligibility::inspect(hwnd, Some(expected_pid))?;
    Ok(hwnd)
}

fn resolve_named_candidate<'a>(
    target: &str,
    candidates: &'a [WindowCandidate],
) -> WindowResult<&'a WindowCandidate> {
    let want = normalize_window_key(target);
    if want.is_empty() {
        return Err(WindowError::EmptyTarget);
    }
    let matches = candidates
        .iter()
        .filter(|candidate| {
            let title = normalize_window_key(&candidate.title);
            let exe = normalize_window_key(&candidate.exe);
            let stem = exe.strip_suffix(".exe").unwrap_or(&exe);
            title == want || exe == want || stem == want
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [candidate] => Ok(*candidate),
        [] => Err(WindowError::NotFound {
            target: target.to_string(),
        }),
        _ => Err(WindowError::Ambiguous {
            target: target.to_string(),
            matches: matches
                .iter()
                .map(|candidate| candidate.stable_target())
                .collect(),
        }),
    }
}

/// Open top-level windows with both human-readable state and a stable target.
/// The stable target is preferred whenever titles/executables are duplicated.
pub(crate) fn list_windows() -> Vec<WindowDescriptor> {
    enum_top_windows()
        .into_iter()
        .map(|candidate| WindowDescriptor {
            title: candidate.title.clone(),
            executable: candidate.exe.clone(),
            hwnd: candidate.hwnd.0 as usize,
            pid: candidate.pid,
            target: candidate.stable_target(),
        })
        .collect()
}

/// Minimize the exactly resolved top-level window with a posted `ShowWindow`
/// request (no input injection, so it works even on a fullscreen game that is
/// swallowing keystrokes) and verify the minimized state. A window whose
/// thread is not pumping cannot block the executor. Non-elevated.
pub(crate) fn minimize_window(target: &str, cancel: &AtomicBool) -> WindowResult<bool> {
    let hwnd = find_top_window(target)?;
    placement::minimize(&Win32Window(hwnd), cancel, VERIFY_BUDGET)
}

/// Resize the window matching `target` to `w`x`h` pixels (restoring it first
/// when minimized or maximized, so it can shrink). Keeps its position and
/// verifies the resulting size. Non-elevated.
pub(crate) fn resize_window(
    target: &str,
    w: i32,
    h: i32,
    cancel: &AtomicBool,
) -> WindowResult<bool> {
    if w <= 0 || h <= 0 {
        return Err(WindowError::InvalidSize {
            width: w,
            height: h,
        });
    }
    let hwnd = find_top_window(target)?;
    placement::place(
        &Win32Window(hwnd),
        Placement::Resize {
            width: w,
            height: h,
        },
        cancel,
        VERIFY_BUDGET,
    )
}

/// Move the window matching `target` so its top-left corner is at screen pixel
/// (x, y). Keeps its size and verifies the resulting position. Non-elevated.
pub(crate) fn move_window(target: &str, x: i32, y: i32, cancel: &AtomicBool) -> WindowResult<bool> {
    let hwnd = find_top_window(target)?;
    placement::place(
        &Win32Window(hwnd),
        Placement::Move { x, y },
        cancel,
        VERIFY_BUDGET,
    )
}

#[cfg(test)]
#[path = "windowing_tests.rs"]
mod tests;
