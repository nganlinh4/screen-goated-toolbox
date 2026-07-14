use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{E_NOINTERFACE, E_POINTER, HWND, POINT};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, ExpandCollapseState, ExpandCollapseState_Collapsed,
    ExpandCollapseState_Expanded, ExpandCollapseState_PartiallyExpanded, IUIAutomation,
    IUIAutomationElement, IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern,
    IUIAutomationLegacyIAccessiblePattern, IUIAutomationSelectionItemPattern,
    UIA_E_ELEMENTNOTAVAILABLE, UIA_E_NOTSUPPORTED, UIA_ExpandCollapsePatternId,
    UIA_InvokePatternId, UIA_LegacyIAccessiblePatternId, UIA_SelectionItemPatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetForegroundWindow, WindowFromPoint,
};

const DISPATCH_PENDING: u8 = 0;
const DISPATCH_STARTED: u8 = 1;
const DISPATCH_EXPIRED: u8 = 2;
const DISPATCH_CANCELLED: u8 = 3;
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(4);
const CANCEL_POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expansion {
    Collapsed,
    Expanded,
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Capabilities {
    invoke: bool,
    expansion: Option<Expansion>,
    legacy_default: bool,
    selection_item: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    Invoke,
    Expand,
    Collapse,
    LegacyDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unsupported {
    SelectionOnly,
    NoDefaultAction,
}

fn choose_activation(capabilities: Capabilities) -> Result<Plan, Unsupported> {
    if capabilities.invoke {
        return Ok(Plan::Invoke);
    }
    match capabilities.expansion {
        Some(Expansion::Collapsed) => return Ok(Plan::Expand),
        Some(Expansion::Expanded) => return Ok(Plan::Collapse),
        Some(Expansion::Other) | None => {}
    }
    if capabilities.legacy_default {
        return Ok(Plan::LegacyDefault);
    }
    Err(if capabilities.selection_item {
        Unsupported::SelectionOnly
    } else {
        Unsupported::NoDefaultAction
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::overlay::computer_control) enum FailureKind {
    Unsupported,
    StaleTarget,
    Setup,
    TargetQuery,
    CapabilityQuery,
    Dispatch,
    Cancelled,
    Timeout,
}

#[derive(Debug, Clone)]
pub(in crate::overlay::computer_control) struct ActivationError {
    kind: FailureKind,
    message: String,
    effect_may_have_occurred: bool,
}

impl ActivationError {
    fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            effect_may_have_occurred: false,
        }
    }

    fn after_dispatch(kind: FailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            effect_may_have_occurred: true,
        }
    }

    pub(in crate::overlay::computer_control) fn kind(&self) -> FailureKind {
        self.kind
    }

    pub(in crate::overlay::computer_control) fn effect_may_have_occurred(&self) -> bool {
        self.effect_may_have_occurred
    }

    fn timeout(effect_may_have_occurred: bool) -> Self {
        let message = "native default-action operation timed out";
        if effect_may_have_occurred {
            Self::after_dispatch(FailureKind::Timeout, message)
        } else {
            Self::new(FailureKind::Timeout, message)
        }
    }

    fn cancelled(effect_may_have_occurred: bool) -> Self {
        let message = "native default-action operation was cancelled";
        if effect_may_have_occurred {
            Self::after_dispatch(FailureKind::Cancelled, message)
        } else {
            Self::new(FailureKind::Cancelled, message)
        }
    }

    #[cfg(test)]
    pub(in crate::overlay::computer_control) fn test_failure(
        kind: FailureKind,
        effect_may_have_occurred: bool,
    ) -> Self {
        if effect_may_have_occurred {
            Self::after_dispatch(kind, "test action failure")
        } else {
            Self::new(kind, "test action failure")
        }
    }
}

impl fmt::Display for ActivationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ActivationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExpectedNativeWindow {
    hwnd: u64,
    pid: u64,
    generation: u64,
}

struct ExpectedActivationTarget {
    role: String,
    name: String,
    automation_id: String,
    runtime_id: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::overlay::computer_control) struct ActivationReceipt {
    method: &'static str,
    dry_run: bool,
}

impl ActivationReceipt {
    pub(in crate::overlay::computer_control) fn method(self) -> &'static str {
        self.method
    }

    pub(in crate::overlay::computer_control) fn dry_run(self) -> bool {
        self.dry_run
    }
}

