// --- CHAIN BLOCK EXECUTION ---
// API execution with retry logic for chain processing blocks.

use crate::api::{
    TranslateImageRequest, TranslateTextRequest, translate_image_streaming,
    translate_text_streaming,
};
use crate::config::{Config, ProcessingBlock};
use crate::gui::settings_ui::get_localized_preset_name;
use crate::overlay::result::{ChainCancelToken, RefineContext, WINDOW_STATES, update_window_text};
use crate::win_types::SendHwnd;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct ExecuteBlockRequest<'a> {
    pub block: &'a ProcessingBlock,
    pub block_idx: usize,
    pub blocks: &'a [ProcessingBlock],
    pub my_hwnd: Option<HWND>,
    pub input_text: &'a str,
    pub context: &'a RefineContext,
    pub model_id: &'a str,
    pub provider: &'a str,
    pub model_full_name: &'a str,
    pub final_prompt: &'a str,
    pub skip_execution: bool,
    pub config: &'a Config,
    pub preset_id: &'a str,
    pub processing_hwnd_shared: Option<SendHwnd>,
    pub cancel_token: &'a Arc<ChainCancelToken>,
}

/// Execute the block's API call and return the result text.
pub fn execute_block(request: ExecuteBlockRequest<'_>) -> String {
    let ExecuteBlockRequest {
        block,
        block_idx,
        blocks,
        my_hwnd,
        input_text,
        context,
        model_id,
        provider,
        model_full_name,
        final_prompt,
        skip_execution,
        config,
        preset_id,
        processing_hwnd_shared,
        cancel_token,
    } = request;
    if block.block_type == "input_adapter" {
        return input_text.to_string();
    }

    if skip_execution {
        if let Some(h) = my_hwnd {
            update_window_text(h, input_text);
        }
        return input_text.to_string();
    }

    let groq_key = config.api_key.clone();
    let gemini_key = config.gemini_api_key.clone();
    let use_json = block_idx == 0 && blocks.len() == 1 && blocks[0].block_type == "image";

    let actual_streaming_enabled = if block.render_mode == "markdown" {
        false
    } else {
        block.streaming_enabled
    };

    let accumulated = Arc::new(Mutex::new(String::new()));
    let is_first_processing_block = blocks
        .iter()
        .position(|b| b.block_type != "input_adapter")
        .map(|pos| pos == block_idx)
        .unwrap_or(false);

    // Retry variables
    let mut current_model_id = model_id.to_string();
    let mut current_provider = provider.to_string();
    let mut current_model_full_name = model_full_name.to_string();
    let mut failed_model_ids: Vec<String> = Vec::new();
    let mut retry_count = 0;
    const MAX_RETRIES: usize = 2;

    let window_shown = Arc::new(Mutex::new(block.block_type != "image"));
    let processing_hwnd_arc = Arc::new(Mutex::new(processing_hwnd_shared));

    // Retry loop
    let res = loop {
        let acc_clone = accumulated.clone();
        let window_shown_clone = window_shown.clone();
        let processing_hwnd_clone = processing_hwnd_arc.clone();

        if retry_count > 0
            && let Ok(mut lock) = acc_clone.lock()
        {
            lock.clear();
        }

        let res_inner = if is_first_processing_block
            && block.block_type == "image"
            && matches!(context, RefineContext::Image(_))
        {
            execute_image_block(ExecuteImageBlockRequest {
                context,
                groq_key: &groq_key,
                gemini_key: &gemini_key,
                final_prompt,
                model_full_name: &current_model_full_name,
                provider: &current_provider,
                streaming_enabled: actual_streaming_enabled,
                use_json,
                accumulated: acc_clone,
                my_hwnd,
                window_shown: window_shown_clone,
                processing_hwnd: processing_hwnd_clone,
                cancel_token,
            })
        } else {
            execute_text_block(ExecuteTextBlockRequest {
                input_text,
                groq_key: &groq_key,
                gemini_key: &gemini_key,
                final_prompt,
                model_full_name: &current_model_full_name,
                provider: &current_provider,
                streaming_enabled: actual_streaming_enabled,
                preset_id,
                config,
                accumulated: acc_clone,
                my_hwnd,
                cancel_token,
            })
        };

        match res_inner {
            Ok(val) => break Ok(val),
            Err(e) => {
                // Never retry after explicit user cancellation.
                if cancel_token.is_cancelled() {
                    break Err(e);
                }

                if retry_count < MAX_RETRIES
                    && crate::overlay::utils::is_retryable_error(&e.to_string())
                {
                    retry_count += 1;
                    failed_model_ids.push(current_model_id.clone());

                    let current_type = if block.block_type == "image" {
                        crate::model_config::ModelType::Vision
                    } else {
                        crate::model_config::ModelType::Text
                    };

                    if let Some(next_model) = crate::model_config::resolve_fallback_model(
                        &current_model_id,
                        &failed_model_ids,
                        &current_type,
                        config,
                    ) {
                        current_model_id = next_model.id;
                        current_provider = next_model.provider;
                        current_model_full_name = next_model.full_name;

                        if let Some(h) = my_hwnd {
                            let retry_msg =
                                get_retry_message(&config.ui_language, &current_model_full_name);
                            update_window_text(h, &retry_msg);
                        }
                        continue;
                    }
                }
                break Err(e);
            }
        }
    };

    // Handle result
    handle_execution_result(
        res,
        my_hwnd,
        &window_shown,
        &processing_hwnd_arc,
        config,
        &current_model_full_name,
    )
}

