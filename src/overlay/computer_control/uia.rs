//! Windows UI Automation ground-truth grounding. Enumerates the on-screen UI
//! element tree (name + control type + exact bounding rect) so the model can
//! pick elements by name and we click the TRUE coordinates — instead of asking
//! a token-starved VLM to read pixels. Our edge over screenshot-only agents.
//!
//! Diagnostic callers may target a specific top-level window instead of the
//! foreground one.

use anyhow::{Result, anyhow};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationValuePattern,
    UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GW_OWNER, GetClassNameW, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindow,
    GetWindowTextW, GetWindowThreadProcessId, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

#[derive(Debug, Clone)]
pub(super) struct UiElement {
    pub name: String,
    pub automation_id: String,
    pub runtime_id: Vec<i32>,
    pub control_type: &'static str,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub enabled: bool,
    pub protected: bool,
    /// Ground-truth control state from UIA patterns (e.g. "on"/"off"/"selected"/
    /// "expanded"/"value 30"), when the element exposes one - so the model reads
    /// state as text instead of (unreliably) eyeballing a few-pixel toggle.
    pub state: Option<String>,
    /// Bounded text observation and independent value/editing capability.
    /// Exact post-action verification uses a targeted authoritative read.
    pub value: value_preview::ValuePreview,
    /// The control must be filled for its form to be valid (UIA IsRequiredForForm).
    pub required: bool,
}

impl UiElement {
    /// Center in physical screen pixels.
    pub fn center(&self) -> (i32, i32) {
        ((self.left + self.right) / 2, (self.top + self.bottom) / 2)
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
    std::thread::spawn(super::session_retention::inherit(move || {
        let _ = tx.send(f());
    }));
    match rx.recv_timeout(std::time::Duration::from_secs(secs)) {
        Ok(v) => v,
        Err(_) => {
            cc_diagnostic!("[cc] UIA {label} timed out (>{secs}s) - using fallback");
            fallback
        }
    }
}

/// The text value of whatever value-bearing control sits at screen point (x, y),
/// via UIA `ElementFromPoint` + ValuePattern — the native controller's fill
/// read-back. Timeout-guarded (ElementFromPoint is a no-timeout cross-process call).
pub(super) enum ReadValue {
    Value(String),
    Protected,
}

pub(super) fn read_value_at(
    x: i32,
    y: i32,
    expected: ExpectedNativeElement<'_>,
    protected: bool,
) -> Option<ReadValue> {
    let role = expected.role.to_string();
    let name = expected.provider_name.to_string();
    let automation_id = expected.automation_id.to_string();
    let runtime_id = expected.runtime_id.to_vec();
    with_timeout("read_value_at", 4, None, move || {
        read_value_at_inner(x, y, &role, &name, &automation_id, &runtime_id, protected)
    })
}

fn read_value_at_inner(
    x: i32,
    y: i32,
    role: &str,
    name: &str,
    automation_id: &str,
    runtime_id: &[i32],
    protected: bool,
) -> Option<ReadValue> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let hit = uia.ElementFromPoint(POINT { x, y }).ok()?;
        let el =
            target::matching_observed_ancestor(&uia, hit, role, name, automation_id, runtime_id)?;
        let is_protected = el.CurrentIsPassword().ok()?.as_bool();
        if is_protected != protected {
            return None;
        }
        let result = if is_protected {
            ReadValue::Protected
        } else {
            ReadValue::Value(read_value(&el)?)
        };
        let hit_after = uia.ElementFromPoint(POINT { x, y }).ok()?;
        let after = target::matching_observed_ancestor(
            &uia,
            hit_after,
            role,
            name,
            automation_id,
            runtime_id,
        )?;
        if after.CurrentIsPassword().ok()?.as_bool() != protected {
            return None;
        }
        Some(result)
    }
}

unsafe fn pick_window(
    uia: &IUIAutomation,
    target: Option<&str>,
) -> Result<(IUIAutomationElement, HWND)> {
    let active = active_window(target)?;
    unsafe { Ok((uia.ElementFromHandle(active)?, active)) }
}

/// The window whose accessibility tree represents `target`: its owned
/// foreground dialog when one is active, otherwise the exactly resolved
/// top-level window; without a target, the foreground window. Win32 reads only,
/// so it can run before a timeout-guarded UIA worker is admitted.
fn active_window(target: Option<&str>) -> Result<HWND> {
    let foreground = unsafe { GetForegroundWindow() }.0 as usize;
    active_window_with(
        target,
        |selector| {
            windowing::find_top_window(selector)
                .map(|hwnd| hwnd.0 as usize)
                .map_err(anyhow::Error::new)
        },
        foreground,
        |candidate, root| unsafe {
            window_is_owned_by(HWND(candidate as *mut _), HWND(root as *mut _))
        },
    )
    .map(|raw| HWND(raw as *mut _))
}

