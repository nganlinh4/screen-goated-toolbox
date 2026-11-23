//! One-round-trip UIA snapshots for enumeration.
//!
//! A plain `FindAll` returns live element references, and every `Current*`
//! property or pattern read on them is a separate cross-process call into the
//! provider. Enumeration instead asks the provider for the whole descendant set
//! together with every property it reads, through one `FindAllBuildCache`
//! request, and reads those values only from the cache. Offscreen nodes are
//! filtered on the provider side so they are never marshalled at all.

use anyhow::Result;
use windows::Win32::Foundation::RECT;
use windows::Win32::System::Variant::{VARIANT, VT_BOOL, VT_I4, VT_R8};
use windows::Win32::UI::Accessibility::{
    ExpandCollapseState_Collapsed, ExpandCollapseState_Expanded,
    ExpandCollapseState_PartiallyExpanded, IUIAutomation, IUIAutomationCacheRequest,
    IUIAutomationElement, IUIAutomationElementArray, ToggleState_Indeterminate, ToggleState_On,
    TreeScope_Descendants, UIA_AutomationIdPropertyId, UIA_BoundingRectanglePropertyId,
    UIA_ControlTypePropertyId, UIA_ExpandCollapseExpandCollapseStatePropertyId,
    UIA_IsEnabledPropertyId, UIA_IsExpandCollapsePatternAvailablePropertyId,
    UIA_IsOffscreenPropertyId, UIA_IsPasswordPropertyId,
    UIA_IsRangeValuePatternAvailablePropertyId, UIA_IsRequiredForFormPropertyId,
    UIA_IsSelectionItemPatternAvailablePropertyId, UIA_IsTogglePatternAvailablePropertyId,
    UIA_IsValuePatternAvailablePropertyId, UIA_NamePropertyId, UIA_PROPERTY_ID,
    UIA_RangeValueValuePropertyId, UIA_SelectionItemIsSelectedPropertyId,
    UIA_ToggleToggleStatePropertyId, UIA_ValueIsReadOnlyPropertyId,
};

const CORE_PROPERTIES: [UIA_PROPERTY_ID; 8] = [
    UIA_BoundingRectanglePropertyId,
    UIA_NamePropertyId,
    UIA_ControlTypePropertyId,
    UIA_AutomationIdPropertyId,
    UIA_IsEnabledPropertyId,
    UIA_IsPasswordPropertyId,
    UIA_IsRequiredForFormPropertyId,
    UIA_IsOffscreenPropertyId,
];

/// Pattern availability and pattern state, read only when values are wanted.
const VALUE_PROPERTIES: [UIA_PROPERTY_ID; 10] = [
    UIA_IsTogglePatternAvailablePropertyId,
    UIA_ToggleToggleStatePropertyId,
    UIA_IsSelectionItemPatternAvailablePropertyId,
    UIA_SelectionItemIsSelectedPropertyId,
    UIA_IsExpandCollapsePatternAvailablePropertyId,
    UIA_ExpandCollapseExpandCollapseStatePropertyId,
    UIA_IsRangeValuePatternAvailablePropertyId,
    UIA_RangeValueValuePropertyId,
    UIA_IsValuePatternAvailablePropertyId,
    UIA_ValueIsReadOnlyPropertyId,
];

/// Every property enumeration reads from one provider node.
pub(super) struct CachedNode {
    pub rect: RECT,
    pub name: String,
    pub control_type: i32,
    pub automation_id: String,
    pub enabled: bool,
    pub password: Option<bool>,
    pub required: bool,
    pub state: Option<String>,
    pub value_supported: bool,
    pub value_writable: bool,
}

/// Visible descendants of `root` with `with_values` controlling whether
/// pattern state is fetched in the same round trip.
pub(super) unsafe fn find_visible_descendants(
    uia: &IUIAutomation,
    root: &IUIAutomationElement,
    with_values: bool,
) -> Result<IUIAutomationElementArray> {
    unsafe {
        let request = cache_request(uia, with_values)?;
        let on_screen =
            uia.CreatePropertyCondition(UIA_IsOffscreenPropertyId, &VARIANT::from(false))?;
        Ok(root.FindAllBuildCache(TreeScope_Descendants, &on_screen, &request)?)
    }
}