/// Resolve and dispatch a native control's structurally exposed UIA default
/// action. The observed window identity is checked again at the final dispatch
/// edge; a timed-out preflight is expired so its worker cannot invoke later.
pub(in crate::overlay::computer_control) fn activate_at(
    point: (i32, i32),
    element: super::target::ExpectedNativeElement<'_>,
    expected_window: (u64, u64, u64),
    dry_run: bool,
    cancel: &AtomicBool,
) -> Result<ActivationReceipt, ActivationError> {
    if cancel.load(Ordering::Acquire) {
        return Err(ActivationError::cancelled(false));
    }
    let (x, y) = point;
    let element = ExpectedActivationTarget {
        role: element.role.to_string(),
        name: element.provider_name.to_string(),
        automation_id: element.automation_id.to_string(),
        runtime_id: element.runtime_id.to_vec(),
    };
    let expected_window = ExpectedNativeWindow {
        hwnd: expected_window.0,
        pid: expected_window.1,
        generation: expected_window.2,
    };
    let dispatch_state = Arc::new(AtomicU8::new(DISPATCH_PENDING));
    let worker_state = Arc::clone(&dispatch_state);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = activate_at_inner(x, y, &element, expected_window, dry_run, &worker_state);
        let _ = tx.send(result);
    });

    wait_for_result(rx, &dispatch_state, cancel)
}

fn wait_for_result(
    rx: std::sync::mpsc::Receiver<Result<ActivationReceipt, ActivationError>>,
    dispatch_state: &AtomicU8,
    cancel: &AtomicBool,
) -> Result<ActivationReceipt, ActivationError> {
    let deadline = Instant::now() + ACTIVATION_TIMEOUT;
    loop {
        if cancel.load(Ordering::Acquire) {
            let prevented = stop_pending_dispatch(dispatch_state, DISPATCH_CANCELLED);
            return Err(ActivationError::cancelled(!prevented));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            eprintln!("[cc] UIA activate_at timed out (>4s) - rejecting late dispatch");
            let prevented = stop_pending_dispatch(dispatch_state, DISPATCH_EXPIRED);
            return Err(ActivationError::timeout(!prevented));
        }
        match rx.recv_timeout(remaining.min(CANCEL_POLL)) {
            Ok(result) => return received_result(result, dispatch_state, cancel),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(ActivationError::new(
                    FailureKind::Setup,
                    "native default-action worker stopped before returning a result",
                ));
            }
        }
    }
}

fn received_result(
    result: Result<ActivationReceipt, ActivationError>,
    dispatch_state: &AtomicU8,
    cancel: &AtomicBool,
) -> Result<ActivationReceipt, ActivationError> {
    if cancel.load(Ordering::Acquire) {
        let prevented = stop_pending_dispatch(dispatch_state, DISPATCH_CANCELLED);
        Err(ActivationError::cancelled(!prevented))
    } else {
        result
    }
}

fn stop_pending_dispatch(dispatch_state: &AtomicU8, stopped: u8) -> bool {
    dispatch_state
        .compare_exchange(
            DISPATCH_PENDING,
            stopped,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
}

fn activate_at_inner(
    x: i32,
    y: i32,
    element: &ExpectedActivationTarget,
    expected_window: ExpectedNativeWindow,
    dry_run: bool,
    dispatch_state: &AtomicU8,
) -> Result<ActivationReceipt, ActivationError> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|error| stage_error(FailureKind::Setup, "initialize COM", error))?;
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| {
                stage_error(FailureKind::Setup, "create UI Automation client", error)
            })?;
        let hit = uia.ElementFromPoint(POINT { x, y }).map_err(|error| {
            stage_error(
                FailureKind::TargetQuery,
                "resolve the native element at the observed point",
                error,
            )
        })?;
        let target = super::target::matching_observed_ancestor(
            &uia,
            hit,
            &element.role,
            &element.name,
            &element.automation_id,
            &element.runtime_id,
        )
        .ok_or_else(|| {
            stale_error("native target identity changed before activation; observe again")
        })?;
        let patterns = AvailablePatterns::query(&target)?;
        let plan = choose_activation(patterns.capabilities()).map_err(|reason| match reason {
            Unsupported::SelectionOnly => ActivationError::new(
                FailureKind::Unsupported,
                "the native target exposes selection but no default activation action; no input was sent",
            ),
            Unsupported::NoDefaultAction => ActivationError::new(
                FailureKind::Unsupported,
                "the native target exposes no supported default activation action; no input was sent",
            ),
        })?;
        let method = plan_method(plan);
        validate_dispatch_edge(&uia, &target, x, y, expected_window)?;

        if dry_run {
            return Ok(ActivationReceipt {
                method,
                dry_run: true,
            });
        }

        let dispatch_target = patterns.for_plan(plan)?;
        dispatch(dispatch_target, dispatch_state)?;
        Ok(ActivationReceipt {
            method,
            dry_run: false,
        })
    }
}