fn active_window_with(
    target: Option<&str>,
    find: impl FnOnce(&str) -> Result<usize>,
    foreground: usize,
    owned_by: impl FnOnce(usize, usize) -> bool,
) -> Result<usize> {
    let Some(target) = target else {
        return Ok(foreground);
    };
    let hwnd = find(target)?;
    Ok(if foreground == hwnd || owned_by(foreground, hwnd) {
        foreground
    } else {
        hwnd
    })
}

unsafe fn window_is_owned_by(mut candidate: HWND, root: HWND) -> bool {
    unsafe {
        for _ in 0..16 {
            let Ok(owner) = GetWindow(candidate, GW_OWNER) else {
                return false;
            };
            if owner.0.is_null() {
                return false;
            }
            if owner == root {
                return true;
            }
            candidate = owner;
        }
        false
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
        let (win, _) = pick_window(&uia, target).ok()?;
        let r = win.CurrentBoundingRectangle().ok()?;
        if r.right > r.left && r.bottom > r.top {
            Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
        } else {
            None
        }
    }
}

pub(super) fn input_target_snapshot() -> serde_json::Value {
    with_timeout(
        "input_target_snapshot",
        2,
        serde_json::json!({"available": false, "reason": "timeout"}),
        || unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return serde_json::json!({"available": false, "reason": "no_foreground_window"});
            }
            let mut title_buf = [0u16; 256];
            let title_len = GetWindowTextW(hwnd, &mut title_buf).max(0) as usize;
            let mut class_buf = [0u16; 128];
            let class_len = GetClassNameW(hwnd, &mut class_buf).max(0) as usize;
            let mut pid = 0u32;
            let thread_id = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let generation = window_instance::known(hwnd.0 as usize as u64, u64::from(pid));
            serde_json::json!({
                "available": true,
                "hwnd": hwnd.0 as usize,
                "pid": pid,
                "thread_id": thread_id,
                "generation": generation,
                "title": String::from_utf16_lossy(&title_buf[..title_len]),
                "class": String::from_utf16_lossy(&class_buf[..class_len]),
            })
        },
    )
}

pub(super) fn foreground_stable_target() -> Option<String> {
    let target = input_target_snapshot();
    let hwnd = target.get("hwnd").and_then(serde_json::Value::as_u64)?;
    let pid = target.get("pid").and_then(serde_json::Value::as_u64)?;
    (hwnd > 0 && pid > 0).then(|| format!("@hwnd:{hwnd}:{pid}"))
}

pub(super) fn resolved_window_identity(target: Option<&str>) -> Result<(u64, u64)> {
    if let Some(target) = target {
        return windowing::window_identity(target).map_err(anyhow::Error::new);
    }
    let snapshot = input_target_snapshot();
    let available = snapshot
        .get("available")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let hwnd = snapshot
        .get("hwnd")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let pid = snapshot
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if !available || hwnd == 0 || pid == 0 {
        anyhow::bail!("concrete window HWND/PID unavailable");
    }
    Ok((hwnd, pid))
}

pub(super) fn observe_native_identity(target: Option<&str>) -> Result<(u64, u64, u64)> {
    let (hwnd, pid) = resolved_window_identity(target)?;
    let generation = window_instance::observe(hwnd, pid)?;
    Ok((hwnd, pid, generation))
}

pub(super) fn current_native_identity(target: Option<&str>) -> Result<(u64, u64, u64)> {
    let (hwnd, pid) = resolved_window_identity(target)?;
    let generation = window_instance::current(hwnd, pid)?;
    Ok((hwnd, pid, generation))
}

pub(super) fn validate_native_identity(hwnd: u64, pid: u64, generation: u64) -> Result<()> {
    window_instance::validate(hwnd, pid, generation)
}

/// Per-turn grounding context: (active window title, cursor x, cursor y, the
/// accessible element currently under the cursor). One COM pass. Best-effort —
/// empty strings if unavailable. Wrapped in `with_timeout` (it runs every turn
/// and `GetWindowTextW` can block on a non-pumping foreground window) so
/// grounding never freezes here.
pub(super) fn pointer_context() -> (String, i32, i32) {
    with_timeout(
        "pointer_context",
        4,
        (String::new(), 0, 0),
        pointer_context_inner,
    )
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

fn ensure_native_accessibility_authority(hwnd: HWND) -> Result<()> {
    if super::browser::owns_foreground_window(hwnd.0 as usize as u64) {
        anyhow::bail!(
            "native accessibility is not authoritative while the exact foreground window is owned by the browser provider"
        );
    }
    Ok(())
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

mod activation;
mod cached_element;
mod circuit;
pub(super) mod control_types;
mod enumeration;
mod target;
mod value_preview;
mod window_instance;
mod windowing;
pub(super) use activation::{ActivationError, FailureKind, activate_at};
pub(super) use enumeration::{enumerate, enumerate_best_effort, enumerate_for_snapshot};
pub(super) use target::{
    ExpectedNativeElement, focused_editable_guard, validate_focused_editable,
    validate_native_element_at, validate_native_focused_element,
    validate_native_provider_ownership,
};
pub(crate) use windowing::{
    WindowError, list_windows, minimize_window, move_window, raise_window,
    raise_window_with_target, resize_window, stable_window_target, window_identity,
};
