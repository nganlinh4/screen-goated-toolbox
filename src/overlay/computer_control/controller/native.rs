//! The native (desktop UIA) `Surface`: perception, execution, and read-back over
//! the foreground window, through `uia::enumerate` and the humanized executor.
//!
//! Perception pairs each unlabeled field with its nearest visible Text label and
//! lists ONLY interactive elements — so the model targets a field by @id and can
//! never click the stray Text label (the native "Name" bug). Fills are read back
//! through UIA ValuePattern, so a fill that didn't land is caught in code.
//!
//! Native exposes no language-neutral form/submit/destructive structure the way
//! HTML does, so those gates stay browser-strong; here the wins are paired-label
//! @id targeting + fill verification. Destructive caution on the desktop stays in
//! the prompt JUDGMENT/SAFETY block (it is inherently language-bound).

use anyhow::Result;
use serde_json::{Value, json};

use super::super::executor;
use super::super::uia::{self, UiElement};
use super::verify::ReadBack;
use super::world::{ElHandle, IndexedElement, SurfaceIdentity, WorldState};
use super::{ActCtx, Surface, Verb, validation};

pub struct NativeSurface {
    target: Option<String>,
}

impl NativeSurface {
    pub fn new(target: Option<String>) -> Self {
        NativeSurface { target }
    }

    fn focus_for_input(&mut self, expected: &SurfaceIdentity) -> Result<()> {
        if let Some(target) = self.target.as_deref()
            && !uia::raise_window(target)?
        {
            anyhow::bail!("the pinned native target could not become foreground");
        }
        if self.identity()? != *expected
            || identity_from_snapshot(&uia::input_target_snapshot())? != *expected
        {
            anyhow::bail!("the pinned native target is not the foreground input surface");
        }
        Ok(())
    }
}

impl Surface for NativeSurface {
    fn prepare_input(
        &mut self,
        verb: Verb,
        act: &ActCtx,
        expected: &SurfaceIdentity,
    ) -> Result<()> {
        if activation_may_focus(verb, act.dry) {
            self.focus_for_input(expected)?;
        }
        Ok(())
    }

    fn identity(&mut self) -> Result<SurfaceIdentity> {
        let (hwnd, pid, generation) = uia::current_native_identity(self.target.as_deref())?;
        Ok(SurfaceIdentity::Native {
            hwnd,
            pid,
            generation,
        })
    }

    fn observe(&mut self) -> Result<WorldState> {
        uia::validate_native_provider_ownership()?;
        let (hwnd, pid, generation) = uia::observe_native_identity(self.target.as_deref())?;
        let before = SurfaceIdentity::Native {
            hwnd,
            pid,
            generation,
        };
        let elements = uia::enumerate(self.target.as_deref())?;
        uia::validate_native_provider_ownership()?;
        let after = self.identity()?;
        if before != after {
            anyhow::bail!("foreground native surface changed while observing controls");
        }
        Ok(build_world(&elements, after))
    }

    fn validate(&self, el: &IndexedElement, verb: Verb, value: Option<&str>) -> Result<(), String> {
        validation::validate_action(el, verb, value)?;
        if verb == Verb::Select {
            return Err(
                "native select is unavailable without an atomic selection primitive".to_string(),
            );
        }
        Ok(())
    }

