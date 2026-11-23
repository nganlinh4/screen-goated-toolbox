use anyhow::{Result, anyhow};

pub(super) use super::control_types::{
    name_for_id as control_type_name, role_for_id as role_for_control_type,
};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, SAFEARRAY,
};
use windows::Win32::System::Ole::{
    SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
};
use windows::Win32::System::Variant::VT_BOOL;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationValuePattern, UIA_DocumentControlTypeId, UIA_IsReadOnlyAttributeId,
    UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

#[derive(Debug, PartialEq, Eq)]
struct ProviderIdentity {
    role: String,
    name: String,
    automation_id: String,
    runtime_id: Vec<i32>,
}

#[derive(Clone, Copy)]
pub(in crate::overlay::computer_control) struct ExpectedNativeElement<'a> {
    pub role: &'a str,
    pub provider_name: &'a str,
    pub automation_id: &'a str,
    pub runtime_id: &'a [i32],
}

pub(super) fn is_interactive_control_type(control_type: i32) -> bool {
    role_for_control_type(control_type).is_some()
}

/// Interactive handles and Text labels are the only provider nodes that can
/// enter the model-facing native world.
pub(super) fn is_grounding_control_type(control_type: i32) -> bool {
    super::control_types::is_grounding(control_type)
}

/// Global point hit-testing describes only the visible foreground surface. An
/// occluded capture must retain its provider tree until it is raised for input.
pub(super) unsafe fn point_hit_test_is_authoritative(selected_hwnd: HWND) -> bool {
    unsafe { selected_hwnd == GetForegroundWindow() }
}

/// UIA providers can expose controls from hidden/inactive views with a non-empty
/// rectangle and `IsOffscreen=false`. Keep an actionable node only when the OS
/// hit test at its center resolves to that exact node or one of its descendants.
pub(super) unsafe fn point_resolves_to_element(
    uia: &IUIAutomation,
    expected: &IUIAutomationElement,
    x: i32,
    y: i32,
) -> bool {
    unsafe {
        let Ok(hit) = uia.ElementFromPoint(POINT { x, y }) else {
            return false;
        };
        let Ok(walker) = uia.RawViewWalker() else {
            return false;
        };
        let mut candidate = hit;
        for _ in 0..64 {
            if uia
                .CompareElements(expected, &candidate)
                .map(|same| same.as_bool())
                .unwrap_or(false)
            {
                return true;
            }
            let Ok(parent) = walker.GetParentElement(&candidate) else {
                break;
            };
            candidate = parent;
        }
        false
    }
}

/// Re-resolve the native target at the final pointer dispatch edge. Provider
/// fields come from the exact observed node before any model-facing label pairing.
pub(in crate::overlay::computer_control) fn validate_native_element_at(
    x: i32,
    y: i32,
    element: ExpectedNativeElement<'_>,
) -> Result<()> {
    let expected = ProviderIdentity {
        role: element.role.to_string(),
        name: element.provider_name.to_string(),
        automation_id: element.automation_id.to_string(),
        runtime_id: element.runtime_id.to_vec(),
    };
    super::with_timeout(
        "validate_native_element_at",
        3,
        Err(anyhow!("native target hit-test timed out")),
        move || validate_native_element_at_inner(x, y, &expected),
    )
}

/// Re-resolve keyboard focus at every key dispatch edge. A click can succeed
/// and then lose focus before Ctrl+A, text, or key-up; window identity alone
/// cannot keep those inputs on the originally observed field.
pub(in crate::overlay::computer_control) fn validate_native_focused_element(
    element: ExpectedNativeElement<'_>,
) -> Result<()> {
    let expected = ProviderIdentity {
        role: element.role.to_string(),
        name: element.provider_name.to_string(),
        automation_id: element.automation_id.to_string(),
        runtime_id: element.runtime_id.to_vec(),
    };
    super::with_timeout(
        "validate_native_focused_element",
        3,
        Err(anyhow!("native focused-element validation timed out")),
        move || validate_native_focused_element_inner(&expected),
    )
}

