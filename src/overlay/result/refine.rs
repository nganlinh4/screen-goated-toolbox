use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use windows::Win32::Foundation::HWND;

use super::refine_session::RefineSubmission;
use super::state::{WINDOW_STATES, WindowState};

fn same_request(current: Option<&Arc<AtomicBool>>, candidate: &Arc<AtomicBool>) -> bool {
    current.is_some_and(|current| Arc::ptr_eq(current, candidate))
}

fn is_current(state: &WindowState, candidate: &Arc<AtomicBool>) -> bool {
    !candidate.load(Ordering::SeqCst) && same_request(state.refine_cancel_token.as_ref(), candidate)
}

fn is_current_window(hwnd: HWND, candidate: &Arc<AtomicBool>) -> bool {
    WINDOW_STATES
        .lock()
        .unwrap()
        .get(&(hwnd.0 as isize))
        .is_some_and(|state| is_current(state, candidate))
}

pub(super) fn apply_refine_text(
    state: &mut WindowState,
    candidate: &Arc<AtomicBool>,
    text: &str,
    first: bool,
) -> bool {
    if !is_current(state, candidate) {
        return false;
    }
    if first {
        state.is_refining = false;
        state.refine_session.mark_streaming();
    }
    state.full_text = text.to_owned();
    true
}

fn update_refine_text(hwnd: HWND, candidate: &Arc<AtomicBool>, text: &str, first: bool) -> bool {
    let changed = {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
            apply_refine_text(state, candidate, text, first)
        } else {
            false
        }
    };
    if changed {
        super::scene_compositor::queue_window_sync(hwnd);
        super::raw_webview::request_sync(hwnd);
    }
    changed
}

fn update_refine_model(
    hwnd: HWND,
    candidate: &Arc<AtomicBool>,
    model: &str,
    name: &str,
    provider: &str,
) {
    let changed = {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states
            .get_mut(&(hwnd.0 as isize))
            .filter(|state| is_current(state, candidate))
        {
            state.model_id = model.to_owned();
            state.model_name = name.to_owned();
            state.provider = provider.to_owned();
            true
        } else {
            false
        }
    };
    if changed {
        super::scene_compositor::sync_controls(hwnd);
    }
}

/// Retires the in-flight refinement at the moment it is cancelled. The
/// pre-refinement text returns immediately rather than when a blocked provider
/// transport finally unwinds, and the undo entry pushed for it is dropped
/// because the restored text is that entry. Late output is fenced by identity.
pub(crate) fn retire_in_flight(state: &mut WindowState) -> bool {
    let Some(token) = state.refine_cancel_token.take() else {
        return false;
    };
    token.store(true, Ordering::SeqCst);
    state.is_refining = false;
    state.is_streaming_active = false;
    if let Some(original) = state.refine_session.submitted_original().map(str::to_owned) {
        if state.text_history.last() == Some(&original) {
            state.text_history.pop();
        }
        state.full_text = original;
    }
    state.refine_session.finish();
    true
}

fn restore_cancelled_refinement(hwnd: HWND, candidate: &Arc<AtomicBool>) {
    let restored = WINDOW_STATES
        .lock()
        .unwrap()
        .get_mut(&(hwnd.0 as isize))
        .filter(|state| same_request(state.refine_cancel_token.as_ref(), candidate))
        .is_some_and(retire_in_flight);
    if restored {
        super::scene_compositor::queue_window_sync(hwnd);
        super::raw_webview::request_sync(hwnd);
        super::scene_compositor::sync_controls(hwnd);
    }
}

/// Ancestor cancellation retires the refinement when it fires, not when the
/// worker's provider call returns. The worker keeps the returned guard alive.
pub(crate) fn retire_on_ancestor_cancel(
    hwnd: HWND,
    chain: Option<&Arc<super::state::ChainCancelToken>>,
    candidate: &Arc<AtomicBool>,
) -> Arc<dyn Fn() + Send + Sync> {
    let hwnd_val = hwnd.0 as isize;
    let candidate = candidate.clone();
    let retire: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        restore_cancelled_refinement(HWND(hwnd_val as *mut std::ffi::c_void), &candidate);
    });
    if let Some(chain) = chain {
        chain.register_cancel_callback(&retire);
    }
    retire
}