unsafe fn cache_request(
    uia: &IUIAutomation,
    with_values: bool,
) -> Result<IUIAutomationCacheRequest> {
    unsafe {
        let request = uia.CreateCacheRequest()?;
        for property in CORE_PROPERTIES {
            request.AddProperty(property)?;
        }
        if with_values {
            for property in VALUE_PROPERTIES {
                request.AddProperty(property)?;
            }
        }
        Ok(request)
    }
}

/// Read a node strictly from its cache. `None` when the node carries no cache
/// for the core properties, so a missing cache can never fall back to one
/// cross-process call per property.
pub(super) unsafe fn read(element: &IUIAutomationElement, with_values: bool) -> Option<CachedNode> {
    unsafe {
        let rect = element.CachedBoundingRectangle().ok()?;
        if element.CachedIsOffscreen().ok()?.as_bool() {
            return None;
        }
        let name = element.CachedName().ok()?.to_string();
        let control_type = element.CachedControlType().ok()?.0;
        let automation_id = element.CachedAutomationId().ok()?.to_string();
        let enabled = element.CachedIsEnabled().ok()?.as_bool();
        let password = element.CachedIsPassword().ok().map(|value| value.as_bool());
        let required = element
            .CachedIsRequiredForForm()
            .is_ok_and(|value| value.as_bool());
        let mut node = CachedNode {
            rect,
            name,
            control_type,
            automation_id,
            enabled,
            password,
            required,
            state: None,
            value_supported: false,
            value_writable: false,
        };
        if with_values {
            let property = |id| element.GetCachedPropertyValue(id).ok();
            node.state = state_from(&property);
            node.value_supported = flag(property(UIA_IsValuePatternAvailablePropertyId));
            node.value_writable = node.value_supported
                && property(UIA_ValueIsReadOnlyPropertyId)
                    .as_ref()
                    .and_then(boolean)
                    .is_some_and(|read_only| !read_only);
        }
        Some(node)
    }
}

/// Ground-truth control state from cached pattern properties: toggle,
/// selected item, expand/collapse, then range value. The pattern-specific
/// value is trusted only when the provider reports that pattern available.
fn state_from(property: &impl Fn(UIA_PROPERTY_ID) -> Option<VARIANT>) -> Option<String> {
    let available = |id| flag(property(id));
    if available(UIA_IsTogglePatternAvailablePropertyId)
        && let Some(state) = property(UIA_ToggleToggleStatePropertyId)
            .as_ref()
            .and_then(integer)
    {
        let label = if state == ToggleState_On.0 {
            "on"
        } else if state == ToggleState_Indeterminate.0 {
            "mixed"
        } else {
            "off"
        };
        return Some(label.to_string());
    }
    if available(UIA_IsSelectionItemPatternAvailablePropertyId)
        && property(UIA_SelectionItemIsSelectedPropertyId)
            .as_ref()
            .and_then(boolean)
            == Some(true)
    {
        return Some("selected".to_string());
    }
    if available(UIA_IsExpandCollapsePatternAvailablePropertyId)
        && let Some(state) = property(UIA_ExpandCollapseExpandCollapseStatePropertyId)
            .as_ref()
            .and_then(integer)
    {
        if state == ExpandCollapseState_Expanded.0 {
            return Some("expanded".to_string());
        } else if state == ExpandCollapseState_Collapsed.0 {
            return Some("collapsed".to_string());
        } else if state == ExpandCollapseState_PartiallyExpanded.0 {
            return Some("partly-expanded".to_string());
        }
    }
    if available(UIA_IsRangeValuePatternAvailablePropertyId)
        && let Some(value) = property(UIA_RangeValueValuePropertyId)
            .as_ref()
            .and_then(real)
    {
        let rounded = (value * 100.0).round() / 100.0;
        return Some(format!("value {rounded}"));
    }
    None
}

fn flag(value: Option<VARIANT>) -> bool {
    value.as_ref().and_then(boolean) == Some(true)
}

/// Typed readers reject the provider's "not supported" sentinel and any
/// coerced type instead of guessing a value from it.
fn boolean(value: &VARIANT) -> Option<bool> {
    (value.vt() == VT_BOOL)
        .then(|| bool::try_from(value).ok())
        .flatten()
}

fn integer(value: &VARIANT) -> Option<i32> {
    (value.vt() == VT_I4)
        .then(|| i32::try_from(value).ok())
        .flatten()
}

fn real(value: &VARIANT) -> Option<f64> {
    (value.vt() == VT_R8)
        .then(|| f64::try_from(value).ok())
        .flatten()
}

#[cfg(test)]
#[path = "cached_element_tests.rs"]
mod tests;