/// Capture the one OS-focused writable editor immediately before an artifact paste.
pub(in crate::overlay::computer_control) fn focused_editable_guard(
    require_document: bool,
) -> Result<serde_json::Value> {
    super::with_timeout(
        "focused_editable_guard",
        3,
        Err(anyhow!("focused editor observation timed out")),
        move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let focused = uia.GetFocusedElement()?;
            ensure_editable(&focused)?;
            if require_document && !within_document(&uia, focused.clone()) {
                return Err(anyhow!(
                    "focused editor is outside the active browser document"
                ));
            }
            let identity = provider_identity(&focused)
                .filter(|identity| !identity.runtime_id.is_empty())
                .ok_or_else(|| anyhow!("focused editor has no stable provider identity"))?;
            Ok(serde_json::json!({
                "screen_px": [0, 0], "role": identity.role,
                "provider_name": identity.name, "automation_id": identity.automation_id,
                "runtime_id": identity.runtime_id, "require_document": require_document,
            }))
        },
    )
}

pub(in crate::overlay::computer_control) fn validate_focused_editable(
    element: ExpectedNativeElement<'_>,
    require_document: bool,
) -> Result<()> {
    let expected = ProviderIdentity {
        role: element.role.to_string(),
        name: element.provider_name.to_string(),
        automation_id: element.automation_id.to_string(),
        runtime_id: element.runtime_id.to_vec(),
    };
    super::with_timeout(
        "validate_focused_editable",
        3,
        Err(anyhow!("focused editor validation timed out")),
        move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let focused = uia.GetFocusedElement()?;
            let actual = provider_identity(&focused)
                .ok_or_else(|| anyhow!("focused editor identity is unavailable"))?;
            if !focused_editor_identity_matches(&expected, &actual) {
                return Err(anyhow!(
                    "focused editor changed before paste; observe again"
                ));
            }
            if require_document && !within_document(&uia, focused.clone()) {
                return Err(anyhow!(
                    "focused editor is outside the active browser document"
                ));
            }
            ensure_editable(&focused)
        },
    )
}

unsafe fn within_document(uia: &IUIAutomation, mut element: IUIAutomationElement) -> bool {
    unsafe {
        let Ok(walker) = uia.RawViewWalker() else {
            return false;
        };
        for _ in 0..64 {
            if element
                .CurrentControlType()
                .is_ok_and(|role| role == UIA_DocumentControlTypeId)
            {
                return true;
            }
            let Ok(parent) = walker.GetParentElement(&element) else {
                return false;
            };
            element = parent;
        }
        false
    }
}

unsafe fn ensure_editable(element: &IUIAutomationElement) -> Result<()> {
    unsafe {
        if !is_enabled_editor(
            role_for_control_type(element.CurrentControlType()?.0),
            element.CurrentIsEnabled()?.as_bool(),
        ) {
            return Err(anyhow!("focused element is not an enabled editor"));
        }
        let read_only = if let Ok(value) =
            element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        {
            value.CurrentIsReadOnly()?.as_bool()
        } else {
            let text = element
                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .map_err(|_| anyhow!("focused editor does not expose writable text state"))?;
            let value = text
                .DocumentRange()?
                .GetAttributeValue(UIA_IsReadOnlyAttributeId)?;
            if value.vt() != VT_BOOL {
                return Err(anyhow!("focused editor writable state is ambiguous"));
            }
            bool::try_from(&value)?
        };
        if read_only {
            return Err(anyhow!("focused editor is read-only"));
        }
        Ok(())
    }
}

fn is_enabled_editor(role: Option<&str>, enabled: bool) -> bool {
    role == Some("textbox") && enabled
}

fn focused_editor_identity_matches(expected: &ProviderIdentity, actual: &ProviderIdentity) -> bool {
    expected.role == actual.role
        && expected.automation_id == actual.automation_id
        && !expected.runtime_id.is_empty()
        && expected.runtime_id == actual.runtime_id
}