struct AvailablePatterns {
    invoke: Option<IUIAutomationInvokePattern>,
    expansion: Option<(IUIAutomationExpandCollapsePattern, Expansion)>,
    legacy_default: Option<IUIAutomationLegacyIAccessiblePattern>,
    selection_item: bool,
}

impl AvailablePatterns {
    unsafe fn query(element: &IUIAutomationElement) -> Result<Self, ActivationError> {
        unsafe {
            let invoke = optional_pattern(
                element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId),
                "acquire Invoke pattern",
            )?;
            let expansion = optional_pattern(
                element.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(
                    UIA_ExpandCollapsePatternId,
                ),
                "acquire ExpandCollapse pattern",
            )?
            .map(|pattern| {
                let state = pattern
                    .CurrentExpandCollapseState()
                    .map(expansion_state)
                    .map_err(|error| capability_error("read ExpandCollapse state", error))?;
                Ok((pattern, state))
            })
            .transpose()?;
            let legacy_default = optional_pattern(
                element.GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(
                    UIA_LegacyIAccessiblePatternId,
                ),
                "acquire LegacyIAccessible pattern",
            )?
            .map(|pattern| {
                let action = pattern
                    .CurrentDefaultAction()
                    .map_err(|error| capability_error("read legacy default action", error))?;
                Ok((!action.to_string().trim().is_empty()).then_some(pattern))
            })
            .transpose()?
            .flatten();
            let selection_item = optional_pattern(
                element.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                    UIA_SelectionItemPatternId,
                ),
                "acquire SelectionItem pattern",
            )?
            .is_some();
            Ok(Self {
                invoke,
                expansion,
                legacy_default,
                selection_item,
            })
        }
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            invoke: self.invoke.is_some(),
            expansion: self.expansion.as_ref().map(|(_, state)| *state),
            legacy_default: self.legacy_default.is_some(),
            selection_item: self.selection_item,
        }
    }

    fn for_plan(self, plan: Plan) -> Result<DispatchTarget, ActivationError> {
        let changed = || {
            capability_error(
                "retain the advertised default-action pattern",
                "the provider withdrew the pattern before dispatch",
            )
        };
        match plan {
            Plan::Invoke => self.invoke.map(DispatchTarget::Invoke).ok_or_else(changed),
            Plan::Expand => self
                .expansion
                .map(|(pattern, _)| DispatchTarget::Expand(pattern))
                .ok_or_else(changed),
            Plan::Collapse => self
                .expansion
                .map(|(pattern, _)| DispatchTarget::Collapse(pattern))
                .ok_or_else(changed),
            Plan::LegacyDefault => self
                .legacy_default
                .map(DispatchTarget::LegacyDefault)
                .ok_or_else(changed),
        }
    }
}

fn optional_pattern<T>(
    result: windows::core::Result<T>,
    stage: &str,
) -> Result<Option<T>, ActivationError> {
    match result {
        Ok(pattern) => Ok(Some(pattern)),
        Err(error) if pattern_is_unavailable(&error) => Ok(None),
        Err(error) if error.code().0 as u32 == UIA_E_ELEMENTNOTAVAILABLE => Err(stale_error(
            "the native element became unavailable while resolving its default action",
        )),
        Err(error) => Err(capability_error(stage, error)),
    }
}

fn pattern_is_unavailable(error: &windows::core::Error) -> bool {
    error.code() == E_NOINTERFACE
        || error.code() == E_POINTER
        || error.code().0 as u32 == UIA_E_NOTSUPPORTED
}

