//! Debug lab admission to the real native session after a supplied selection.
use windows::Win32::Foundation::RECT;

pub(in super::super) fn region(value: &serde_json::Value) -> Option<RECT> {
    let edge = |key| value.get(key)?.as_i64().and_then(|n| i32::try_from(n).ok());
    let rect = RECT {
        left: edge("left")?,
        top: edge("top")?,
        right: edge("right")?,
        bottom: edge("bottom")?,
    };
    let width = i64::from(rect.right) - i64::from(rect.left);
    let height = i64::from(rect.bottom) - i64::from(rect.top);
    ((16..=32768).contains(&width) && (16..=32768).contains(&height)).then_some(rect)
}

pub(in super::super) fn start(value: &serde_json::Value, observed: super::super::LabSnapshot) {
    if crate::paths::runtime_state_root().is_none() {
        return;
    }
    let Some(rect) = region(value) else { return };
    let mut admission = super::engine::DIAGNOSTIC_ADMISSION.lock().unwrap();
    if !admission.accepts_snapshot(observed.subtitle) {
        return;
    }
    let Some(_claim) = super::super::claim_lab_capture(observed) else {
        return;
    };
    let result = super::super::runtime::with_lab_start_guard(observed.jobs, || {
        let result = super::start_region(rect, String::new());
        admission.invalidate();
        result
    });
    if let Some(Err(error)) = result {
        super::super::capture::notify_error(&error.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_subtitle_acceptance_rejects_empty_overflow_and_inverted_regions() {
        for value in [
            serde_json::json!({}),
            serde_json::json!({"left":0,"top":0,"right":-1,"bottom":20}),
            serde_json::json!({"left":0,"top":0,"right":4294967296i64,"bottom":20}),
        ] {
            assert!(region(&value).is_none());
        }
        assert!(region(&serde_json::json!({"left":0,"top":0,"right":100,"bottom":20})).is_some());
    }
}
