//! Windows UI Automation ground-truth grounding. Enumerates the on-screen UI
//! element tree (name + control type + exact bounding rect) so the model can
//! pick elements by name and we click the TRUE coordinates — instead of asking
//! a token-starved VLM to read pixels. Our edge over screenshot-only agents.
//!
//! `--cc-uia-dump` (optionally `CC_UIA_WINDOW=<title substring>` to target a
//! specific top-level window instead of the foreground one).

use anyhow::{Result, anyhow};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Children, TreeScope_Descendants,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowTextW,
    GetWindowThreadProcessId, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_MINIMIZE, SW_RESTORE,
    SetForegroundWindow, SetWindowPos, ShowWindow,
};

#[derive(Debug, Clone)]
pub(super) struct UiElement {
    pub name: String,
    pub control_type: &'static str,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub enabled: bool,
}

impl UiElement {
    /// Center in physical screen pixels.
    pub fn center(&self) -> (i32, i32) {
        ((self.left + self.right) / 2, (self.top + self.bottom) / 2)
    }
}

fn control_type_name(id: i32) -> &'static str {
    match id {
        50000 => "Button",
        50002 => "CheckBox",
        50003 => "ComboBox",
        50004 => "Edit",
        50005 => "Hyperlink",
        50006 => "Image",
        50007 => "ListItem",
        50008 => "List",
        50009 => "Menu",
        50010 => "MenuBar",
        50011 => "MenuItem",
        50013 => "RadioButton",
        50015 => "Slider",
        50018 => "Tab",
        50019 => "TabItem",
        50020 => "Text",
        50021 => "ToolBar",
        50023 => "Tree",
        50024 => "TreeItem",
        50025 => "Custom",
        50026 => "Group",
        50030 => "Document",
        50031 => "SplitButton",
        50032 => "Window",
        50033 => "Pane",
        50036 => "Table",
        50037 => "TitleBar",
        _ => "Other",
    }
}

/// Enumerate on-screen UIA elements of the target window (or the foreground
/// window if `target` is None). Filters out zero-area and offscreen elements.
pub(super) fn enumerate(target: Option<&str>) -> Result<Vec<UiElement>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let root = pick_window(&uia, target)?;
        let cond = uia.CreateTrueCondition()?;
        let arr = root.FindAll(TreeScope_Descendants, &cond)?;
        let n = arr.Length()?;
        let mut out = Vec::new();
        for i in 0..n {
            let Ok(el) = arr.GetElement(i) else { continue };
            let rect = el.CurrentBoundingRectangle().unwrap_or_default();
            if rect.right <= rect.left || rect.bottom <= rect.top {
                continue; // no on-screen area
            }
            if el.CurrentIsOffscreen().map(|b| b.as_bool()).unwrap_or(true) {
                continue;
            }
            let name = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
            let ct = el.CurrentControlType().map(|c| c.0).unwrap_or(0);
            let enabled = el.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(false);
            out.push(UiElement {
                name,
                control_type: control_type_name(ct),
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
                enabled,
            });
        }
        Ok(out)
    }
}

unsafe fn pick_window(uia: &IUIAutomation, target: Option<&str>) -> Result<IUIAutomationElement> {
    unsafe {
        if let Some(t) = target {
            let want = t.to_lowercase();
            // Prefer the FOREGROUND window when it matches — disambiguates several
            // windows of the same app (e.g. multiple Chrome windows), which a
            // first-match scan would otherwise pick wrongly.
            if let Ok(fg) = uia.ElementFromHandle(GetForegroundWindow()) {
                let name = fg.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                if name.to_lowercase().contains(&want) {
                    return Ok(fg);
                }
            }
            let root = uia.GetRootElement()?;
            let cond = uia.CreateTrueCondition()?;
            let children = root.FindAll(TreeScope_Children, &cond)?;
            for i in 0..children.Length()? {
                let Ok(el) = children.GetElement(i) else { continue };
                let name = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                if name.to_lowercase().contains(&want) {
                    return Ok(el);
                }
            }
            return Err(anyhow!("no top-level window matching {t:?}"));
        }
        Ok(uia.ElementFromHandle(GetForegroundWindow())?)
    }
}

/// Screen-pixel rect (x, y, w, h) of the target window (or foreground window if
/// `target` is None). Used as the default crop/view for the model.
pub(super) fn target_window_rect(target: Option<&str>) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let win = pick_window(&uia, target).ok()?;
        let r = win.CurrentBoundingRectangle().ok()?;
        if r.right > r.left && r.bottom > r.top {
            Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
        } else {
            None
        }
    }
}

/// Bring the top-level window whose title contains `target` to the foreground
/// (restoring it if minimized), and VERIFY it actually took. General-purpose:
/// used at startup for `CC_UIA_WINDOW` scoping AND by the agent's `focus_window`
/// tool to switch to any app by name (e.g. when a window it opened is hidden
/// behind a fullscreen game). Returns false if no window matched OR the switch
/// could not be forced (e.g. an exclusive-fullscreen app owns the foreground —
/// which nothing short of minimizing it can move, elevated or not).
pub(super) fn raise_window(target: &str) -> bool {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(uia) =
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        else {
            return false;
        };
        let Ok(el) = pick_window(&uia, Some(target)) else {
            return false;
        };
        let Ok(hwnd) = el.CurrentNativeWindowHandle() else {
            return false;
        };
        if hwnd.0.is_null() {
            return false;
        }
        force_foreground_hwnd(hwnd)
    }
}