    fn execute(
        &mut self,
        el: &IndexedElement,
        verb: Verb,
        value: Option<&str>,
        act: &ActCtx,
        expected: &SurfaceIdentity,
    ) -> Result<Value> {
        let ElHandle::Native {
            cx,
            cy,
            ref provider_name,
            ref automation_id,
            ref runtime_id,
        } = el.handle
        else {
            anyhow::bail!("not a native element");
        };
        let target = uia::ExpectedNativeElement {
            role: &el.role,
            provider_name,
            automation_id,
            runtime_id,
        };
        // A dry activation is a capability probe only. In particular it must not
        // foreground a pinned window. prepare_input raised the pinned target
        // before the pre-action snapshot; re-checking here keeps the
        // dispatch-edge guard, and only raises again if focus was taken since.
        if activation_may_focus(verb, act.dry) {
            self.focus_for_input(expected)?;
        }
        match verb {
            Verb::Click | Verb::Submit | Verb::Toggle => {
                click_native(cx, cy, target, act, expected)
            }
            Verb::Activate => activate_native(cx, cy, target, act, expected),
            Verb::Fill => {
                let focus = click_native(cx, cy, target, act, expected)?; // focus the field
                if !act.dry {
                    ensure_identity(self, expected)?;
                    // Select existing content so the paste replaces it, then type.
                    let select_all = checked_execution(executor::execute_ex(
                        "key_combination",
                        &guarded_args(
                            json!({"keys": "Control+A"}),
                            expected,
                            Some((cx, cy, target)),
                        ),
                        act.profile,
                        act.cancel,
                    ))
                    .map_err(after_prior_native_input)?;
                    ensure_identity(self, expected)?;
                    let typed = checked_execution(executor::execute_ex(
                        "type_text",
                        &guarded_args(
                            json!({"text": value.unwrap_or("")}),
                            expected,
                            Some((cx, cy, target)),
                        ),
                        act.profile,
                        act.cancel,
                    ))
                    .map_err(after_prior_native_input)?;
                    return Ok(
                        json!({"ok": true, "focus": focus, "select_all": select_all, "typed": typed}),
                    );
                }
                Ok(json!({"ok": true, "dry_run": true, "focus": focus}))
            }
            Verb::Select => {
                anyhow::bail!(
                    "native select is unavailable without an atomic selection primitive; no input was sent"
                )
            }
        }
    }

    fn read_back(&mut self, el: &IndexedElement, expected: &SurfaceIdentity) -> ReadBack {
        let ElHandle::Native {
            cx,
            cy,
            ref provider_name,
            ref automation_id,
            ref runtime_id,
        } = el.handle
        else {
            return ReadBack::default();
        };
        if uia::validate_native_provider_ownership().is_err()
            || !self.identity().is_ok_and(|id| id == *expected)
            || !identity_from_snapshot(&uia::input_target_snapshot())
                .is_ok_and(|id| id == *expected)
        {
            return ReadBack::default();
        }
        let result = uia::read_value_at(
            cx,
            cy,
            uia::ExpectedNativeElement {
                role: &el.role,
                provider_name,
                automation_id,
                runtime_id,
            },
            el.protected,
        );
        if uia::validate_native_provider_ownership().is_err()
            || !self.identity().is_ok_and(|id| id == *expected)
            || !identity_from_snapshot(&uia::input_target_snapshot())
                .is_ok_and(|id| id == *expected)
        {
            return ReadBack::default();
        }
        let (value, protected) = match result {
            Some(uia::ReadValue::Value(value)) => (Some(value), false),
            Some(uia::ReadValue::Protected) => (None, true),
            None => (None, false),
        };
        ReadBack {
            value,
            protected,
            selected_text: None,
            validity: None,
        }
    }
}

fn activation_may_focus(verb: Verb, dry_run: bool) -> bool {
    verb != Verb::Activate || !dry_run
}

/// Humanized left-click at a screen pixel (the executor maps screen→0-1000 and
/// drives `SendInput`). `uncertain:false` — UIA gives the exact center, no hesitation.
fn click_native(
    cx: i32,
    cy: i32,
    target: uia::ExpectedNativeElement<'_>,
    act: &ActCtx,
    expected: &SurfaceIdentity,
) -> Result<Value> {
    pointer_native(cx, cy, target, "click", act, expected)
}

/// Perform the control's structurally exposed UIA default action. Selection-only
/// controls are deliberately rejected rather than guessed into a double-click.
fn activate_native(
    cx: i32,
    cy: i32,
    target: uia::ExpectedNativeElement<'_>,
    act: &ActCtx,
    expected: &SurfaceIdentity,
) -> Result<Value> {
    let SurfaceIdentity::Native {
        hwnd,
        pid,
        generation,
    } = expected
    else {
        anyhow::bail!("native activation requires an observed native window identity");
    };
    match uia::activate_at(
        (cx, cy),
        target,
        (*hwnd, *pid, *generation),
        act.dry,
        act.cancel,
    ) {
        Ok(receipt) => Ok(json!({
            "ok": true,
            "method": receipt.method(),
            "dry_run": receipt.dry_run(),
            "screen_px": [cx, cy],
        })),
        Err(error) => Err(native_activation_failure(error).into()),
    }
}