pub(in crate::overlay::computer_control) fn validate_native_provider_ownership() -> Result<()> {
    validate_native_provider_state(super::super::browser::input_active())
}

fn validate_native_provider_state(browser_semantic_active: bool) -> Result<()> {
    if browser_semantic_active {
        Err(anyhow!(
            "native source became stale when a precise browser provider took ownership; use a fresh browser frame"
        ))
    } else {
        Ok(())
    }
}

fn validate_native_element_at_inner(x: i32, y: i32, expected: &ProviderIdentity) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let hit = uia.ElementFromPoint(POINT { x, y })?;
        matching_expected_ancestor(&uia, hit, expected).ok_or_else(|| {
            anyhow!("native target identity changed before pointer injection; observe again")
        })?;
        Ok(())
    }
}

fn validate_native_focused_element_inner(expected: &ProviderIdentity) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let focused = uia.GetFocusedElement()?;
        matching_expected_ancestor(&uia, focused, expected).ok_or_else(|| {
            anyhow!("native keyboard focus changed before input injection; observe again")
        })?;
        Ok(())
    }
}

pub(super) unsafe fn matching_observed_ancestor(
    uia: &IUIAutomation,
    hit: IUIAutomationElement,
    role: &str,
    name: &str,
    automation_id: &str,
    expected_runtime_id: &[i32],
) -> Option<IUIAutomationElement> {
    let expected = ProviderIdentity {
        role: role.to_string(),
        name: name.to_string(),
        automation_id: automation_id.to_string(),
        runtime_id: expected_runtime_id.to_vec(),
    };
    unsafe { matching_expected_ancestor(uia, hit, &expected) }
}

unsafe fn matching_expected_ancestor(
    uia: &IUIAutomation,
    hit: IUIAutomationElement,
    expected: &ProviderIdentity,
) -> Option<IUIAutomationElement> {
    unsafe {
        let chain = interactive_ancestor_chain(uia, hit);
        let index = matching_identity_index(expected, chain.iter().map(|(_, identity)| identity))?;
        Some(chain[index].0.clone())
    }
}

unsafe fn interactive_ancestor_chain(
    uia: &IUIAutomation,
    mut element: IUIAutomationElement,
) -> Vec<(IUIAutomationElement, ProviderIdentity)> {
    unsafe {
        let Ok(walker) = uia.RawViewWalker() else {
            return Vec::new();
        };
        let mut chain = Vec::new();
        for _ in 0..64 {
            if let Some(identity) = provider_identity(&element) {
                chain.push((element.clone(), identity));
            }
            let Ok(parent) = walker.GetParentElement(&element) else {
                break;
            };
            element = parent;
        }
        chain
    }
}

unsafe fn provider_identity(element: &IUIAutomationElement) -> Option<ProviderIdentity> {
    unsafe {
        let control_type = element.CurrentControlType().ok()?.0;
        let role = role_for_control_type(control_type)?;
        Some(ProviderIdentity {
            role: role.to_string(),
            name: element.CurrentName().ok()?.to_string(),
            automation_id: element.CurrentAutomationId().ok()?.to_string(),
            runtime_id: runtime_id(element).unwrap_or_default(),
        })
    }
}

fn matching_identity_index<'a>(
    expected: &ProviderIdentity,
    chain: impl IntoIterator<Item = &'a ProviderIdentity>,
) -> Option<usize> {
    chain
        .into_iter()
        .position(|candidate| provider_identity_matches(expected, candidate))
}

fn provider_identity_matches(expected: &ProviderIdentity, actual: &ProviderIdentity) -> bool {
    expected.role == actual.role
        && expected.name == actual.name
        && expected.automation_id == actual.automation_id
        && !expected.runtime_id.is_empty()
        && expected.runtime_id == actual.runtime_id
}