/// Trigger edit/refine action
pub fn trigger_edit(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    let mut states = WINDOW_STATES.lock().unwrap();
    if let Some(state) = states.get_mut(&hwnd_key) {
        if state.refine_session.is_editing() {
            state.refine_session.cancel_edit();
        } else {
            retire_in_flight(state);
            state.refine_session.begin_edit();
        }
    }
    drop(states);

    super::scene_compositor::sync_controls(hwnd);
}

pub(crate) fn update_refine_draft(hwnd: HWND, session: u64, text: &str) {
    let changed = WINDOW_STATES
        .lock()
        .unwrap()
        .get_mut(&(hwnd.0 as isize))
        .is_some_and(|state| {
            state.refine_session.edit_id() == Some(session) && state.refine_session.set_draft(text)
        });
    if changed {
        super::scene_compositor::update_cached_refine_draft(hwnd, session, text);
    }
}

pub fn trigger_refine_submit(hwnd: HWND, session: u64, text: &str) {
    if text.trim().is_empty() {
        return;
    }

    let hwnd_key = hwnd.0 as isize;

    let submission = (|| {
        let mut states = WINDOW_STATES.lock().unwrap();
        let state = states.get_mut(&hwnd_key)?;
        if state.refine_session.edit_id() != Some(session) {
            return None;
        }
        let original_text = state.full_text.clone();
        let submission = state
            .refine_session
            .begin_submit(original_text.clone(), text)?;
        if let Some(previous) = state.refine_cancel_token.take() {
            previous.store(true, Ordering::SeqCst);
        }
        let api_cancel = Arc::new(AtomicBool::new(false));
        if let Some(chain_token) = &state.cancellation_token {
            chain_token.register_cancel_flag(&api_cancel);
        }
        state.refine_cancel_token = Some(api_cancel.clone());
        state.text_history.push(original_text.clone());
        state.redo_history.clear();
        state.input_text = original_text;
        state.full_text.clear();
        state.is_refining = true;
        state.is_streaming_active = true;
        Some((submission, api_cancel))
    })();
    let Some((submission, api_cancel)) = submission else {
        return;
    };
    crate::overlay::input_history::add_to_history(text);
    super::scene_compositor::queue_window_sync(hwnd);
    super::raw_webview::request_sync(hwnd);
    start_refinement(hwnd, submission, api_cancel);
    super::scene_compositor::sync_controls(hwnd);
}

pub fn trigger_refine_cancel(hwnd: HWND) {
    cancel_refine(hwnd, None);
}

pub(crate) fn trigger_refine_cancel_for_session(hwnd: HWND, session: u64) {
    cancel_refine(hwnd, Some(session));
}

fn cancel_refine(hwnd: HWND, session: Option<u64>) {
    let hwnd_key = hwnd.0 as isize;
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key)
            && session.is_none_or(|id| state.refine_session.edit_id() == Some(id))
        {
            state.refine_session.cancel_edit();
        }
    }
    super::scene_compositor::sync_controls(hwnd);
}

