use windows::Win32::Foundation::HWND;

use super::refine_session::RefineSubmission;
use super::state::WINDOW_STATES;

/// Trigger edit/refine action
pub fn trigger_edit(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    let mut states = WINDOW_STATES.lock().unwrap();
    if let Some(state) = states.get_mut(&hwnd_key) {
        if state.refine_session.is_editing() {
            state.refine_session.cancel_edit();
        } else {
            state.refine_session.begin_edit();
        }
    }
    drop(states);

    super::scene_compositor::sync_controls(hwnd);
}

pub(crate) fn update_refine_draft(hwnd: HWND, text: &str) {
    let changed = WINDOW_STATES
        .lock()
        .unwrap()
        .get_mut(&(hwnd.0 as isize))
        .is_some_and(|state| state.refine_session.set_draft(text));
    if changed {
        super::scene_compositor::update_cached_refine_draft(hwnd, text);
    }
}

pub fn trigger_refine_submit(hwnd: HWND, text: &str) {
    if text.trim().is_empty() {
        return;
    }

    let hwnd_key = hwnd.0 as isize;

    crate::overlay::input_history::add_to_history(text);

    let submission = (|| {
        let mut states = WINDOW_STATES.lock().unwrap();
        let state = states.get_mut(&hwnd_key)?;
        let original_text = state.full_text.clone();
        let submission = state
            .refine_session
            .begin_submit(original_text.clone(), text)?;
        state.text_history.push(original_text.clone());
        state.redo_history.clear();
        state.input_text = original_text;
        state.full_text.clear();
        state.is_refining = true;
        state.is_streaming_active = true;
        Some(submission)
    })();
    let Some(submission) = submission else {
        return;
    };
    super::update_window_text(hwnd, "");
    start_refinement(hwnd, submission);
    super::scene_compositor::sync_controls(hwnd);
}

pub fn trigger_refine_cancel(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            state.refine_session.cancel_edit();
        }
    }
    super::scene_compositor::sync_controls(hwnd);
}

fn start_refinement(hwnd: HWND, submission: RefineSubmission) {
    let hwnd_key = hwnd.0 as isize;
    let (context_data, model_id, provider, streaming, chain_token) = {
        let states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get(&hwnd_key) {
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

    let hwnd_val = hwnd.0 as usize;
    std::thread::spawn(move || {
        let capture_hwnd = HWND(hwnd_val as *mut std::ffi::c_void);

        let (groq_key, gemini_key) = {
            let app = crate::APP.lock().unwrap();
            (
                app.config.api_key.clone(),
                app.config.gemini_api_key.clone(),
            )
        };

        let api_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let api_cancel_cb = api_cancel.clone();
        let chain_token_cb = chain_token.clone();

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
                cancel_token: Some(api_cancel),
            },
            move |chunk| {
                if let Some(ref ct) = chain_token_cb
                    && ct.is_cancelled()
                {
                    api_cancel_cb.store(true, std::sync::atomic::Ordering::SeqCst);
                    return;
                }

                if first_chunk {
                    let mut states = WINDOW_STATES.lock().unwrap();
                    if let Some(state) = states.get_mut(&(capture_hwnd.0 as isize)) {
                        state.is_refining = false;
                        state.refine_session.mark_streaming();
                    }
                    first_chunk = false;
                }

                if let Some(wiped) = chunk.strip_prefix(crate::api::WIPE_SIGNAL) {
                    acc_text.clear();
                    acc_text.push_str(wiped);
                } else {
                    acc_text.push_str(chunk);
                }
                super::update_window_text(capture_hwnd, &acc_text);
            },
        );

        let accumulated_len = WINDOW_STATES
            .lock()
            .unwrap()
            .get(&(capture_hwnd.0 as isize))
            .map(|state| state.full_text.len())
            .unwrap_or(0);
        let final_text = match result {
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
                final_text
            }
            Err(e) => {
                let (lang, model_full_name) = {
                    let app = crate::APP.lock().unwrap();
                    let full_name = crate::model_config::get_model_by_id(&model_id)
                        .map(|m| m.full_name)
                        .unwrap_or_else(|| model_id.to_string());
                    (app.config.ui_language.clone(), full_name)
                };
                crate::overlay::utils::show_api_key_error_notification(&e.to_string(), &lang);
                crate::overlay::utils::get_error_message(
                    &e.to_string(),
                    &lang,
                    Some(&model_full_name),
                )
            }
        };
        {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&(capture_hwnd.0 as isize)) {
                state.is_refining = false;
                state.is_streaming_active = false;
                state.refine_session.finish();
            }
        }
        super::update_window_text(capture_hwnd, &final_text);
    });
}
