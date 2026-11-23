//! Authoritative and optional UIA enumeration paths.

use super::*;

const AUTHORITATIVE_TIMEOUT_SECS: u64 = 6;
const OPTIONAL_TIMEOUT_SECS: u64 = 1;

pub(in crate::overlay::computer_control) fn enumerate(
    target: Option<&str>,
) -> Result<Vec<UiElement>> {
    enumerate_with(target, EnumerationKind::Authoritative, Detail::Values)
}

pub(in crate::overlay::computer_control) fn enumerate_best_effort(
    target: Option<&str>,
) -> Result<Vec<UiElement>> {
    enumerate_with(target, EnumerationKind::Optional, Detail::Values)
}

/// Grid selection uses control labels and geometry, never text contents.
pub(in crate::overlay::computer_control) fn enumerate_for_snapshot(
    target: Option<&str>,
) -> Result<Vec<UiElement>> {
    enumerate_with(target, EnumerationKind::Optional, Detail::Controls)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Detail {
    Controls,
    Values,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnumerationKind {
    Authoritative,
    Optional,
}

fn enumerate_with(
    target: Option<&str>,
    kind: EnumerationKind,
    detail: Detail,
) -> Result<Vec<UiElement>> {
    let surface = match circuit::resolve_surface(target) {
        Ok(surface) => surface,
        Err(error) => {
            report_unavailable(kind, "unresolved", None, &error.to_string());
            return Err(error);
        }
    };
    let circuit_key = circuit_key(&surface.key(), kind);
    let (permit, lease) = match circuit::admit(&surface, kind.circuit_kind()) {
        Ok(admitted) => admitted,
        Err(circuit::AdmissionError::CoolingDown(retry_after)) => {
            report_unavailable(
                kind,
                &circuit_key,
                Some(retry_after.as_millis()),
                "enumeration is cooling down after a timeout",
            );
            return Err(anyhow!(
                "UIA enumeration cooling down for {} ms after a timeout",
                retry_after.as_millis()
            ));
        }
        Err(circuit::AdmissionError::Capacity) => {
            report_unavailable(
                kind,
                &circuit_key,
                None,
                "enumeration circuit capacity reached",
            );
            return Err(anyhow!("UIA enumeration circuit capacity reached"));
        }
        Err(circuit::AdmissionError::InFlight) => {
            report_unavailable(kind, &circuit_key, None, "enumeration is already in flight");
            return Err(anyhow!("UIA enumeration is already in flight"));
        }
    };
    let hwnd = surface.hwnd;
    let timeout_secs = match kind {
        EnumerationKind::Authoritative => AUTHORITATIVE_TIMEOUT_SECS,
        EnumerationKind::Optional => OPTIONAL_TIMEOUT_SECS,
    };
    let label = match kind {
        EnumerationKind::Authoritative => "enumerate",
        EnumerationKind::Optional => "enumerate_optional",
    };
    let result = with_timeout(
        label,
        timeout_secs,
        Err(circuit::timeout_error()),
        move || {
            // Held until this worker exits, even after its caller times out.
            let _lease = lease;
            enumerate_inner(hwnd, detail)
        },
    );
    if result.is_ok() {
        permit.success();
    } else if result.as_ref().is_err_and(circuit::is_timeout) {
        permit.timeout();
    }
    if let Err(error) = &result {
        report_unavailable(kind, &circuit_key, None, &error.to_string());
    }
    result
}

impl EnumerationKind {
    fn circuit_kind(self) -> circuit::Kind {
        match self {
            Self::Authoritative => circuit::Kind::Authoritative,
            Self::Optional => circuit::Kind::Optional,
        }
    }
}

fn circuit_key(authoritative_key: &str, kind: EnumerationKind) -> String {
    match kind {
        EnumerationKind::Authoritative => authoritative_key.to_string(),
        EnumerationKind::Optional => optional_key(authoritative_key),
    }
}

fn optional_key(authoritative_key: &str) -> String {
    format!("{authoritative_key}|optional")
}

fn report_unavailable(
    kind: EnumerationKind,
    surface: &str,
    retry_after_ms: Option<u128>,
    error: &str,
) {
    let details = serde_json::json!({
        "surface": surface,
        "retry_after_ms": retry_after_ms,
        "error": error,
    });
    match kind {
        EnumerationKind::Authoritative => super::super::telemetry::typed_error(
            "ERR_UIA_ENUMERATION_FAILED",
            "grounding",
            "accessible control enumeration was unavailable",
            details,
        ),
        EnumerationKind::Optional => super::super::telemetry::event(
            "uia_enumeration_degraded",
            "grounding",
            super::super::telemetry::Privacy::Safe,
            details,
        ),
    }
}

pub(super) fn enumerate_inner(hwnd: usize, detail: Detail) -> Result<Vec<UiElement>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let selected_hwnd = HWND(hwnd as *mut _);
        let root = uia.ElementFromHandle(selected_hwnd)?;
        ensure_native_accessibility_authority(selected_hwnd)?;
        let point_hit_test_is_authoritative =
            target::point_hit_test_is_authoritative(selected_hwnd);
        let with_values = detail == Detail::Values;
        let arr = cached_element::find_visible_descendants(&uia, &root, with_values)?;
        let n = arr.Length()?;
        let mut out = Vec::new();
        for i in 0..n {
            let Ok(el) = arr.GetElement(i) else { continue };
            let Some(node) = cached_element::read(&el, with_values) else {
                continue;
            };
            let rect = node.rect;
            if rect.right <= rect.left || rect.bottom <= rect.top {
                continue;
            }
            let ct = node.control_type;
            let actionable = target::is_interactive_control_type(ct);
            if point_hit_test_is_authoritative
                && target::is_grounding_control_type(ct)
                && !target::point_resolves_to_element(
                    &uia,
                    &el,
                    (rect.left + rect.right) / 2,
                    (rect.top + rect.bottom) / 2,
                )
            {
                continue;
            }
            let (automation_id, runtime_id) = if actionable {
                (
                    node.automation_id,
                    target::runtime_id(&el).unwrap_or_default(),
                )
            } else {
                (String::new(), Vec::new())
            };
            if actionable && runtime_id.is_empty() {
                continue;
            }
            let state = (with_values && !node.name.trim().is_empty())
                .then_some(node.state)
                .flatten();
            let value = if with_values && control_types::has_text_value(ct) {
                value_preview::read(
                    &el,
                    node.password == Some(false),
                    value_preview::Capability {
                        value_supported: node.value_supported,
                        value_writable: node.value_writable,
                    },
                )
            } else {
                value_preview::ValuePreview::default()
            };
            out.push(UiElement {
                name: node.name,
                automation_id,
                runtime_id,
                control_type: target::control_type_name(ct),
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
                enabled: node.enabled,
                protected: node.password == Some(true),
                state,
                value,
                required: node.required,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_timeout_cannot_open_the_authoritative_circuit() {
        let base = "opaque-surface";
        assert_eq!(circuit_key(base, EnumerationKind::Authoritative), base);
        assert_ne!(circuit_key(base, EnumerationKind::Optional), base);
    }
}