fn start_refinement(hwnd: HWND, submission: RefineSubmission, api_cancel: Arc<AtomicBool>) {
    let hwnd_key = hwnd.0 as isize;
    let (context_data, model_id, provider, streaming, chain) = {
        let states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states
            .get(&hwnd_key)
            .filter(|state| same_request(state.refine_cancel_token.as_ref(), &api_cancel))
        {
            (
                state.context_data.clone(),
                state.model_id.clone(),
                state.provider.clone(),
                state.streaming_enabled,
                state.cancellation_token.clone(),
            )
        } else {
            return;
        }
    };
    let ancestor_guard = retire_on_ancestor_cancel(hwnd, chain.as_ref(), &api_cancel);

    let hwnd_val = hwnd.0 as usize;
    std::thread::spawn(move || {
        let _ancestor_guard = ancestor_guard;
        let capture_hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        if api_cancel.load(Ordering::SeqCst) {
            restore_cancelled_refinement(capture_hwnd, &api_cancel);
            return;
        }

        let (groq_key, gemini_key) = {
            let app = crate::APP.lock().unwrap();
            (
                app.config.api_key.clone(),
                app.config.gemini_api_key.clone(),
            )
        };

        let api_cancel_cb = api_cancel.clone();
        let api_cancel_model = api_cancel.clone();

        let mut acc_text = String::new();
        let mut first_chunk = true;

        let ui_language = {
            let app = crate::APP.lock().unwrap();
            app.config.ui_language.clone()
        };

        let result = crate::api::refine_text_streaming(
            crate::api::RefineTextRequest {
                groq_api_key: &groq_key,
                gemini_api_key: &gemini_key,
                context: context_data,
                previous_text: submission.original_text,
                user_prompt: submission.instruction,
                original_model_id: &model_id,
                original_provider: &provider,
                streaming_enabled: streaming,
                ui_language: &ui_language,
                cancel_token: Some(api_cancel.clone()),
                on_model_selected: &mut |model, name, provider| {
                    update_refine_model(capture_hwnd, &api_cancel_model, model, name, provider);
                },
            },
            move |chunk| {
                if api_cancel_cb.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(wiped) = chunk.strip_prefix(crate::api::WIPE_SIGNAL) {
                    acc_text.clear();
                    acc_text.push_str(wiped);
                } else {
                    acc_text.push_str(chunk);
                }
                if update_refine_text(capture_hwnd, &api_cancel_cb, &acc_text, first_chunk) {
                    first_chunk = false;
                }
            },
        );

        if api_cancel.load(Ordering::SeqCst) {
            restore_cancelled_refinement(capture_hwnd, &api_cancel);
            return;
        }
        if !is_current_window(capture_hwnd, &api_cancel) {
            return;
        }
        let accumulated_len = WINDOW_STATES
            .lock()
            .unwrap()
            .get(&(capture_hwnd.0 as isize))
            .map(|state| state.full_text.len())
            .unwrap_or(0);
        let (final_text, notification) = match result {
            Ok(final_text) => {
                if final_text.trim().is_empty() {
                    crate::log_info!(
                        "[MarkdownDiag] blank_model_result hwnd={} provider={} model={} final_len={} accumulated_len={}",
                        capture_hwnd.0 as isize,
                        provider,
                        model_id,
                        final_text.len(),
                        accumulated_len
                    );
                }
                (final_text, None)
            }
            Err(e) => {
                let (lang, model_full_name) = {
                    let app = crate::APP.lock().unwrap();
                    let full_name = crate::model_config::get_model_by_id(&model_id)
                        .map(|m| m.full_name)
                        .unwrap_or_else(|| model_id.to_string());
                    (app.config.ui_language.clone(), full_name)
                };
                let detail = e.to_string();
                (
                    crate::overlay::utils::get_error_message(
                        &detail,
                        &lang,
                        Some(&model_full_name),
                    ),
                    Some((detail, lang)),
                )
            }
        };
        let accepted = {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states
                .get_mut(&(capture_hwnd.0 as isize))
                .filter(|state| is_current(state, &api_cancel))
            {
                state.is_refining = false;
                state.is_streaming_active = false;
                state.refine_session.finish();
                state.full_text = final_text;
                state.refine_cancel_token = None;
                true
            } else {
                false
            }
        };
        if accepted {
            if let Some((detail, lang)) = notification
                && !api_cancel.load(Ordering::SeqCst)
            {
                crate::overlay::utils::show_api_key_error_notification(&detail, &lang);
            }
            super::scene_compositor::queue_window_sync(capture_hwnd);
            super::raw_webview::request_sync(capture_hwnd);
        } else if api_cancel.load(Ordering::SeqCst) {
            restore_cancelled_refinement(capture_hwnd, &api_cancel);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_identity_rejects_replacement_even_if_both_are_live() {
        let first = Arc::new(AtomicBool::new(false));
        let second = Arc::new(AtomicBool::new(false));
        assert!(same_request(Some(&first), &first));
        assert!(!same_request(Some(&second), &first));
        assert!(!same_request(None, &first));
    }
}