#[derive(Debug)]
struct NativeActionFailure(Value);

impl std::fmt::Display for NativeActionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            self.0
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("native action failed"),
        )
    }
}

impl std::error::Error for NativeActionFailure {}

fn native_activation_failure(error: uia::ActivationError) -> NativeActionFailure {
    let code = match error.kind() {
        uia::FailureKind::Unsupported => "ERR_NATIVE_ACTIVATION_UNSUPPORTED",
        uia::FailureKind::StaleTarget => "ERR_NATIVE_STALE_TARGET",
        uia::FailureKind::Setup => "ERR_NATIVE_ACTIVATION_SETUP",
        uia::FailureKind::TargetQuery => "ERR_NATIVE_ACTIVATION_TARGET_QUERY",
        uia::FailureKind::CapabilityQuery => "ERR_NATIVE_ACTIVATION_CAPABILITY_QUERY",
        uia::FailureKind::Dispatch => "ERR_NATIVE_ACTIVATION_FAILED",
        uia::FailureKind::Cancelled => "ERR_NATIVE_ACTIVATION_CANCELLED",
        uia::FailureKind::Timeout => "ERR_NATIVE_ACTIVATION_TIMEOUT",
    };
    NativeActionFailure(json!({
        "ok": false,
        "code": code,
        "dispatch_ok": false,
        "effect_may_have_occurred": error.effect_may_have_occurred(),
        "error": error.to_string(),
    }))
}

pub(super) fn typed_value(error: &anyhow::Error) -> Option<Value> {
    error
        .downcast_ref::<NativeActionFailure>()
        .map(|failure| failure.0.clone())
}

fn pointer_native(
    cx: i32,
    cy: i32,
    target: uia::ExpectedNativeElement<'_>,
    tool: &str,
    act: &ActCtx,
    expected: &SurfaceIdentity,
) -> Result<Value> {
    if act.dry {
        return Ok(json!({"ok": true, "dry_run": true, "tool": tool, "screen_px": [cx, cy]}));
    }
    let (vx, vy, vw, vh) = uia::virtual_desktop();
    let nx = (cx - vx) as f64 / vw.max(1) as f64 * 1000.0;
    let ny = (cy - vy) as f64 / vh.max(1) as f64 * 1000.0;
    checked_execution(executor::execute_ex(
        tool,
        &guarded_args(
            json!({"x": nx, "y": ny, "button": "left", "uncertain": false}),
            expected,
            Some((cx, cy, target)),
        ),
        act.profile,
        act.cancel,
    ))
}

