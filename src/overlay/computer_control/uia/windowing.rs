//! Win32 top-level window management: find / raise / minimize / resize / move a
//! window by an exact title, executable, or stable HWND/PID identity, including fullscreen games that
//! expose no UIA provider. Split from the UIA tree code (`uia.rs`) for the
//! file-size limit. Self-contained — only Win32 windowing + process APIs, no UIA.

use std::fmt;

use unicode_normalization::UnicodeNormalization;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, SW_MINIMIZE, SW_RESTORE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SetForegroundWindow, SetWindowPos, ShowWindow,
};
use windows_core::BOOL;

const STABLE_TARGET_PREFIX: &str = "@hwnd:";

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
    Ambiguous {
        target: String,
        matches: Vec<String>,
    },
    InvalidSize {
        width: i32,
        height: i32,
    },
}

impl WindowError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::EmptyTarget | Self::InvalidStableTarget { .. } => "invalid_window_target",
            Self::InvalidSize { .. } => "invalid_window_size",
            Self::NotFound { .. } => "window_not_found",
            Self::StaleStableTarget { .. } => "stale_window_target",
            Self::Ambiguous { .. } => "ambiguous_window_target",
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
            Self::Ambiguous { target, matches } => write!(
                f,
                "window target {target:?} is ambiguous; choose one stable target: {}",
                matches.join(", ")
            ),
            Self::InvalidSize { width, height } => {
                write!(f, "window size must be positive, got {width}x{height}")
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
    let raised = unsafe { force_foreground_hwnd(selected) };
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

// --- Win32 EnumWindows: finds EVERY top-level window, including fullscreen GAMES (Unity/Unreal/
//     native) that expose NO UI Automation provider and so never appear in the UIA tree. That gap
//     is exactly why `focus_window` couldn't reach a minimized game and matched a browser tab by
//     title instead. Mirrors the proven enumeration in `realtime_webview::app_selection`. ---

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

/// (hwnd, title, exe basename) for each visible, titled top-level window of ANOTHER process (our
/// own windows — the orb, the app — are skipped). Win32-level, so it sees games the UIA tree can't.
fn enum_top_windows() -> Vec<WindowCandidate> {
    extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }
            // Resolve the owning process and skip our OWN windows (orb / app / splash) FIRST — this
            // MUST precede GetWindowTextW: that call sends WM_GETTEXT and blocks INDEFINITELY on a
            // same-process window whose thread isn't pumping (MS-documented), which hung the agent
            // when it was asked to act on this very app. Other processes' titles read cached (safe).
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 || pid == std::process::id() {
                return BOOL(1);
            }
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
            resolve_named_candidate(target, &candidates).map(|candidate| candidate.hwnd)
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
    resolve_named_candidate(target, &candidates).map(WindowCandidate::stable_target)
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
    let mut actual_pid = 0u32;
    if unsafe { IsWindow(Some(hwnd)).as_bool() } {
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut actual_pid)) };
    }
    if actual_pid != expected_pid {
        return Err(WindowError::StaleStableTarget {
            hwnd: raw,
            expected_pid,
            actual_pid,
        });
    }
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

/// Minimize the exactly resolved top-level window via a DIRECT
/// `ShowWindow` on its handle (no input injection — so it works even on a
/// fullscreen game that's swallowing keystrokes, unlike a synthetic Win+D).
/// Returns a resolution error when no unique window is known. Non-elevated.
pub(crate) fn minimize_window(target: &str) -> WindowResult<bool> {
    let hwnd = find_top_window(target)?;
    unsafe {
        let _ = ShowWindow(hwnd, SW_MINIMIZE);
    }
    Ok(true)
}

/// Resize the window matching `target` to `w`x`h` pixels (restoring it first, so
/// a maximized window can shrink). Keeps its position. Non-elevated.
pub(crate) fn resize_window(target: &str, w: i32, h: i32) -> WindowResult<bool> {
    if w <= 0 || h <= 0 {
        return Err(WindowError::InvalidSize {
            width: w,
            height: h,
        });
    }
    unsafe {
        let hwnd = find_top_window(target)?;
        let _ = ShowWindow(hwnd, SW_RESTORE); // can't resize while maximized
        Ok(SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER).is_ok())
    }
}

