// --- CHAIN BLOCK EXECUTION ---
// API execution with retry logic for chain processing blocks.

use crate::api::{
    TranslateImageRequest, TranslateTextRequest, translate_image_streaming,
    translate_text_streaming,
};
use crate::config::{Config, ProcessingBlock};
use crate::gui::settings_ui::get_localized_preset_name;
use crate::overlay::result::{ChainCancelToken, RefineContext, WINDOW_STATES, update_window_text};
use crate::retry_model_chain::{
    InteractiveRequestWorkload, RetryChainKind, claim_model_attempt, interactive_request_timeout,
    preflight_skip_reason, record_model_failure, record_model_success, release_model_probe,
    resolve_next_retry_model,
};
use crate::win_types::SendHwnd;
use std::collections::HashSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::visibility::claim_result_reveal;

const MAX_INTERACTIVE_PROVIDER_ATTEMPTS: usize = 2;

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

    let surface_streaming_enabled = block.streaming_enabled && block.render_mode != "markdown";

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
    let mut blocked_providers: HashSet<String> = HashSet::new();
    let mut provider_attempts = 0;
    let retry_chain_kind = RetryChainKind::from_block_type(&block.block_type)
        .filter(|_| !crate::model_config::model_is_non_llm(model_id));

    // Area of the image this block would send, where there is one. An endpoint
    // that declares a reliable floor is passed over below it, and the chain moves
    // to the next model on its own.
    let input_pixels = match context {
        RefineContext::Image(bytes) => crate::image_decode::load_from_memory(bytes)
            .ok()
            .map(|image| image.width().saturating_mul(image.height())),
        _ => None,
    };
    let encoded_media_bytes = match context {
        RefineContext::Image(bytes) => (bytes.len() as u64).saturating_mul(4).saturating_add(2) / 3,
        _ => 0,
    };
    let workload = InteractiveRequestWorkload {
        encoded_request_bytes: (final_prompt.len() as u64)
            .saturating_add(input_text.len() as u64)
            .saturating_add(encoded_media_bytes),
    };

    let window_shown = Arc::new(Mutex::new(block.block_type != "image"));
    let processing_hwnd_arc = Arc::new(Mutex::new(processing_hwnd_shared));

    // Retry loop
    let res = loop {
        let acc_clone = accumulated.clone();
        let window_shown_clone = window_shown.clone();
        let processing_hwnd_clone = processing_hwnd_arc.clone();

        if !failed_model_ids.is_empty()
            && let Ok(mut lock) = acc_clone.lock()
        {
            lock.clear();
        }

        if let Some(chain_kind) = retry_chain_kind
            && let Some(skip_reason) = preflight_skip_reason(
                &current_model_id,
                &current_provider,
                config,
                &blocked_providers,
                input_pixels,
            )
            .or_else(|| claim_model_attempt(&current_model_id))
        {
            if crate::overlay::utils::should_block_retry_provider(&skip_reason) {
                blocked_providers.insert(current_provider.clone());
            }

            failed_model_ids.push(current_model_id.clone());

            if let Some(next_model) = resolve_next_retry_model(
                &current_model_id,
                &failed_model_ids,
                &blocked_providers,
                chain_kind,
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

            break Err(anyhow::anyhow!(skip_reason));
        }

        provider_attempts += 1;
        let transport_streaming_enabled = crate::api::endpoint_supports_progress_streaming(
            &current_provider,
            &current_model_full_name,
        );
        let request_timeout = interactive_request_timeout(
            &current_model_id,
            config,
            transport_streaming_enabled,
            workload,
        );
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
                streaming_enabled: transport_streaming_enabled,
                request_timeout,
                accumulated: acc_clone,
                my_hwnd: my_hwnd.filter(|_| surface_streaming_enabled),
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
                target_language: gtx_target_language(block),
                model_full_name: &current_model_full_name,
                provider: &current_provider,
                streaming_enabled: transport_streaming_enabled,
                request_timeout,
                preset_id,
                config,
                accumulated: acc_clone,
                my_hwnd: my_hwnd.filter(|_| surface_streaming_enabled),
                cancel_token,
            })
        };

        match res_inner {
            Ok(val) => {
                record_model_success(&current_model_id);
                break Ok(val);
            }
            Err(e) => {
                // Never retry after explicit user cancellation.
                if cancel_token.is_cancelled() {
                    release_model_probe(&current_model_id);
                    break Err(e);
                }

                record_model_failure(&current_model_id, &e.to_string());

                if may_retry_provider(provider_attempts)
                    && let Some(chain_kind) = retry_chain_kind
                    && crate::overlay::utils::should_advance_retry_chain(&e.to_string())
                {
                    if crate::overlay::utils::should_block_retry_provider(&e.to_string()) {
                        blocked_providers.insert(current_provider.clone());
                    }

                    failed_model_ids.push(current_model_id.clone());

                    if let Some(next_model) = resolve_next_retry_model(
                        &current_model_id,
                        &failed_model_ids,
                        &blocked_providers,
                        chain_kind,
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
    request_timeout: Option<Duration>,
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
        request_timeout,
        accumulated,
        my_hwnd,
        window_shown,
        processing_hwnd,
        cancel_token,
    } = request;
    if let RefineContext::Image(img_data) = context {
        let img = crate::image_decode::load_from_memory(img_data)
            .expect("Failed to load image")
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
                response_schema: None,
                cancel_token: Some(api_cancel),
                request_timeout,
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
    target_language: Option<String>,
    model_full_name: &'a str,
    provider: &'a str,
    streaming_enabled: bool,
    request_timeout: Option<Duration>,
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
        target_language,
        model_full_name,
        provider,
        streaming_enabled,
        request_timeout,
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
            response_schema: None,
            search_label,
            ui_language: &config.ui_language,
            cancel_token: Some(api_cancel),
            request_timeout,
            target_language,
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
                crate::overlay::result::latency::mark_window(h, "provider_first_output");
                {
                    let mut s = WINDOW_STATES.lock().unwrap();
                    if let Some(st) = s.get_mut(&(h.0 as isize)) {
                        st.is_refining = false;
                    }
                }
                update_window_text(h, &t);
            }
        },
    )
}

