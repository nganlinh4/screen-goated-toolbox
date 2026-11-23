use super::Session;
use std::sync::Arc;
use windows::Win32::{Foundation::RECT, Graphics::Gdi::*};

#[derive(Debug)]
pub(super) struct DisplayChanged;
impl std::fmt::Display for DisplayChanged {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("subtitle capture display changed")
    }
}
impl std::error::Error for DisplayChanged {}

pub(super) fn validate(monitor: HMONITOR, bounds: RECT) -> anyhow::Result<()> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let current =
        unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() }.then_some(info.rcMonitor);
    if changed(bounds, current) {
        return Err(DisplayChanged.into());
    }
    Ok(())
}

fn changed(saved: RECT, current: Option<RECT>) -> bool {
    current != Some(saved)
}

pub(super) fn stop(session: &Arc<Session>) {
    let language = session.view.lock().unwrap().language.clone();
    if super::stop_expected(Some(session)) {
        let text = crate::gui::locale::LocaleText::get(&language).screen_translate;
        crate::log_info!("[Subtitles] display changed; capture closed; notice language={language}");
        crate::overlay::auto_copy_badge::show_detailed_notification(
            text.screen_translate_title,
            text.screen_translate_subtitle_display_changed,
            crate::overlay::auto_copy_badge::NotificationType::Info,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_display_invalidation_includes_removal_resize_and_origin_change() {
        let bounds = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        assert!(!changed(bounds, Some(bounds)));
        assert!(changed(bounds, None));
        assert!(changed(
            bounds,
            Some(RECT {
                right: 1280,
                ..bounds
            })
        ));
        assert!(changed(
            bounds,
            Some(RECT {
                left: -1920,
                right: 0,
                ..bounds
            })
        ));
    }
}