pub(super) unsafe fn runtime_id(element: &IUIAutomationElement) -> Result<Vec<i32>> {
    unsafe {
        let array = OwnedSafeArray(element.GetRuntimeId()?);
        if array.0.is_null() || SafeArrayGetDim(array.0) != 1 {
            return Err(anyhow!(
                "native provider returned an invalid RuntimeId array"
            ));
        }
        let lower = SafeArrayGetLBound(array.0, 1)?;
        let upper = SafeArrayGetUBound(array.0, 1)?;
        let len = upper.saturating_sub(lower).saturating_add(1) as usize;
        if len == 0 || len > 128 {
            return Err(anyhow!(
                "native provider returned an invalid RuntimeId length"
            ));
        }
        let mut values = Vec::with_capacity(len);
        for index in lower..=upper {
            let mut value = 0_i32;
            SafeArrayGetElement(
                array.0,
                &index,
                (&mut value as *mut i32).cast::<core::ffi::c_void>(),
            )?;
            values.push(value);
        }
        Ok(values)
    }
}

struct OwnedSafeArray(*mut SAFEARRAY);

impl Drop for OwnedSafeArray {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = SafeArrayDestroy(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(
        role: &str,
        name: &str,
        automation_id: &str,
        runtime_id: &[i32],
    ) -> ProviderIdentity {
        ProviderIdentity {
            role: role.into(),
            name: name.into(),
            automation_id: automation_id.into(),
            runtime_id: runtime_id.to_vec(),
        }
    }

    #[test]
    fn paste_editor_must_be_an_enabled_textbox() {
        assert!(is_enabled_editor(Some("textbox"), true));
        assert!(!is_enabled_editor(Some("textbox"), false));
        assert!(!is_enabled_editor(Some("button"), true));
        assert!(!is_enabled_editor(None, true));
    }

    #[test]
    fn paste_editor_identity_survives_value_labels_but_not_focus_replacement() {
        let expected = identity("textbox", "before", "editor", &[4, 8]);
        assert!(focused_editor_identity_matches(
            &expected,
            &identity("textbox", "after", "editor", &[4, 8]),
        ));
        assert!(!focused_editor_identity_matches(
            &expected,
            &identity("textbox", "before", "editor", &[4, 9]),
        ));
    }

    #[test]
    fn ancestor_chain_accepts_exact_parent_and_rejects_overlapping_hidden_node() {
        let expected = identity("listitem", "row", "row-7", &[42, 7]);
        let chain = [
            identity("button", "child", "child-1", &[42, 8]),
            identity("listitem", "row", "row-7", &[42, 7]),
        ];
        assert_eq!(matching_identity_index(&expected, chain.iter()), Some(1));

        let unrelated = [identity("listitem", "row", "row-7", &[99, 7])];
        assert_eq!(matching_identity_index(&expected, unrelated.iter()), None);
    }

    #[test]
    fn runtime_id_is_mandatory_and_must_match() {
        let expected = identity("link", "entry", "", &[]);
        assert!(!provider_identity_matches(
            &expected,
            &identity("link", "entry", "", &[1, 2])
        ));
        let expected = identity("link", "entry", "", &[1, 2]);
        assert!(provider_identity_matches(
            &expected,
            &identity("link", "entry", "", &[1, 2])
        ));
        assert!(!provider_identity_matches(
            &expected,
            &identity("link", "entry", "", &[1, 3])
        ));
    }

    #[test]
    fn provider_transition_invalidates_native_source_before_input() {
        assert!(validate_native_provider_state(false).is_ok());
        assert!(validate_native_provider_state(true).is_err());
    }

    #[test]
    fn role_mapping_covers_every_native_controller_role() {
        for (role, control_type) in [
            ("textbox", 50004),
            ("textbox", 50030),
            ("button", 50000),
            ("button", 50031),
            ("link", 50005),
            ("checkbox", 50002),
            ("radio", 50013),
            ("combobox", 50003),
            ("slider", 50015),
            ("tab", 50019),
            ("menuitem", 50011),
            ("listitem", 50007),
            ("treeitem", 50024),
        ] {
            assert_eq!(role_for_control_type(control_type), Some(role));
            assert!(is_interactive_control_type(control_type));
        }
        assert!(is_grounding_control_type(50020));
        assert!(!is_grounding_control_type(50026));
    }
}
