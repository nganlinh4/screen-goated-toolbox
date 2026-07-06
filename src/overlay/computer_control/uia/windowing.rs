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
    match find_top_window(target) {
        Some(hwnd) => unsafe { force_foreground_hwnd(hwnd) },
        None => false,
    }
}

// --- Win32 EnumWindows: finds EVERY top-level window, including fullscreen GAMES (Unity/Unreal/
//     native) that expose NO UI Automation provider and so never appear in the UIA tree. That gap
//     is exactly why `focus_window` couldn't reach a minimized game and matched a browser tab by
//     title instead. Mirrors the proven enumeration in `realtime_webview::app_selection`. ---

/// Executable basename (e.g. "GenshinImpact.exe") owning `pid`; "" on failure.
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

/// Resolve `target` (a title OR exe-name substring, case-insensitive) to a top-level HWND. Ranks an
/// EXE-name match above a title-only match — so an app-name target finds the
/// real app window, not a browser tab whose title happens to mention it.
pub(crate) fn find_top_window(target: &str) -> Option<HWND> {
    let want = target.trim().to_lowercase();
    if want.is_empty() {
        return None;
    }
    let fg = unsafe { GetForegroundWindow() };
    let mut best: Option<HWND> = None;
    let mut best_score = i32::MIN;
    for (hwnd, title, exe) in enum_top_windows() {
        let exe_hit = exe.to_lowercase().contains(&want);
        if !exe_hit && !title.to_lowercase().contains(&want) {
            continue;
        }
        let score = i32::from(exe_hit) * 2 + i32::from(hwnd.0 == fg.0);
        if score > best_score {
            best_score = score;
            best = Some(hwnd);
        }
    }
    best
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
