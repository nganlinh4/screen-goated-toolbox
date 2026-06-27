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
    CUIAutomation, ExpandCollapseState_Collapsed, ExpandCollapseState_Expanded,
    ExpandCollapseState_PartiallyExpanded, IUIAutomation, IUIAutomationElement,
    IUIAutomationExpandCollapsePattern, IUIAutomationRangeValuePattern,
    IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern, IUIAutomationValuePattern,
    ToggleState_Indeterminate, ToggleState_On, TreeScope_Children, TreeScope_Descendants,
    UIA_ExpandCollapsePatternId, UIA_RangeValuePatternId, UIA_SelectionItemPatternId,
    UIA_TogglePatternId, UIA_ValuePatternId,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowTextW,
    GetWindowThreadProcessId, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SetForegroundWindow,
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
    /// Ground-truth control state from UIA patterns (e.g. "on"/"off"/"selected"/
    /// "expanded"/"value 30"), when the element exposes one - so the model reads
    /// state as text instead of (unreliably) eyeballing a few-pixel toggle.
    pub state: Option<String>,
    /// Current text of a value-bearing control (Edit / Document / ComboBox) via
    /// UIA ValuePattern: `Some("")` for an empty field, `None` for a control with
    /// no value concept. Powers the native controller's perception + fill verify.
    pub value: Option<String>,
    /// The control must be filled for its form to be valid (UIA IsRequiredForForm).
    pub required: bool,
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

/// Best-effort ground-truth STATE of an interactive element, via UIA control
/// patterns - the on/off, selected, expanded, or value the model would otherwise
/// have to (unreliably) read from a few pixels. `None` when it exposes no state.
/// Unsupported patterns return `Err` (windows-rs null-checks the out-pointer), so
/// the `if let Ok` chains simply fall through.
unsafe fn read_state(el: &IUIAutomationElement) -> Option<String> {
    unsafe {
        // Toggle = checkbox / switch / toggle-button (on | off | mixed).
        if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
            && let Ok(st) = p.CurrentToggleState()
        {
            let s = if st == ToggleState_On {
                "on"
            } else if st == ToggleState_Indeterminate {
                "mixed"
            } else {
                "off"
            };
            return Some(s.to_string());
        }
        // Selection = tab / list item / radio: surface only the SELECTED one.
        if let Ok(p) =
            el.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
            && let Ok(sel) = p.CurrentIsSelected()
            && sel.as_bool()
        {
            return Some("selected".to_string());
        }
        // Expand/collapse = tree / menu / combo open state (LeafNode → no tag).
        if let Ok(p) = el
            .GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId)
            && let Ok(st) = p.CurrentExpandCollapseState()
        {
            if st == ExpandCollapseState_Expanded {
                return Some("expanded".to_string());
            } else if st == ExpandCollapseState_Collapsed {
                return Some("collapsed".to_string());
            } else if st == ExpandCollapseState_PartiallyExpanded {
                return Some("partly-expanded".to_string());
            }
        }
        // Range = slider / progress: the numeric value (trimmed of float noise).
        if let Ok(p) =
            el.GetCurrentPatternAs::<IUIAutomationRangeValuePattern>(UIA_RangeValuePatternId)
            && let Ok(v) = p.CurrentValue()
        {
            let r = (v * 100.0).round() / 100.0;
            return Some(format!("value {r}"));
        }
        None
    }
}

/// Current text of a value-bearing control via UIA ValuePattern: `Some("")` for an
/// empty field (present but blank), `None` when the control exposes no value at all.
unsafe fn read_value(el: &IUIAutomationElement) -> Option<String> {
    unsafe {
        if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            && let Ok(v) = p.CurrentValue()
        {
            return Some(v.to_string());
        }
        None
    }
}

/// Run `f` on a throwaway worker thread (its own COM apartment / input queue) with a hard time cap,
/// returning `fallback` if it hasn't finished in time. A whole CLASS of Win32/UIA calls have NO
/// timeout and block forever on an unresponsive provider - a cross-process UIA tree walk, a
/// `WM_GETTEXT` to a non-pumping window, a `SetForegroundWindow` to a wedged thread - and any one of
/// them froze the agent indefinitely (`click_at` / `run_command` / `list_windows`). This is the
/// single guard so one stuck window can never hang the loop: the worker keeps running in the
/// background and self-cleans, we just stop waiting. `T: Send` so the result crosses the boundary.
fn with_timeout<T: Send + 'static>(
    label: &str,
    secs: u64,
    fallback: T,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(std::time::Duration::from_secs(secs)) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("[cc] UIA {label} timed out (>{secs}s) - using fallback");
            fallback
        }
    }
}