fn guarded_args(
    mut args: Value,
    expected: &SurfaceIdentity,
    element: Option<(i32, i32, uia::ExpectedNativeElement<'_>)>,
) -> Value {
    let SurfaceIdentity::Native {
        hwnd,
        pid,
        generation,
    } = expected
    else {
        return args;
    };
    args["expected_input_target"] = json!({"hwnd": hwnd, "pid": pid, "generation": generation});
    if let Some((cx, cy, element)) = element {
        args["expected_input_target"]["element"] = json!({
            "screen_px": [cx, cy],
            "role": element.role,
            "provider_name": element.provider_name,
            "automation_id": element.automation_id,
            "runtime_id": element.runtime_id,
        });
    }
    args
}

fn ensure_identity(surface: &mut NativeSurface, expected: &SurfaceIdentity) -> Result<()> {
    surface.focus_for_input(expected)
}

fn identity_from_snapshot(snapshot: &Value) -> Result<SurfaceIdentity> {
    let available = snapshot.get("available").and_then(Value::as_bool) == Some(true);
    let hwnd = snapshot.get("hwnd").and_then(Value::as_u64).unwrap_or(0);
    let pid = snapshot.get("pid").and_then(Value::as_u64).unwrap_or(0);
    let generation = snapshot
        .get("generation")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if !available || hwnd == 0 || pid == 0 || generation == 0 {
        anyhow::bail!("concrete foreground window generation unavailable");
    }
    Ok(SurfaceIdentity::Native {
        hwnd,
        pid,
        generation,
    })
}

fn checked_execution(value: Value) -> Result<Value> {
    if value.get("ok").and_then(Value::as_bool) == Some(false)
        || value.get("cancelled").and_then(Value::as_bool) == Some(true)
    {
        let mut value = value;
        value["dispatch_ok"] = json!(false);
        if value.get("effect_may_have_occurred").is_none() {
            value["effect_may_have_occurred"] = json!(true);
        }
        return Err(NativeActionFailure(value).into());
    }
    Ok(value)
}

fn after_prior_native_input(error: anyhow::Error) -> anyhow::Error {
    if let Some(mut receipt) = typed_value(&error) {
        // Focusing and selecting text already sent input even if the next step
        // failed before dispatch. Do not report the entire fill as no-effect.
        receipt["prior_input_dispatched"] = json!(true);
        receipt["effect_may_have_occurred"] = json!(true);
        receipt["retryable"] = json!(false);
        NativeActionFailure(receipt).into()
    } else {
        error
    }
}

/// The label text nearest to (and outside) `e` — preferring a Text just to its
/// LEFT on the same row, else one just ABOVE in roughly the same column. This is
/// the native label-pairing that lets the model target the FIELD, not the label.
fn nearest_label(e: &UiElement, labels: &[&UiElement]) -> Option<String> {
    let ey = (e.top + e.bottom) / 2;
    let row_tol = (e.bottom - e.top).max(10);
    let mut best: Option<(i64, String)> = None;
    for l in labels {
        let ly = (l.top + l.bottom) / 2;
        let left_of =
            l.right <= e.left + 5 && (e.left - l.right) < 400 && (ly - ey).abs() <= row_tol;
        let above =
            l.bottom <= e.top + 5 && (e.top - l.bottom) < 80 && (l.left - e.left).abs() < 200;
        let d = if left_of {
            (e.left - l.right) as i64 // closest-left wins
        } else if above {
            1000 + (e.top - l.bottom) as i64 // left-of preferred over above
        } else {
            continue;
        };
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, l.name.trim().to_string()));
        }
    }
    best.map(|(_, n)| n)
}

/// Build the indexed world from a UIA enumeration: keep interactive elements,
/// pair unlabeled ones with a nearby Text label, drop anonymous noise.
pub(super) fn build_world(els: &[UiElement], identity: SurfaceIdentity) -> WorldState {
    let labels: Vec<&UiElement> = els
        .iter()
        .filter(|e| e.control_type == "Text" && !e.name.trim().is_empty())
        .collect();
    let mut out = Vec::new();
    let mut id = 0u32;
    for e in els {
        let Some(role) = uia::control_types::role_for_name(e.control_type) else {
            continue;
        };
        let mut name = e.name.trim().to_string();
        if name.is_empty() {
            name = nearest_label(e, &labels).unwrap_or_default();
        }
        // Drop anonymous noise: no name, no value, no state, and not a text field.
        if name.is_empty()
            && e.value.text.is_none()
            && !e.value.supported
            && e.state.is_none()
            && role != "textbox"
        {
            continue;
        }
        id += 1;
        let (cx, cy) = e.center();
        out.push(IndexedElement {
            id,
            role: role.to_string(),
            name,
            value: None,
            value_preview: e.value.text.clone(),
            editable: (role == "textbox" && !e.value.supported)
                || (matches!(role, "textbox" | "combobox") && e.value.writable),
            protected: e.protected,
            state: Some(format!(
                "{}{}",
                e.state.as_deref().unwrap_or_default(),
                if e.value.incomplete {
                    " value·unobserved-or-truncated"
                } else {
                    ""
                }
            ))
            .filter(|state| !state.is_empty()),
            enabled: e.enabled,
            required: e.required,
            submit: false, // no language-neutral submit signal on the desktop
            form: None,    // native has no form grouping
            risk: None,    // no clean structural signal on the desktop — prompt handles it
            handle: ElHandle::Native {
                cx,
                cy,
                provider_name: e.name.clone(),
                automation_id: e.automation_id.clone(),
                runtime_id: e.runtime_id.clone(),
            },
        });
        if out.len() >= 200 {
            break;
        }
    }
    WorldState {
        elements: out,
        url: None,
        title: None,
        identity,
    }
}

#[cfg(test)]
mod tests;
