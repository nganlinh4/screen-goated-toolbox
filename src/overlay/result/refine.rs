use windows::Win32::Foundation::HWND;

use super::button_canvas;
use super::state::WINDOW_STATES;

/// Trigger edit/refine action
pub fn trigger_edit(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;

    {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            state.is_editing = !state.is_editing;
            if state.is_editing {
                state.input_text.clear();
            }
        }
    }

    button_canvas::update_window_position(hwnd);
}

pub fn trigger_refine_submit(hwnd: HWND, text: &str) {
    if text.trim().is_empty() {
        return;
    }

    let hwnd_key = hwnd.0 as isize;

    crate::overlay::input_history::add_to_history(text);

    let mut should_trigger_refine = false;
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            let text_to_refine = state.full_text.clone();
            state.text_history.push(text_to_refine.clone());
            state.redo_history.clear();
            state.input_text = text_to_refine;
            state.full_text = String::new();
            should_trigger_refine = true;
        }
    }

    if should_trigger_refine {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            state.is_editing = false;
            state.is_refining = true;
            state.is_streaming_active = true;
        }
        drop(states);
        super::update_window_text(hwnd, "");
        start_refinement(hwnd, text);
    }
    button_canvas::update_window_position(hwnd);
}

pub fn trigger_refine_cancel(hwnd: HWND) {
    let hwnd_key = hwnd.0 as isize;
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            state.is_editing = false;
        }
    }
    button_canvas::update_window_position(hwnd);
}

fn start_refinement(hwnd: HWND, user_prompt: &str) {
    let hwnd_key = hwnd.0 as isize;
    let (context_data, model_id, provider, streaming, preset_prompt, prev_text, chain_token) = {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(s) = states.get_mut(&hwnd_key) {
            let prev = s.full_text.clone();
            (
                s.context_data.clone(),
                s.model_id.clone(),
                s.provider.clone(),
                s.streaming_enabled,
                s.preset_prompt.clone(),
                prev,
                s.cancellation_token.clone(),
            )
        } else {
            return;
        }
    };

    let user_input = user_prompt.to_string();
    let (final_prev_text, final_user_prompt) =
        if prev_text.trim().is_empty() && !preset_prompt.is_empty() {
            (user_input, preset_prompt)
        } else {
            (prev_text, user_input)
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
                previous_text: final_prev_text,
                user_prompt: final_user_prompt,
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
            }
        }
        super::update_window_text(capture_hwnd, &final_text);
    });
}