/// Move the window matching `target` so its top-left corner is at screen pixel
/// (x, y). Keeps its size. Non-elevated.
pub(crate) fn move_window(target: &str, x: i32, y: i32) -> WindowResult<bool> {
    unsafe {
        let hwnd = find_top_window(target)?;
        let _ = ShowWindow(hwnd, SW_RESTORE); // can't move while maximized
        Ok(SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER).is_ok())
    }
}

/// Force `hwnd` to the foreground using the `AttachThreadInput` trick (a
/// background process otherwise can't legally call `SetForegroundWindow`),
/// restoring it if minimized. Retries briefly and verifies — returns whether
/// `hwnd` is actually the foreground window afterward. Non-elevated, best-effort.
unsafe fn force_foreground_hwnd(hwnd: HWND) -> bool {
    unsafe {
        let this_tid = GetCurrentThreadId();
        for attempt in 0..3 {
            if GetForegroundWindow().0 == hwnd.0 {
                return true;
            }
            // Only un-minimize. SW_RESTORE changes a maximized window's placement,
            // while on a minimized window it restores the prior maximized/normal state.
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let fg_tid = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let attach = fg_tid != 0 && fg_tid != this_tid;
            if attach {
                let _ = AttachThreadInput(this_tid, fg_tid, true);
            }
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
            if attach {
                let _ = AttachThreadInput(this_tid, fg_tid, false);
            }
            if GetForegroundWindow().0 == hwnd.0 {
                return true;
            }
            if attempt < 2 {
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        }
        GetForegroundWindow().0 == hwnd.0
    }
}

#[cfg(test)]
mod tests {
    use super::{WindowCandidate, WindowError, parse_stable_target, resolve_named_candidate};
    use windows::Win32::Foundation::HWND;

    fn candidate(raw: usize, pid: u32, title: &str, exe: &str) -> WindowCandidate {
        WindowCandidate {
            hwnd: HWND(raw as *mut _),
            pid,
            title: title.to_string(),
            exe: exe.to_string(),
        }
    }

    #[test]
    fn title_substrings_are_not_window_targets() {
        let candidates = [candidate(1, 10, "Notes 10", "host.exe")];
        assert!(matches!(
            resolve_named_candidate("Notes 1", &candidates),
            Err(WindowError::NotFound { .. })
        ));
    }

    #[test]
    fn ambiguous_exact_titles_fail_with_stable_choices() {
        let candidates = [
            candidate(1, 10, "Notes", "host-a.exe"),
            candidate(2, 11, "Notes", "host-b.exe"),
        ];
        let error = resolve_named_candidate(" notes ", &candidates).unwrap_err();
        assert_eq!(
            error,
            WindowError::Ambiguous {
                target: " notes ".to_string(),
                matches: vec!["@hwnd:1:10".to_string(), "@hwnd:2:11".to_string()],
            }
        );
    }

    #[test]
    fn exact_executable_stem_resolves_uniquely() {
        let candidates = [
            candidate(1, 10, "Document", "editor.exe"),
            candidate(2, 11, "Editor guide", "browser.exe"),
        ];
        assert_eq!(
            resolve_named_candidate("ＥＤＩＴＯＲ", &candidates)
                .unwrap()
                .hwnd
                .0 as usize,
            1
        );
    }

    #[test]
    fn stable_target_requires_both_nonzero_identity_parts() {
        assert_eq!(
            parse_stable_target("@hwnd:123:77").unwrap(),
            Some((123, 77))
        );
        for invalid in ["@hwnd:123", "@hwnd:0:77", "@hwnd:123:0", "@hwnd:x:77"] {
            assert!(matches!(
                parse_stable_target(invalid),
                Err(WindowError::InvalidStableTarget { .. })
            ));
        }
    }
}
