use crate::win_types::SendHwnd;

/// Destination selected when a processing request starts.
#[derive(Clone, Copy, Debug)]
pub enum AutoPasteDestination {
    /// The external window active when the request began, if any.
    External(Option<SendHwnd>),
    /// The text input session selected for this request.
    TextInput(u64),
    /// The refine session selected for this request.
    Refine(SendHwnd, u64),
}

impl AutoPasteDestination {
    pub(crate) fn capture_current() -> Self {
        let external = crate::APP
            .lock()
            .ok()
            .and_then(|app| app.last_active_window);
        Self::capture(external)
    }

    pub(crate) fn capture_foreground() -> Self {
        Self::capture(crate::overlay::utils::get_target_window_for_paste().map(SendHwnd))
    }

    pub(crate) fn capture(external: Option<SendHwnd>) -> Self {
        match crate::overlay::text_input::try_active_session() {
            Ok(Some(session)) => Self::TextInput(session),
            Err(_) => Self::External(None),
            Ok(None) => match crate::overlay::result::active_refine_session() {
                Ok(Some((hwnd, session))) => Self::Refine(hwnd, session),
                Ok(None) => Self::External(external),
                Err(_) => Self::External(None),
            },
        }
    }
}