/// Enumerate the on-screen UIA elements of `target` (foreground if None), filtering zero-area and
/// offscreen elements. `FindAll` walks the WHOLE descendant tree with no timeout; on a stall we
/// return empty so grounding falls back to the visual marks instead of freezing (see `with_timeout`).
pub(super) fn enumerate(target: Option<&str>) -> Result<Vec<UiElement>> {
    let owned = target.map(str::to_string);
    with_timeout("enumerate", 6, Ok(Vec::new()), move || {
        enumerate_inner(owned.as_deref())
    })
}

fn enumerate_inner(target: Option<&str>) -> Result<Vec<UiElement>> {
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
            // Read control state only for named elements (the ones we display), to
            // bound the extra per-element pattern probes.
            let state = if name.trim().is_empty() { None } else { read_state(&el) };
            // Text value (Edit/Document/ComboBox) + required-for-form — powers the
            // native controller's perception, gates, and fill verification.
            let value = if matches!(ct, 50004 | 50030 | 50003) { read_value(&el) } else { None };
            let required = el.CurrentIsRequiredForForm().map(|b| b.as_bool()).unwrap_or(false);
            out.push(UiElement {
                name,
                control_type: control_type_name(ct),
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
                enabled,
                state,
                value,
                required,
            });
        }
        Ok(out)
    }
}

/// The text value of whatever value-bearing control sits at screen point (x, y),
/// via UIA `ElementFromPoint` + ValuePattern — the native controller's fill
/// read-back. Timeout-guarded (ElementFromPoint is a no-timeout cross-process call).
pub(super) fn read_value_at(x: i32, y: i32) -> Option<String> {
    with_timeout("read_value_at", 4, None, move || read_value_at_inner(x, y))
}

fn read_value_at_inner(x: i32, y: i32) -> Option<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let el = uia.ElementFromPoint(POINT { x, y }).ok()?;
        read_value(&el)
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
/// `target` is None). Used as the default crop/view for the model. Wrapped in
/// `with_timeout` because `pick_window` + `CurrentBoundingRectangle` are
/// cross-process UIA calls with no timeout (the same hang class as `enumerate`).
pub(super) fn target_window_rect(target: Option<&str>) -> Option<(i32, i32, i32, i32)> {
    let owned = target.map(str::to_string);
    with_timeout("target_window_rect", 6, None, move || {
        target_window_rect_inner(owned.as_deref())
    })
}

fn target_window_rect_inner(target: Option<&str>) -> Option<(i32, i32, i32, i32)> {
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

/// Re-assert the foreground window as the focused keyboard target before sending
/// keys, so keystrokes land on the on-screen window even if focus has drifted
/// (e.g. to this app's own overlay). The `AttachThreadInput` trick lets a
/// background process legally call `SetForegroundWindow`/`SetFocus`. Best-effort;
/// a web canvas's DOM focus still requires a click on it first. Wrapped in
/// `with_timeout`: `GetWindowTextW` (WM_GETTEXT) blocks forever on a non-pumping
/// window and `SetForegroundWindow` can stall on a wedged thread - the same hang
/// class - so a stuck target can never freeze the key-send path.
pub(super) fn focus_foreground() {
    with_timeout("focus_foreground", 4, (), focus_foreground_inner);
}

fn focus_foreground_inner() {
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
/// empty strings if unavailable. Wrapped in `with_timeout` (it runs every turn
/// and `GetWindowTextW` can block on a non-pumping foreground window) so
/// grounding never freezes here.
pub(super) fn pointer_context() -> (String, i32, i32) {
    with_timeout("pointer_context", 4, (String::new(), 0, 0), pointer_context_inner)
}

fn pointer_context_inner() -> (String, i32, i32) {
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

/// True when the foreground window is a Chromium browser. Chrome, Edge, Brave,
/// Opera and Vivaldi all use the `Chrome_WidgetWin_1` top-level window class -
/// a signal that is brand-independent, NOT localized, and (unlike the title) not
/// subject to truncation. Used to decide whether a vision-located click/drag
/// should be driven through the browser's OWN trusted input pipeline (CDP)
/// instead of synthetic OS mouse events, which canvas/WebGL web games ignore.
pub(super) fn foreground_is_chromium() -> bool {
    unsafe {
        let mut buf = [0u16; 64];
        let n = GetClassNameW(GetForegroundWindow(), &mut buf);
        String::from_utf16_lossy(&buf[..n.max(0) as usize]) == "Chrome_WidgetWin_1"
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

mod windowing;
pub(crate) use windowing::{
    list_windows, minimize_window, move_window, raise_window, resize_window,
};