fn may_retry_provider(completed_attempts: usize) -> bool {
    completed_attempts < MAX_INTERACTIVE_PROVIDER_ATTEMPTS
}

fn gtx_target_language(block: &ProcessingBlock) -> Option<String> {
    block
        .language_vars
        .get("language1")
        .or_else(|| block.language_vars.get("language"))
        .or_else(|| {
            if block.selected_language.trim().is_empty() {
                None
            } else {
                Some(&block.selected_language)
            }
        })
        .map(|lang| lang.trim().to_string())
        .filter(|lang| !lang.is_empty())
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
        crate::overlay::result::latency::mark_window(h, "provider_first_output");
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
                crate::overlay::result::latency::mark_window(h, "provider_first_output");
                {
                    let mut s = WINDOW_STATES.lock().unwrap();
                    if let Some(st) = s.get_mut(&(h.0 as isize)) {
                        st.is_refining = false;
                        st.is_streaming_active = false;
                    }
                }
                update_window_text(h, &txt);
                if claim_result_reveal(window_shown) {
                    unsafe {
                        let _ = ShowWindow(h, SW_SHOW);
                    }
                }
            }
            txt
        }
        Err(e) => {
            crate::overlay::utils::show_api_key_error_notification(
                &e.to_string(),
                &config.ui_language,
            );
            let err = crate::overlay::utils::get_error_message(
                &e.to_string(),
                &config.ui_language,
                Some(model_full_name),
            );
            if let Some(h) = my_hwnd {
                crate::overlay::result::latency::mark_window(h, "provider_first_output");
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
                {
                    let mut s = WINDOW_STATES.lock().unwrap();
                    if let Some(st) = s.get_mut(&(h.0 as isize)) {
                        st.is_refining = false;
                        st.is_streaming_active = false;
                    }
                }
                update_window_text(h, &err);
            }
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::may_retry_provider;

    #[test]
    fn interactive_retry_is_limited_to_one_fallback() {
        assert!(may_retry_provider(1));
        assert!(!may_retry_provider(2));
    }
}
