// --- CHAIN BLOCK EXECUTION ---
// API execution with retry logic for chain processing blocks.

use crate::api::{translate_image_streaming, translate_text_streaming};
use crate::config::{Config, ProcessingBlock};
use crate::gui::settings_ui::get_localized_preset_name;
use crate::overlay::result::{update_window_text, ChainCancelToken, RefineContext, WINDOW_STATES};
use crate::win_types::SendHwnd;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Execute the block's API call and return the result text.
#[allow(clippy::too_many_arguments)]
pub fn execute_block(
    block: &ProcessingBlock,
    block_idx: usize,
    blocks: &[ProcessingBlock],
    my_hwnd: Option<HWND>,
    input_text: &str,
    context: &RefineContext,
    model_id: &str,
    provider: &str,
    model_full_name: &str,
    final_prompt: &str,
    skip_execution: bool,
    config: &Config,
    preset_id: &str,
    processing_hwnd_shared: Option<SendHwnd>,
    cancel_token: &Arc<ChainCancelToken>,
) -> String {
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

        if retry_count > 0 {
            if let Ok(mut lock) = acc_clone.lock() {
                lock.clear();
            }
        }

        let res_inner = if is_first_processing_block
            && block.block_type == "image"
            && matches!(context, RefineContext::Image(_))
        {
            execute_image_block(
                context,
                &groq_key,
                &gemini_key,
                final_prompt,
                &current_model_full_name,
                &current_provider,
                actual_streaming_enabled,
                use_json,
                acc_clone,
                my_hwnd,
                window_shown_clone,
                processing_hwnd_clone,
                cancel_token,
            )
        } else {
            execute_text_block(
                input_text,
                &groq_key,
                &gemini_key,
                final_prompt,
                &current_model_full_name,
                &current_provider,
                actual_streaming_enabled,
                preset_id,
                config,
                acc_clone,
                my_hwnd,
                cancel_token,
            )
        };

        match res_inner {
            Ok(val) => break Ok(val),
            Err(e) => {
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
                            let retry_msg = get_retry_message(&config.ui_language, &current_model_full_name);
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
    handle_execution_result(res, my_hwnd, &window_shown, &processing_hwnd_arc, config, &current_model_full_name)
}

/// Execute an image processing block.
#[allow(clippy::too_many_arguments)]
fn execute_image_block(
    context: &RefineContext,
    groq_key: &str,
    gemini_key: &str,
    final_prompt: &str,
    model_full_name: &str,
    provider: &str,
    streaming_enabled: bool,
    use_json: bool,
    accumulated: Arc<Mutex<String>>,
    my_hwnd: Option<HWND>,
    window_shown: Arc<Mutex<bool>>,
    processing_hwnd: Arc<Mutex<Option<SendHwnd>>>,
    cancel_token: &Arc<ChainCancelToken>,
) -> anyhow::Result<String> {
    if let RefineContext::Image(img_data) = context {
        let img = image::load_from_memory(img_data)
            .expect("Failed to load png")
            .to_rgba8();

        // Bridge: chain token → API-level AtomicBool
        let api_cancel = Arc::new(AtomicBool::new(false));
        let api_cancel_cb = api_cancel.clone();
        let chain_token_cb = cancel_token.clone();

        translate_image_streaming(
            groq_key,
            gemini_key,
            final_prompt.to_string(),
            model_full_name.to_string(),
            provider.to_string(),
            img,
            Some(img_data.clone()),
            streaming_enabled,
            use_json,
            Some(api_cancel),
            move |chunk| {
                if chain_token_cb.is_cancelled() {
                    api_cancel_cb.store(true, Ordering::SeqCst);
                    return;
                }
                handle_streaming_chunk(chunk, &accumulated, my_hwnd, &window_shown, &processing_hwnd);
            },
        )
    } else {
        Err(anyhow::anyhow!("Missing image context"))
    }
}

/// Execute a text processing block.
#[allow(clippy::too_many_arguments)]
fn execute_text_block(
    input_text: &str,
    groq_key: &str,
    gemini_key: &str,
    final_prompt: &str,
    model_full_name: &str,
    provider: &str,
    streaming_enabled: bool,
    preset_id: &str,
    config: &Config,
    accumulated: Arc<Mutex<String>>,
    my_hwnd: Option<HWND>,
    cancel_token: &Arc<ChainCancelToken>,
) -> anyhow::Result<String> {
    let search_label = Some(get_localized_preset_name(preset_id, &config.ui_language));

    // Bridge: chain token → API-level AtomicBool
    let api_cancel = Arc::new(AtomicBool::new(false));
    let api_cancel_cb = api_cancel.clone();
    let chain_token_cb = cancel_token.clone();

    translate_text_streaming(
        groq_key,
        gemini_key,
        input_text.to_string(),
        final_prompt.to_string(),
        model_full_name.to_string(),
        provider.to_string(),
        streaming_enabled,
        false,
        search_label,
        &config.ui_language,
        Some(api_cancel),
        move |chunk| {
            if chain_token_cb.is_cancelled() {
                api_cancel_cb.store(true, Ordering::SeqCst);
                return;
            }

            let mut t = accumulated.lock().unwrap();
            if chunk.starts_with(crate::api::WIPE_SIGNAL) {
                t.clear();
                t.push_str(&chunk[crate::api::WIPE_SIGNAL.len()..]);
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
    if chunk.starts_with(crate::api::WIPE_SIGNAL) {
        t.clear();
        t.push_str(&chunk[crate::api::WIPE_SIGNAL.len()..]);
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