/// Titles of the visible top-level windows — the agent's situational awareness
/// of what's open to switch to (`focus_window`) or push aside (`minimize_window`).
/// Best-effort; empty on failure.
pub(super) fn list_windows() -> Vec<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(uia) =
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        else {
            return Vec::new();
        };
        let (Ok(root), Ok(cond)) = (uia.GetRootElement(), uia.CreateTrueCondition()) else {
            return Vec::new();
        };
        let Ok(children) = root.FindAll(TreeScope_Children, &cond) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Ok(len) = children.Length() {
            for i in 0..len {
                let Ok(el) = children.GetElement(i) else { continue };
                let name = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                let name = name.trim().to_string();
                if !name.is_empty() && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        out
    }
}

/// Minimize the top-level window whose title contains `target` via a DIRECT
/// `ShowWindow` on its handle (no input injection — so it works even on a
/// fullscreen game that's swallowing keystrokes, unlike a synthetic Win+D).
/// Returns whether a window matched. Non-elevated.
pub(super) fn minimize_window(target: &str) -> bool {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(uia) =
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        else {
            return false;
        };
        let Ok(el) = pick_window(&uia, Some(target)) else {
            return false;
        };
        let Ok(hwnd) = el.CurrentNativeWindowHandle() else {
            return false;
        };
        if hwnd.0.is_null() {
            return false;
        }
        let _ = ShowWindow(hwnd, SW_MINIMIZE);
        true
    }
}

/// Resolve the top-level window whose title contains `target` to its handle.
unsafe fn find_window(target: &str) -> Option<HWND> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let el = pick_window(&uia, Some(target)).ok()?;
        let hwnd = el.CurrentNativeWindowHandle().ok()?;
        (!hwnd.0.is_null()).then_some(hwnd)
    }
}

/// Resize the window matching `target` to `w`x`h` pixels (restoring it first, so
/// a maximized window can shrink). Keeps its position. Non-elevated.
pub(super) fn resize_window(target: &str, w: i32, h: i32) -> bool {
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
pub(super) fn move_window(target: &str, x: i32, y: i32) -> bool {
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
            let _ = ShowWindow(hwnd, SW_RESTORE);
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

/// Re-assert the foreground window as the focused keyboard target before sending
/// keys, so keystrokes land on the on-screen window even if focus has drifted
/// (e.g. to this app's own overlay). The `AttachThreadInput` trick lets a
/// background process legally call `SetForegroundWindow`/`SetFocus`. Best-effort;
/// a web canvas's DOM focus still requires a click on it first.
pub(super) fn focus_foreground() {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return;
        }
        // Diagnostic: which on-screen window will receive the keystrokes? If this
        // logs our own overlay / SGT window instead of the target app, that's why
        // app-specific shortcuts (Explorer Ctrl+A, F2, …) don't land.
        let mut buf = [0u16; 128];
        let n = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        eprintln!("[cc] keys -> '{}'", title.chars().take(50).collect::<String>());
        let this_tid = GetCurrentThreadId();
        let fg_tid = GetWindowThreadProcessId(hwnd, None);
        let attach = fg_tid != 0 && fg_tid != this_tid;
        if attach {
            let _ = AttachThreadInput(this_tid, fg_tid, true);
        }
        // Re-assert the window as foreground if focus has DRIFTED (no-op if it's
        // already foreground). We deliberately do NOT call SetFocus(hwnd) here -
        // that moves keyboard focus to the TOP-LEVEL window, wiping the child
        // control (e.g. Explorer's file list) that a prior click just focused, so
        // shortcuts like Shift+Delete / F2 land on nothing.
        let _ = SetForegroundWindow(hwnd);
        if attach {
            let _ = AttachThreadInput(this_tid, fg_tid, false);
        }
    }
}

/// Per-turn grounding context: (active window title, cursor x, cursor y, the
/// accessible element currently under the cursor). One COM pass. Best-effort —
/// empty strings if unavailable.
pub(super) fn pointer_context() -> (String, i32, i32) {
    unsafe {
        // Pure Win32 (fast): foreground window title + cursor position. We
        // deliberately do NOT do a UIA ElementFromPoint here - that second
        // CoCreate + tree-walk cost ~100-300ms PER TURN for little value.
        let hwnd = GetForegroundWindow();
        let mut buf = [0u16; 128];
        let n = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        (title, p.x, p.y)
    }
}

/// Virtual desktop origin + size in physical px (x, y, w, h).
pub(super) fn virtual_desktop() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

/// CLI: dump the element tree with physical px + normalized 0-1000 centers.
pub fn run_dump(target: Option<&str>) -> Result<()> {
    let els = enumerate(target)?;
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    eprintln!(
        "[uia] {} on-screen elements (target={:?}; virtual desktop {vw}x{vh} @ {vx},{vy})",
        els.len(),
        target
    );
    let named: Vec<_> = els.iter().filter(|e| !e.name.trim().is_empty()).collect();
    eprintln!("[uia] {} of them have a non-empty name:", named.len());
    for (i, e) in named.iter().enumerate().take(150) {
        let (cx, cy) = e.center();
        let nx = ((cx - vx) as f64 / vw.max(1) as f64 * 1000.0).round() as i32;
        let ny = ((cy - vy) as f64 / vh.max(1) as f64 * 1000.0).round() as i32;
        eprintln!(
            "[uia] {i:>3} {:<10} norm=({nx:>4},{ny:>4}) {}\"{}\"",
            e.control_type,
            if e.enabled { "" } else { "[disabled] " },
            e.name
        );
    }
    Ok(())
}
