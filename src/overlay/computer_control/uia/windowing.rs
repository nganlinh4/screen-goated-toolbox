//! Win32 top-level window management: find / raise / minimize / resize / move a
//! window by a piece of its title or exe name, including fullscreen games that
//! expose no UIA provider. Split from the UIA tree code (`uia.rs`) for the
//! file-size limit. Self-contained — only Win32 windowing + process APIs, no UIA.

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, SW_MINIMIZE, SW_RESTORE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowPos, ShowWindow,
};
use windows_core::BOOL;

/// Bring the top-level window whose title contains `target` to the foreground
/// (restoring it if minimized), and VERIFY it actually took. General-purpose:
/// used at startup for `CC_UIA_WINDOW` scoping AND by the agent's `focus_window`
/// tool to switch to any app by name (e.g. when a window it opened is hidden
/// behind a fullscreen game). Returns false if no window matched OR the switch
/// could not be forced (e.g. an exclusive-fullscreen app owns the foreground —
/// which nothing short of minimizing it can move, elevated or not).
pub(crate) fn raise_window(target: &str) -> bool {
    let before = unsafe { GetForegroundWindow() };
    let selected = find_top_window(target);
    let raised = selected.is_some_and(|hwnd| unsafe { force_foreground_hwnd(hwnd) });
    let after = unsafe { GetForegroundWindow() };
    super::super::telemetry::event(
        "focus_window_result",
        "windowing",
        super::super::telemetry::Privacy::Safe,
        serde_json::json!({
            "requested_target": target,
            "selected_hwnd": selected.map(|hwnd| hwnd.0 as usize),
            "foreground_before_hwnd": before.0 as usize,
            "foreground_after_hwnd": after.0 as usize,
            "raised": raised,
            "verified": selected.is_some_and(|hwnd| hwnd.0 == after.0),
        }),
    );
    raised
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
pub(crate) fn enum_top_windows() -> Vec<(HWND, String, String)> {
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
            if title.is_empty() || title == "Program Manager" || title == "Settings" {
                return BOOL(1);
            }
            let out = &mut *(lparam.0 as *mut Vec<(HWND, String, String)>);
            out.push((hwnd, title, process_exe_name(pid)));
            BOOL(1)
        }
    }
    let mut out: Vec<(HWND, String, String)> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

/// Resolve `target` (a title OR exe-name substring, case-insensitive) to a top-level HWND.
/// Exact/executable and title-prefix matches outrank a loose title substring. That
/// keeps a real app window ahead of another window whose document title merely
/// mentions the requested app. Foreground is only a tie-breaker; it must never make
/// a weak substring beat a precise match.
pub(crate) fn find_top_window(target: &str) -> Option<HWND> {
    let want = target.trim().to_lowercase();
    if want.is_empty() {
        return None;
    }
    let fg = unsafe { GetForegroundWindow() };
    let mut best: Option<(HWND, String, String, i32, bool)> = None;
    let mut best_score = i32::MIN;
    let mut matched_count = 0usize;
    for (hwnd, title, exe) in enum_top_windows() {
        let foreground = hwnd.0 == fg.0;
        let Some(score) = window_match_score(&want, &title, &exe, foreground) else {
            continue;
        };
        matched_count += 1;
        if score > best_score {
            best_score = score;
            best = Some((hwnd, title, exe, score, foreground));
        }
    }
    let mut pid = 0u32;
    if let Some((hwnd, ..)) = best.as_ref() {
        unsafe { GetWindowThreadProcessId(*hwnd, Some(&mut pid)) };
    }
    super::super::telemetry::event(
        "window_resolution",
        "windowing",
        super::super::telemetry::Privacy::Safe,
        serde_json::json!({
            "requested_target": target,
            "normalized_target": want,
            "matched_count": matched_count,
            "selected": best.as_ref().map(|(hwnd, title, exe, score, foreground)| serde_json::json!({
                "hwnd": hwnd.0 as usize,
                "pid": pid,
                "title": title,
                "exe": exe,
                "score": score,
                "was_foreground": foreground,
            })),
        }),
    );
    best.map(|(hwnd, ..)| hwnd)
}