/// Execute an image processing block.
struct ExecuteImageBlockRequest<'a> {
    context: &'a RefineContext,
    groq_key: &'a str,
    gemini_key: &'a str,
    final_prompt: &'a str,
    model_full_name: &'a str,
    provider: &'a str,
    streaming_enabled: bool,
    use_json: bool,
    accumulated: Arc<Mutex<String>>,
    my_hwnd: Option<HWND>,
    window_shown: Arc<Mutex<bool>>,
    processing_hwnd: Arc<Mutex<Option<SendHwnd>>>,
    cancel_token: &'a Arc<ChainCancelToken>,
}

fn execute_image_block(request: ExecuteImageBlockRequest<'_>) -> anyhow::Result<String> {
    let ExecuteImageBlockRequest {
        context,
        groq_key,
        gemini_key,
        final_prompt,
        model_full_name,
        provider,
        streaming_enabled,
        use_json,
        accumulated,
        my_hwnd,
        window_shown,
        processing_hwnd,
        cancel_token,
    } = request;
    if let RefineContext::Image(img_data) = context {
        let img = image::load_from_memory(img_data)
            .expect("Failed to load png")
            .to_rgba8();

        // Bridge: chain token → API-level AtomicBool
        let api_cancel = Arc::new(AtomicBool::new(false));
        let api_cancel_cb = api_cancel.clone();
        let chain_token_cb = cancel_token.clone();

        translate_image_streaming(
            TranslateImageRequest {
                groq_api_key: groq_key,
                gemini_api_key: gemini_key,
                prompt: final_prompt.to_string(),
                model: model_full_name.to_string(),
                provider: provider.to_string(),
                image: img,
                original_bytes: Some(img_data.clone()),
                streaming_enabled,
                use_json_format: use_json,
                cancel_token: Some(api_cancel),
            },
            move |chunk| {
                if chain_token_cb.is_cancelled() {
                    api_cancel_cb.store(true, Ordering::SeqCst);
                    return;
                }
                handle_streaming_chunk(
                    chunk,
                    &accumulated,
                    my_hwnd,
                    &window_shown,
                    &processing_hwnd,
                );
            },
        )
    } else {
        Err(anyhow::anyhow!("Missing image context"))
    }
}

/// Execute a text processing block.
struct ExecuteTextBlockRequest<'a> {
    input_text: &'a str,
    groq_key: &'a str,
    gemini_key: &'a str,
    final_prompt: &'a str,
    model_full_name: &'a str,
    provider: &'a str,
    streaming_enabled: bool,
    preset_id: &'a str,
    config: &'a Config,
    accumulated: Arc<Mutex<String>>,
    my_hwnd: Option<HWND>,
    cancel_token: &'a Arc<ChainCancelToken>,
}

fn execute_text_block(request: ExecuteTextBlockRequest<'_>) -> anyhow::Result<String> {
    let ExecuteTextBlockRequest {
        input_text,
        groq_key,
        gemini_key,
        final_prompt,
        model_full_name,
        provider,
        streaming_enabled,
        preset_id,
        config,
        accumulated,
        my_hwnd,
        cancel_token,
    } = request;
    let search_label = Some(get_localized_preset_name(preset_id, &config.ui_language));

    // Bridge: chain token → API-level AtomicBool
    let api_cancel = Arc::new(AtomicBool::new(false));
    let api_cancel_cb = api_cancel.clone();
    let chain_token_cb = cancel_token.clone();

    translate_text_streaming(
        TranslateTextRequest {
            groq_api_key: groq_key,
            gemini_api_key: gemini_key,
            text: input_text.to_string(),
            instruction: final_prompt.to_string(),
            model: model_full_name.to_string(),
            provider: provider.to_string(),
            streaming_enabled,
            use_json_format: false,
            search_label,
            ui_language: &config.ui_language,
            cancel_token: Some(api_cancel),
        },
        move |chunk| {
            if chain_token_cb.is_cancelled() {
                api_cancel_cb.store(true, Ordering::SeqCst);
                return;
            }

            let mut t = accumulated.lock().unwrap();
            if let Some(wiped) = chunk.strip_prefix(crate::api::WIPE_SIGNAL) {
                t.clear();
                t.push_str(wiped);
            } else {
                t.push_str(chunk);
            }

            if let Some(h) = my_hwnd {
                {
                    let mut s = WINDOW_STATES.lock().unwrap();
                    if let Some(st) = s.get_mut(&(h.0 as isize)) {
                        st.is_refining = false;
                        st.font_cache_dirty = true;
                    }
                }
                update_window_text(h, &t);
            }
        },
    )
}

