//! Authoritative and optional UIA enumeration paths.

use super::*;

const AUTHORITATIVE_TIMEOUT_SECS: u64 = 6;
const OPTIONAL_TIMEOUT_SECS: u64 = 1;

pub(in crate::overlay::computer_control) fn enumerate(
    target: Option<&str>,
) -> Result<Vec<UiElement>> {
    enumerate_with(target, EnumerationKind::Authoritative)
}

pub(in crate::overlay::computer_control) fn enumerate_best_effort(
    target: Option<&str>,
) -> Result<Vec<UiElement>> {
    enumerate_with(target, EnumerationKind::Optional)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnumerationKind {
    Authoritative,
    Optional,
}

fn enumerate_with(target: Option<&str>, kind: EnumerationKind) -> Result<Vec<UiElement>> {
    let authoritative_key = circuit::surface_key(target);
    let circuit_key = circuit_key(&authoritative_key, kind);
    if let Some(retry_after) = circuit::remaining(&circuit_key) {
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
    let owned = target.map(str::to_string);
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
        move || enumerate_inner(owned.as_deref()),
    );
    if result.is_ok() {
        circuit::record_success(&authoritative_key);
        circuit::record_success(&optional_key(&authoritative_key));
    } else if result.as_ref().is_err_and(circuit::is_timeout) {
        circuit::record_timeout(&circuit_key);
    }
    if let Err(error) = &result {
        report_unavailable(kind, &circuit_key, None, &error.to_string());
    }
    result
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

fn enumerate_inner(target: Option<&str>) -> Result<Vec<UiElement>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let (root, selected_hwnd) = pick_window(&uia, target)?;
        ensure_native_accessibility_authority(selected_hwnd)?;
        let point_hit_test_is_authoritative =
            target::point_hit_test_is_authoritative(selected_hwnd);
        let cond = uia.CreateTrueCondition()?;
        let arr = root.FindAll(TreeScope_Descendants, &cond)?;
        let n = arr.Length()?;
        let mut out = Vec::new();
        for i in 0..n {
            let Ok(el) = arr.GetElement(i) else { continue };
            let rect = el.CurrentBoundingRectangle().unwrap_or_default();
            if rect.right <= rect.left || rect.bottom <= rect.top {
                continue;
            }
            if el.CurrentIsOffscreen().map(|b| b.as_bool()).unwrap_or(true) {
                continue;
            }
            let name = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
            let ct = el.CurrentControlType().map(|c| c.0).unwrap_or(0);
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
                    el.CurrentAutomationId()
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    target::runtime_id(&el).unwrap_or_default(),
                )
            } else {
                (String::new(), Vec::new())
            };
            if actionable && runtime_id.is_empty() {
                continue;
            }
            let enabled = el.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(false);
            let state = (!name.trim().is_empty()).then(|| read_state(&el)).flatten();
            let value = matches!(ct, 50004 | 50030 | 50003)
                .then(|| read_value(&el))
                .flatten();
            let required = el
                .CurrentIsRequiredForForm()
                .map(|b| b.as_bool())
                .unwrap_or(false);
            out.push(UiElement {
                name,
                automation_id,
                runtime_id,
                control_type: target::control_type_name(ct),
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
