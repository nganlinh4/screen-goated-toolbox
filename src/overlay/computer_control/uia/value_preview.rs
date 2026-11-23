use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationTextPattern, UIA_TextPatternId,
};
use windows::core::BSTR;

const MAX_PREVIEW_UNITS: usize = 1024;

#[derive(Clone, Debug, Default)]
pub(in crate::overlay::computer_control) struct ValuePreview {
    pub text: Option<String>,
    pub incomplete: bool,
    pub supported: bool,
    pub writable: bool,
}

// In-memory fixtures may supply a known complete observation without a provider.
#[cfg(test)]
impl From<Option<String>> for ValuePreview {
    fn from(text: Option<String>) -> Self {
        Self {
            supported: text.is_some(),
            writable: text.is_some(),
            text,
            incomplete: false,
        }
    }
}

/// Bounded text preview with value capability taken from the enumeration
/// cache. Only the bounded text read itself reaches the provider; it runs
/// only for text-value controls, which are a small part of a tree.
pub(super) unsafe fn read(
    element: &IUIAutomationElement,
    readable: bool,
    capability: Capability,
) -> ValuePreview {
    if !readable {
        return ValuePreview {
            incomplete: true,
            ..ValuePreview::default()
        };
    }
    let mut preview = read_with(|maximum| unsafe {
        element
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .ok()?
            .DocumentRange()
            .ok()?
            .GetText(maximum)
            .ok()
    });
    preview.supported = capability.value_supported;
    preview.writable = capability.value_writable;
    preview
}

/// Cached ValuePattern capability for one node.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Capability {
    pub value_supported: bool,
    pub value_writable: bool,
}

fn read_with(read: impl FnOnce(i32) -> Option<BSTR>) -> ValuePreview {
    let Some(text) = read((MAX_PREVIEW_UNITS + 1) as i32) else {
        return ValuePreview {
            incomplete: true,
            ..ValuePreview::default()
        };
    };
    let units: &[u16] = &text;
    let truncated = units.len() > MAX_PREVIEW_UNITS;
    let mut length = units.len().min(MAX_PREVIEW_UNITS);
    if truncated && length > 0 && (0xd800..=0xdbff).contains(&units[length - 1]) {
        length -= 1;
    }
    let converted = String::from_utf16(&units[..length]);
    ValuePreview {
        incomplete: truncated || converted.is_err(),
        text: Some(converted.unwrap_or_else(|_| String::from_utf16_lossy(&units[..length]))),
        ..ValuePreview::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_a_bounded_prefix_and_reports_overlong_provider_output() {
        let result = read_with(|maximum| {
            assert_eq!(maximum, 1025);
            Some(BSTR::from_wide(&vec![b'x' as u16; 20000]))
        });
        assert_eq!(result.text.as_ref().unwrap().len(), MAX_PREVIEW_UNITS);
        assert!(result.incomplete);
    }
    #[test]
    fn unsupported_is_unknown_but_an_empty_text_range_is_observed() {
        let missing = read_with(|_| None);
        assert!(missing.text.is_none());
        assert!(missing.incomplete);
        let empty = read_with(|_| Some(BSTR::new()));
        assert_eq!(empty.text.as_deref(), Some(""));
        assert!(!empty.incomplete);
    }
    #[test]
    fn truncation_preserves_surrogate_boundaries_and_invalid_utf16_is_incomplete() {
        let mut units = vec![b'x' as u16; MAX_PREVIEW_UNITS - 1];
        units.extend([0xd83d, 0xde00]);
        let result = read_with(|_| Some(BSTR::from_wide(&units)));
        assert!(result.incomplete);
        assert!(!result.text.unwrap().contains('\u{fffd}'));
        let result = read_with(|_| Some(BSTR::from_wide(&[0xd800])));
        assert!(result.incomplete);
    }
}