/// Handle a streaming chunk for image blocks.
fn handle_streaming_chunk(
    chunk: &str,
    accumulated: &Arc<Mutex<String>>,
    my_hwnd: Option<HWND>,
    window_shown: &Arc<Mutex<bool>>,
    processing_hwnd: &Arc<Mutex<Option<SendHwnd>>>,
) {
    let mut t = accumulated.lock().unwrap();
    if let Some(wiped) = chunk.strip_prefix(crate::api::WIPE_SIGNAL) {
        t.clear();
        t.push_str(wiped);
    } else {
        t.push_str(chunk);
    }

    if let Some(h) = my_hwnd {
        // Show window on first chunk for image blocks
        {
            let mut shown = window_shown.lock().unwrap();
            if !*shown {
                *shown = true;
                unsafe {
                    let _ = ShowWindow(h, SW_SHOW);
                }
                let mut proc_hwnd = processing_hwnd.lock().unwrap();
                if let Some(ph) = proc_hwnd.take() {
                    unsafe {
                        let _ = PostMessageW(Some(ph.0), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                }
            }
        }
        {
            let mut s = WINDOW_STATES.lock().unwrap();
            if let Some(st) = s.get_mut(&(h.0 as isize)) {
                st.is_refining = false;
                st.font_cache_dirty = true;
            }
        }
        update_window_text(h, &t);
    }
}

/// Get localized retry message.
fn get_retry_message(lang: &str, model_name: &str) -> String {
    match lang {
        "vi" => format!("(Đang thử lại {}...)", model_name),
        "ko" => format!("({} 재시도 중...)", model_name),
        "ja" => format!("({} 再試行中...)", model_name),
        "zh" => format!("(正在重试 {}...)", model_name),
        _ => format!("(Retrying {}...)", model_name),
    }
}

/// Handle the execution result (success or error).
fn handle_execution_result(
    res: anyhow::Result<String>,
    my_hwnd: Option<HWND>,
    window_shown: &Arc<Mutex<bool>>,
    processing_hwnd_arc: &Arc<Mutex<Option<SendHwnd>>>,
    config: &Config,
    model_full_name: &str,
) -> String {
    match res {
        Ok(txt) => {
            if let Some(h) = my_hwnd {
                let mut s = WINDOW_STATES.lock().unwrap();
                if let Some(st) = s.get_mut(&(h.0 as isize)) {
                    st.is_refining = false;
                    st.is_streaming_active = false;
                    st.font_cache_dirty = true;
                    st.pending_text = Some(txt.clone());
                    st.full_text = txt.clone();
                }
            }
            txt
        }
        Err(e) => {
            let err = crate::overlay::utils::get_error_message(
                &e.to_string(),
                &config.ui_language,
                Some(model_full_name),
            );
            if let Some(h) = my_hwnd {
                // Show window if hidden (image blocks)
                {
                    let mut shown = window_shown.lock().unwrap();
                    if !*shown {
                        *shown = true;
                        unsafe {
                            let _ = ShowWindow(h, SW_SHOW);
                        }
                        let mut proc_hwnd = processing_hwnd_arc.lock().unwrap();
                        if let Some(ph) = proc_hwnd.take() {
                            unsafe {
                                let _ = PostMessageW(Some(ph.0), WM_CLOSE, WPARAM(0), LPARAM(0));
                            }
                        }
                    }
                }
                let mut s = WINDOW_STATES.lock().unwrap();
                if let Some(st) = s.get_mut(&(h.0 as isize)) {
                    st.is_refining = false;
                    st.is_streaming_active = false;
                    st.font_cache_dirty = true;
                    st.pending_text = Some(err.clone());
                    st.full_text = err.clone();
                }
            }
            String::new()
        }
    }
}