fn window_match_score(want: &str, title: &str, exe: &str, foreground: bool) -> Option<i32> {
    let title = title.trim().to_lowercase();
    let exe = exe.trim().to_lowercase();
    let exe_stem = exe.strip_suffix(".exe").unwrap_or(&exe);

    let (base, matched_len) = if exe == want || exe_stem == want {
        (1_000, exe_stem.len())
    } else if exe_stem.starts_with(want) {
        (900, exe_stem.len())
    } else if exe_stem.contains(want) {
        (800, exe_stem.len())
    } else if title == want {
        (700, title.len())
    } else if title.starts_with(want) {
        (600, title.len())
    } else if title.contains(want) {
        (400, title.len())
    } else {
        return None;
    };

    // Prefer the least-surprising match within the same tier: less unrelated
    // trailing title text, then the window already in front.
    let extra = matched_len.saturating_sub(want.len()).min(100) as i32;
    Some(base - extra + i32::from(foreground) * 10)
}

/// Labels of the open top-level windows — the agent's situational awareness of what's open to
/// switch to (`focus_window`) or push aside (`minimize_window`). Each is `"title  [exe]"` so the
/// agent can disambiguate a fullscreen game/app from a browser tab whose title
/// mentions the same thing. Best-effort.
pub(crate) fn list_windows() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (_, title, exe) in enum_top_windows() {
        let label = if exe.is_empty() {
            title
        } else {
            format!("{title}  [{exe}]")
        };
        if !out.contains(&label) {
            out.push(label);
        }
    }
    out
}

/// Minimize the top-level window whose title contains `target` via a DIRECT
/// `ShowWindow` on its handle (no input injection — so it works even on a
/// fullscreen game that's swallowing keystrokes, unlike a synthetic Win+D).
/// Returns whether a window matched. Non-elevated.
pub(crate) fn minimize_window(target: &str) -> bool {
    match find_top_window(target) {
        Some(hwnd) => unsafe {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
            true
        },
        None => false,
    }
}

/// Resolve the top-level window whose title contains `target` to its handle.
fn find_window(target: &str) -> Option<HWND> {
    find_top_window(target)
}

/// Resize the window matching `target` to `w`x`h` pixels (restoring it first, so
/// a maximized window can shrink). Keeps its position. Non-elevated.
pub(crate) fn resize_window(target: &str, w: i32, h: i32) -> bool {
    if w <= 0 || h <= 0 {
        return false;
    }
    unsafe {
        let Some(hwnd) = find_window(target) else {
            return false;
        };
        let _ = ShowWindow(hwnd, SW_RESTORE); // can't resize while maximized
        SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER).is_ok()
    }
}

/// Move the window matching `target` so its top-left corner is at screen pixel
/// (x, y). Keeps its size. Non-elevated.
pub(crate) fn move_window(target: &str, x: i32, y: i32) -> bool {
    unsafe {
        let Some(hwnd) = find_window(target) else {
            return false;
        };
        let _ = ShowWindow(hwnd, SW_RESTORE); // can't move while maximized
        SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER).is_ok()
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
            // Only UN-MINIMIZE; never SW_RESTORE a MAXIMIZED window — that un-maximizes it (the user
            // saw a maximized Chrome shrink when focus_window switched to it). SW_RESTORE on a minimized
            // window correctly brings back its prior maximized/normal state.
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
    use super::window_match_score;

    #[test]
    fn executable_match_beats_document_title_mention() {
        let app = window_match_score("editor", "Untitled", "editor.exe", false).unwrap();
        let mention = window_match_score(
            "editor",
            "Editor usage guide - Web Browser",
            "browser.exe",
            true,
        )
        .unwrap();
        assert!(app > mention);
    }

    #[test]
    fn title_prefix_beats_loose_foreground_substring() {
        let app = window_match_score("sample app", "Sample App 2.0 - Project", "host.exe", false)
            .unwrap();
        let mention = window_match_score(
            "sample app",
            "Article about Sample App - Web Browser",
            "browser.exe",
            true,
        )
        .unwrap();
        assert!(app > mention);
    }

    #[test]
    fn foreground_breaks_ties_without_overriding_match_quality() {
        let background = window_match_score("notes", "Notes - A", "host.exe", false).unwrap();
        let foreground = window_match_score("notes", "Notes - B", "host.exe", true).unwrap();
        assert!(foreground > background);
    }
}