fn expansion_state(state: ExpandCollapseState) -> Expansion {
    if state == ExpandCollapseState_Collapsed {
        Expansion::Collapsed
    } else if state == ExpandCollapseState_Expanded
        || state == ExpandCollapseState_PartiallyExpanded
    {
        Expansion::Expanded
    } else {
        Expansion::Other
    }
}

enum DispatchTarget {
    Invoke(IUIAutomationInvokePattern),
    Expand(IUIAutomationExpandCollapsePattern),
    Collapse(IUIAutomationExpandCollapsePattern),
    LegacyDefault(IUIAutomationLegacyIAccessiblePattern),
}

unsafe fn dispatch(
    target: DispatchTarget,
    dispatch_state: &AtomicU8,
) -> Result<(), ActivationError> {
    unsafe {
        // This claim is adjacent to the provider call. If timeout expired the
        // preflight first, the call is never made; once claimed, a timeout must
        // conservatively report that the provider may have accepted the action.
        claim_dispatch(dispatch_state)?;
        let result = match target {
            DispatchTarget::Invoke(pattern) => pattern.Invoke(),
            DispatchTarget::Expand(pattern) => pattern.Expand(),
            DispatchTarget::Collapse(pattern) => pattern.Collapse(),
            DispatchTarget::LegacyDefault(pattern) => pattern.DoDefaultAction(),
        };
        result.map_err(|error| {
            ActivationError::after_dispatch(
                FailureKind::Dispatch,
                format!("native default-action dispatch failed: {error}"),
            )
        })
    }
}

fn claim_dispatch(dispatch_state: &AtomicU8) -> Result<(), ActivationError> {
    match dispatch_state.compare_exchange(
        DISPATCH_PENDING,
        DISPATCH_STARTED,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => Ok(()),
        Err(DISPATCH_CANCELLED) => Err(ActivationError::cancelled(false)),
        Err(_) => Err(ActivationError::timeout(false)),
    }
}

fn plan_method(plan: Plan) -> &'static str {
    match plan {
        Plan::Invoke => "uia_invoke",
        Plan::Expand => "uia_expand",
        Plan::Collapse => "uia_collapse",
        Plan::LegacyDefault => "uia_legacy_default",
    }
}

fn validate_dispatch_edge(
    uia: &IUIAutomation,
    target: &IUIAutomationElement,
    x: i32,
    y: i32,
    expected: ExpectedNativeWindow,
) -> Result<(), ActivationError> {
    super::validate_native_provider_ownership().map_err(|error| {
        stale_error(format!(
            "native provider ownership changed before activation: {error}"
        ))
    })?;
    super::validate_native_identity(expected.hwnd, expected.pid, expected.generation)
        .map_err(|error| stale_error(format!("native window generation changed: {error}")))?;
    unsafe {
        let foreground_root = root_hwnd(GetForegroundWindow());
        let point_root = root_hwnd(WindowFromPoint(POINT { x, y }));
        validate_roots(expected.hwnd, foreground_root, point_root)?;
        if !super::target::point_resolves_to_element(uia, target, x, y) {
            return Err(stale_error(
                "the native element at the observed point changed before activation",
            ));
        }
        Ok(())
    }
}

unsafe fn root_hwnd(hwnd: HWND) -> u64 {
    if hwnd.0.is_null() {
        return 0;
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    root.0 as usize as u64
}

fn validate_roots(
    expected_hwnd: u64,
    foreground_root: u64,
    point_root: u64,
) -> Result<(), ActivationError> {
    if foreground_root != expected_hwnd {
        return Err(stale_error(format!(
            "foreground root changed before native activation; expected {expected_hwnd}, got {foreground_root}"
        )));
    }
    if point_root != expected_hwnd {
        return Err(stale_error(format!(
            "the observed point now belongs to another window; expected root {expected_hwnd}, got {point_root}"
        )));
    }
    Ok(())
}

fn stale_error(message: impl Into<String>) -> ActivationError {
    ActivationError::new(FailureKind::StaleTarget, message)
}

fn capability_error(stage: &str, error: impl fmt::Display) -> ActivationError {
    stage_error(FailureKind::CapabilityQuery, stage, error)
}

fn stage_error(kind: FailureKind, stage: &str, error: impl fmt::Display) -> ActivationError {
    ActivationError::new(
        kind,
        format!("native activation could not {stage}: {error}"),
    )
}

#[cfg(test)]
#[path = "activation/tests.rs"]
mod tests;
