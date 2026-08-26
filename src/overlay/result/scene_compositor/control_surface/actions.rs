use super::super::protocol::{ButtonAction, DragOutcome};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub(in crate::overlay::result::scene_compositor) fn handle(id: isize, action: ButtonAction) {
    let hwnd = HWND(id as *mut std::ffi::c_void);
    if !matches!(
        action,
        ButtonAction::SetOpacity { .. } | ButtonAction::UpdateRefineDraft { .. }
    ) {
        crate::overlay::result::raise_window(hwnd);
    }
    match action {
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
        ButtonAction::UpdateRefineDraft { text } => {
            crate::overlay::result::refine::update_refine_draft(hwnd, &text)
        }
        ButtonAction::SubmitRefine { text } => {
            crate::overlay::result::trigger_refine_submit(hwnd, &text)
        }
        ButtonAction::CancelRefine => crate::overlay::result::trigger_refine_cancel(hwnd),
        ButtonAction::HistoryUpRefine { text } => update_history(hwnd, &text, true),
        ButtonAction::HistoryDownRefine { text } => update_history(hwnd, &text, false),
        ButtonAction::Mic => start_microphone(),
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
    }
}

fn post(hwnd: HWND, message: u32) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0));
    }
}

fn update_history(hwnd: HWND, current: &str, upward: bool) {
    let text = if upward {
        crate::overlay::input_history::navigate_history_up(current)
    } else {
        crate::overlay::input_history::navigate_history_down(current)
    };
    if let Some(text) = text {
        crate::overlay::result::set_refine_text(hwnd, &text, false);
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
