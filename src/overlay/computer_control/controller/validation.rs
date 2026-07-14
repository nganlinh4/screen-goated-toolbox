use super::Verb;
use super::world::IndexedElement;

/// Validate verb/value compatibility without considering mutable state. This is
/// safe to run as a whole-plan preflight before any batched effect occurs.
pub(super) fn validate_shape(
    element: &IndexedElement,
    verb: Verb,
    value: Option<&str>,
) -> Result<(), String> {
    let role = element.role.as_str();
    match verb {
        Verb::Fill if !matches!(role, "textbox" | "searchbox" | "spinbutton") => {
            return Err(format!("fill is incompatible with role {role:?}"));
        }
        Verb::Select if role != "combobox" => {
            return Err(format!("select is incompatible with role {role:?}"));
        }
        Verb::Submit if !element.submit => {
            return Err("submit requires a structurally identified submit control".to_string());
        }
        Verb::Toggle if !matches!(role, "checkbox" | "radio" | "switch" | "menuitemcheckbox") => {
            return Err(format!("toggle is incompatible with role {role:?}"));
        }
        _ => {}
    }
    if matches!(verb, Verb::Fill | Verb::Select) && value.is_none() {
        return Err(format!("{} requires a value", verb.as_str()));
    }
    if !matches!(verb, Verb::Fill | Verb::Select) && value.is_some() {
        return Err(format!("{} does not accept a value", verb.as_str()));
    }
    Ok(())
}

/// Validate the current element immediately before dispatch. Enabledness is
/// intentionally checked here, not during batch preflight, because earlier
/// steps may legitimately enable a later control.
pub(super) fn validate_action(
    element: &IndexedElement,
    verb: Verb,
    value: Option<&str>,
) -> Result<(), String> {
    validate_shape(element, verb, value)?;
    if !element.enabled {
        return Err("target control is disabled in the current observation".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::computer_control::controller::world::ElHandle;

    fn element(role: &str, enabled: bool) -> IndexedElement {
        IndexedElement {
            id: 1,
            role: role.to_string(),
            name: "control".to_string(),
            value: None,
            state: None,
            enabled,
            required: false,
            submit: false,
            form: None,
            risk: None,
            handle: ElHandle::Native {
                cx: 1,
                cy: 1,
                provider_name: "control".into(),
                automation_id: "control-id".into(),
                runtime_id: vec![1],
            },
        }
    }

    #[test]
    fn rejects_effectful_malformed_calls_before_dispatch() {
        assert!(validate_action(&element("button", true), Verb::Fill, Some("x")).is_err());
        assert!(validate_action(&element("textbox", true), Verb::Fill, None).is_err());
        assert!(validate_action(&element("button", true), Verb::Click, Some("x")).is_err());
        assert!(validate_action(&element("checkbox", false), Verb::Toggle, None).is_err());
    }

    #[test]
    fn accepts_structurally_compatible_calls() {
        assert!(validate_action(&element("textbox", true), Verb::Fill, Some("")).is_ok());
        assert!(validate_action(&element("checkbox", true), Verb::Toggle, None).is_ok());
        assert!(validate_action(&element("listitem", true), Verb::Activate, None).is_ok());
    }
}
