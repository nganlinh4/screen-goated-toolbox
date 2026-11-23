use super::super::protocol::{ButtonAction, DragOutcome};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub(in crate::overlay::result::scene_compositor) fn handle(id: isize, action: ButtonAction) {
    let hwnd = HWND(id as *mut std::ffi::c_void);
    let session = match &action {
        ButtonAction::UpdateRefineDraft { session, .. }
        | ButtonAction::SubmitRefine { session, .. }
        | ButtonAction::CancelRefine { session }
        | ButtonAction::HistoryUpRefine { session, .. }
        | ButtonAction::HistoryDownRefine { session, .. }
        | ButtonAction::Mic { session } => Some(*session),
        _ => None,
    };
    if session
        .is_some_and(|session| !crate::overlay::result::is_current_refine_session(hwnd, session))
    {
        return;
    }
    if !matches!(
        action,
        ButtonAction::SetOpacity { .. } | ButtonAction::UpdateRefineDraft { .. }
    ) {
        crate::overlay::result::raise_window(hwnd);
    }
    match action {
        ButtonAction::CopyImage => super::super::source_image::request(id),
        ButtonAction::Copy => post(
            hwnd,
            crate::overlay::result::event_handler::misc::WM_COPY_CLICK,
        ),
        ButtonAction::Undo => post(
            hwnd,
            crate::overlay::result::event_handler::misc::WM_UNDO_CLICK,
        ),
        ButtonAction::Redo => post(
            hwnd,
            crate::overlay::result::event_handler::misc::WM_REDO_CLICK,
        ),
        ButtonAction::Edit => post(
            hwnd,
            crate::overlay::result::event_handler::misc::WM_EDIT_CLICK,
        ),
        ButtonAction::Download => post(
            hwnd,
            crate::overlay::result::event_handler::misc::WM_DOWNLOAD_CLICK,
        ),
        ButtonAction::Back => post(
            hwnd,
            crate::overlay::result::event_handler::misc::WM_BACK_CLICK,
        ),
        ButtonAction::Forward => post(
            hwnd,
            crate::overlay::result::event_handler::misc::WM_FORWARD_CLICK,
        ),
        ButtonAction::Speaker => post(
            hwnd,
            crate::overlay::result::event_handler::misc::WM_SPEAKER_CLICK,
        ),
        ButtonAction::SetOpacity { value } => {
            crate::overlay::result::scene_compositor::set_control_scope_opacity(hwnd, value)
        }
        ButtonAction::UpdateRefineDraft { text, session } => {
            crate::overlay::result::refine::update_refine_draft(hwnd, session, &text)
        }
        ButtonAction::SubmitRefine { text, session } => {
            crate::overlay::result::trigger_refine_submit(hwnd, session, &text)
        }
        ButtonAction::CancelRefine { session } => {
            crate::overlay::result::refine::trigger_refine_cancel_for_session(hwnd, session)
        }
        ButtonAction::HistoryUpRefine { text, session } => {
            update_history(hwnd, session, &text, true)
        }
        ButtonAction::HistoryDownRefine { text, session } => {
            update_history(hwnd, session, &text, false)
        }
        ButtonAction::Mic { session } => {
            if crate::overlay::result::is_current_refine_session(hwnd, session) {
                start_microphone();
            }
        }
    }
}

pub(in crate::overlay::result::scene_compositor) fn handle_drag_finished(
    id: isize,
    targets: &[isize],
    outcome: DragOutcome,
) {
    let hwnd = HWND(id as *mut std::ffi::c_void);
    match outcome {
        DragOutcome::Moved => {
            if crate::overlay::result::scene_compositor::source_group_ids(id).is_some() {
                return;
            }
            for target in targets {
                crate::overlay::result::event_handler::save_window_geometry(
                    HWND(*target as *mut std::ffi::c_void),
                    "COMPOSITOR_DRAG",
                );
            }
        }
        DragOutcome::CloseOne => crate::overlay::result::trigger_close_window(hwnd),
        DragOutcome::CloseGroup => crate::overlay::result::trigger_close_group(hwnd),
        DragOutcome::CloseAll => crate::overlay::result::trigger_close_all(),
        DragOutcome::Cancelled => {}
    }
}

fn post(hwnd: HWND, message: u32) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0));
    }
}

fn update_history(hwnd: HWND, session: u64, current: &str, upward: bool) {
    if !crate::overlay::result::is_current_refine_session(hwnd, session) {
        return;
    }
    let text = if upward {
        crate::overlay::input_history::navigate_history_up(current)
    } else {
        crate::overlay::input_history::navigate_history_down(current)
    };
    if let Some(text) = text {
        crate::overlay::result::replace_refine_text_for_session(hwnd, session, &text);
    }
}

fn start_microphone() {
    let preset = crate::APP
        .lock()
        .unwrap()
        .config
        .presets
        .iter()
        .position(|preset| preset.id == "preset_transcribe");
    if let Some(index) = preset {
        std::thread::spawn(move || crate::overlay::recording::show_recording_overlay(index));
    }
}
