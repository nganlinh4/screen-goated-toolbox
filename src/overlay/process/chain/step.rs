// --- CHAIN STEP EXECUTION ---
// Core logic for running a single step in the processing chain.

use crate::config::{Config, ProcessingBlock};
use crate::overlay::result::{
    ChainCancelToken, RefineContext, ResultWindowParams, WINDOW_STATES, WindowType,
    create_result_window_shell, get_chain_color, initialize_result_window, link_windows,
};
use crate::win_types::SendHwnd;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::execution::{ExecuteBlockRequest, execute_block};
use super::post_process::{continue_chain, handle_auto_copy, handle_auto_speak, save_to_history};
use super::templates::{generate_audio_player_html, generate_image_display_html};
use crate::overlay::process::types::get_next_window_position_for_chain;

pub struct ChainStepRequest {
    pub block_idx: usize,
    pub input_text: String,
    pub current_rect: RECT,
    pub blocks: Vec<ProcessingBlock>,
    pub connections: Vec<(usize, usize)>,
    pub graph_explicit: bool,
    pub config: Config,
    pub parent_hwnd: Arc<Mutex<Option<SendHwnd>>>,
    pub context: RefineContext,
    pub skip_execution: bool,
    pub preprocessed_audio: Option<(usize, String)>,
    pub visited_blocks: Vec<usize>,
    pub processing_indicator_hwnd: Option<super::ProcessingIndicator>,
    pub cancel_token: Arc<ChainCancelToken>,
    pub preset_id: String,
    pub disable_auto_paste: bool,
    pub(crate) auto_paste_destination: super::AutoPasteDestination,
    pub chain_id: String,
    pub input_hwnd_refocus: Option<(SendHwnd, u64)>,
}

/// Recursive step to run a single block in the chain.
pub fn run_chain_step(request: ChainStepRequest) {
    let ChainStepRequest {
        block_idx,
        mut input_text,
        current_rect,
        blocks,
        connections,
        graph_explicit,
        config,
        parent_hwnd,
        context,
        skip_execution,
        preprocessed_audio,
        mut visited_blocks,
        mut processing_indicator_hwnd,
        cancel_token,
        preset_id,
        disable_auto_paste,
        auto_paste_destination,
        chain_id,
        input_hwnd_refocus,
    } = request;
    // Check if cancelled before starting
    if cancel_token.is_cancelled() {
        if let Some(h) = processing_indicator_hwnd {
            h.close();
        }
        return;
    }

    if block_idx >= blocks.len() || visited_blocks.contains(&block_idx) {
        if let Some(h) = processing_indicator_hwnd {
            h.close();
        }
        return;
    }
    visited_blocks.push(block_idx);

    #[cfg(test)]
    traversal_tests::record_visit(&chain_id, block_idx, auto_paste_destination);

    // Clone the block to avoid borrowing issues when passing blocks to continue_chain
    let block = blocks[block_idx].clone();
    let preprocessed_text = super::audio_route::preprocessed_text(block_idx, &preprocessed_audio);
    let skip_execution = skip_execution || preprocessed_text.is_some();
    if let Some(text) = preprocessed_text {
        input_text = text.to_string();
    }
    let trace_id = format!("{chain_id}:{block_idx}");
    crate::overlay::result::latency::begin(&trace_id);

    // 1. Resolve Model & Prompt
    //
    // Discovery may not have completed yet. Never send input to another model
    // merely because the selected model is currently unavailable.
    let block_model_id = block.model.clone();
    let resolved = resolve_block_model(&block, &config, skip_execution);
    let unsupported_image_search = block.block_type == "image" && block.search_enabled;
    let implicit_search = !block.search_enabled
        && resolved
            .as_ref()
            .is_some_and(|model| model.search_tool_enabled_by_default);
    if !skip_execution
        && (unsupported_image_search
            || implicit_search
            || (matches!(block.block_type.as_str(), "image" | "text" | "audio")
                && resolved.is_none()))
    {
        if let Some(h) = processing_indicator_hwnd {
            h.close();
        }
        let message = match (
            config.ui_language.as_str(),
            unsupported_image_search,
            implicit_search,
        ) {
            ("vi", true, _) => "Tìm kiếm không được hỗ trợ cho hình ảnh.".to_string(),
            ("ko", true, _) => "이미지 검색은 지원되지 않습니다.".to_string(),
            (_, true, _) => "Search is not supported for image requests.".to_string(),
            ("vi", _, true) => "Mô hình này cần bật tìm kiếm.".to_string(),
            ("ko", _, true) => "이 모델을 사용하려면 검색을 활성화해야 합니다.".to_string(),
            (_, _, true) => "This model requires search to be enabled.".to_string(),
            ("vi", _, _) => format!("Mô hình không khả dụng: {block_model_id}"),
            ("ko", _, _) => format!("모델을 사용할 수 없습니다: {block_model_id}"),
            (_, _, _) => format!("Model unavailable: {block_model_id}"),
        };
        crate::overlay::auto_copy_badge::show_error_notification(&message);
        return;
    }
    let model_id = resolved
        .as_ref()
        .map_or_else(|| block_model_id.clone(), |model| model.id.clone());
    let model_conf = resolved;
    let provider = model_conf
        .clone()
        .map(|m| m.provider)
        .unwrap_or_else(|| "groq".to_string());
    let model_full_name = model_conf.map(|m| m.full_name).unwrap_or(model_id.clone());

    let mut final_prompt = block.prompt.clone();
    for (key, value) in &block.language_vars {
        final_prompt = final_prompt.replace(&format!("{{{}}}", key), value);
    }
    if final_prompt.contains("{language1}") && !block.language_vars.contains_key("language1") {
        final_prompt = final_prompt.replace("{language1}", &block.selected_language);
    }
    final_prompt = final_prompt.replace("{language}", &block.selected_language);

    // 2. Determine Visibility & Position
    let visible_count_before = blocks
        .iter()
        .take(block_idx)
        .filter(|b| b.show_overlay)
        .count();
    let bg_color = get_chain_color(visible_count_before);

    // PERSISTENCE: Check if the preset has a saved geometry (only for first block)
    let mut starting_rect = current_rect;
    if block_idx == 0
        && let Ok(app) = crate::APP.lock()
    {
        let found_preset = app.config.presets.iter().find(|p| p.id == preset_id);
        if let Some(p) = found_preset {
            let is_image_category = p.preset_type == "image";
            if !is_image_category && let Some(geom) = &p.window_geometry {
                starting_rect = RECT {
                    left: geom.x,
                    top: geom.y,
                    right: geom.x + geom.width,
                    bottom: geom.y + geom.height,
                };
            }
        }
    }

    let my_rect = if block.show_overlay {
        get_next_window_position_for_chain(&chain_id, starting_rect)
    } else {
        starting_rect
    };

    let mut my_hwnd: Option<HWND> = None;

    let is_first_processing_block =
        super::path_state::first_processing_in_path(&blocks, &visited_blocks);

    // 3. Create Window (if visible)
    let should_create_window = block.show_overlay;

    if block.block_type == "input_adapter" && !block.show_overlay {
        // Input adapter without overlay - invisible pass-through
    } else if should_create_window {
        let (created_hwnd, new_processing_hwnd) = create_block_window(CreateBlockWindowRequest {
            block: &block,
            block_idx,
            is_first_processing_block,
            my_rect,
            context: &context,
            input_text: &input_text,
            model_id: &model_id,
            provider: &provider,
            final_prompt: &final_prompt,
            bg_color,
            visible_count_before,
            skip_execution,
            parent_hwnd: &parent_hwnd,
            cancel_token: &cancel_token,
            preset_id: &preset_id,
            chain_id: &chain_id,
            config: &config,
            processing_indicator_hwnd,
            input_hwnd_refocus,
            trace_id: &trace_id,
        });
        my_hwnd = created_hwnd;
        processing_indicator_hwnd = new_processing_hwnd;
    }

    // 4. Execution (API Call)
    let input_text_for_history = input_text.clone();
    crate::overlay::result::latency::mark(&trace_id, "provider_dispatch");
    let result_text = execute_block(ExecuteBlockRequest {
        block: &block,
        block_idx,
        is_first_processing_block,
        my_hwnd,
        input_text: &input_text,
        context: &context,
        model_id: &model_id,
        provider: &provider,
        model_full_name: &model_full_name,
        final_prompt: &final_prompt,
        skip_execution,
        config: &config,
        preset_id: &preset_id,
        processing_hwnd_shared: processing_indicator_hwnd,
        cancel_token: &cancel_token,
    });
    crate::overlay::result::latency::mark(&trace_id, "provider_complete");

    let Some(result_text) = result_text else {
        if let Some(h) = processing_indicator_hwnd {
            h.close();
        }
        return;
    };

    // Check cancellation after execution — skip post-processing and chain continuation
    if cancel_token.is_cancelled() {
        if let Some(h) = processing_indicator_hwnd {
            h.close();
        }
        return;
    }

    // 5. Post-Processing
    handle_auto_copy(
        &block,
        &result_text,
        &context,
        &preset_id,
        disable_auto_paste,
        auto_paste_destination,
        &cancel_token,
    );
    handle_auto_speak(&block, &result_text, my_hwnd, &cancel_token);
    if block.block_type == "audio"
        && !skip_execution
        && let RefineContext::Audio(wav_data) = &context
        && let Ok(app) = crate::APP.lock()
    {
        app.history.save_audio_shared_cancellable(
            wav_data.clone(),
            result_text.clone(),
            cancel_token.clone(),
        );
    }
    save_to_history(
        &block,
        &result_text,
        &input_text_for_history,
        &context,
        &cancel_token,
    );

    // 6. Chain Next Steps
    continue_chain(super::post_process::ContinueChainRequest {
        block_idx,
        block: &block,
        result_text,
        blocks,
        connections,
        graph_explicit,
        config,
        my_hwnd,
        parent_hwnd,
        context,
        skip_execution,
        preprocessed_audio,
        visited_blocks,
        processing_indicator_hwnd,
        cancel_token,
        preset_id,
        disable_auto_paste,
        auto_paste_destination,
        chain_id,
        input_hwnd_refocus,
        my_rect,
        starting_rect,
    });
}

#[cfg(test)]
#[path = "traversal_tests.rs"]
mod traversal_tests;

fn resolve_block_model(
    block: &ProcessingBlock,
    config: &Config,
    already_produced: bool,
) -> Option<crate::model_config::ModelConfig> {
    let expected = match block.block_type.as_str() {
        "image" => crate::model_config::ModelType::Vision,
        "text" => crate::model_config::ModelType::Text,
        "audio" => crate::model_config::ModelType::Audio,
        _ => return None,
    };
    crate::model_config::get_model_by_id_with_custom(&block.model, &config.custom_models)
        .filter(|model| model.model_type == expected && (already_produced || model.enabled))
}

/// Create window for a block and return (hwnd, updated processing_indicator_hwnd).
struct CreateBlockWindowRequest<'a> {
    block: &'a ProcessingBlock,
    block_idx: usize,
    is_first_processing_block: bool,
    my_rect: RECT,
    context: &'a RefineContext,
    input_text: &'a str,
    model_id: &'a str,
    provider: &'a str,
    final_prompt: &'a str,
    bg_color: u32,
    visible_count_before: usize,
    skip_execution: bool,
    parent_hwnd: &'a Arc<Mutex<Option<SendHwnd>>>,
    cancel_token: &'a Arc<ChainCancelToken>,
    preset_id: &'a str,
    chain_id: &'a str,
    config: &'a Config,
    processing_indicator_hwnd: Option<super::ProcessingIndicator>,
    input_hwnd_refocus: Option<(SendHwnd, u64)>,
    trace_id: &'a str,
}

fn create_block_window(
    request: CreateBlockWindowRequest<'_>,
) -> (Option<HWND>, Option<super::ProcessingIndicator>) {
    let CreateBlockWindowRequest {
        block,
        block_idx,
        is_first_processing_block,
        my_rect,
        context,
        input_text,
        model_id,
        provider,
        final_prompt,
        bg_color,
        visible_count_before,
        skip_execution,
        parent_hwnd,
        cancel_token,
        preset_id,
        chain_id,
        config,
        mut processing_indicator_hwnd,
        input_hwnd_refocus,
        trace_id,
    } = request;
    let ctx_clone = if super::path_state::retain_result_context(
        &block.block_type,
        block_idx,
        is_first_processing_block,
        context,
    ) {
        context.clone()
    } else {
        RefineContext::None
    };

    let stream_en = block.streaming_enabled && !skip_execution;
    let is_image_block = block.block_type == "image";
    let is_input_adapter_image =
        block.block_type == "input_adapter" && matches!(context, RefineContext::Image(_));

    let locale = crate::gui::locale::LocaleText::get(&config.ui_language);

    // Generate initial content
    let initial_content = if block.block_type == "input_adapter" {
        match context {
            RefineContext::Image(img_data) => generate_image_display_html(img_data),
            RefineContext::Audio(wav_data) => generate_audio_player_html(wav_data, &locale),
            RefineContext::None => input_text.to_string(),
        }
    } else {
        String::new()
    };
    let initial_content_clone = initial_content.clone();

    let parent_clone = parent_hwnd.clone();
    let cancel_token_thread = cancel_token.clone();
    let chain_id_thread = chain_id.to_string();
    let input_hwnd_refocus_thread = input_hwnd_refocus;
    let preset_id_for_window = preset_id.to_string();
    let trace_id_for_window = trace_id.to_string();
    let m_id = model_id.to_string();
    let prov = provider.to_string();
    let prompt_c = final_prompt.to_string();

    let (tx_hwnd, rx_hwnd) = std::sync::mpsc::channel();
    crate::overlay::result::latency::mark(trace_id, "window_thread_queued");

    std::thread::spawn(move || {
        crate::overlay::result::latency::mark(&trace_id_for_window, "window_thread_started");
        let is_root = visible_count_before == 0;

        let hwnd = create_result_window_shell(ResultWindowParams {
            target_rect: my_rect,
            win_type: WindowType::Primary,
            context: ctx_clone,
            model_id: m_id,
            provider: prov,
            streaming_enabled: stream_en,
            start_editing: false,
            preset_prompt: prompt_c,
            custom_bg_color: bg_color,
            initial_text: initial_content_clone,
            preset_id: Some(preset_id_for_window),
            is_chain_root: is_root,
            latency_trace_id: Some(trace_id_for_window.clone()),
        });

        // Assign cancellation token and chain_id immediately
        {
            let mut s = WINDOW_STATES.lock().unwrap();
            if let Some(st) = s.get_mut(&(hwnd.0 as isize)) {
                st.cancellation_token = Some(cancel_token_thread.clone());
                st.chain_id = Some(chain_id_thread.clone());
            }
        }

        if let Ok(p_guard) = parent_clone.lock()
            && let Some(ph) = *p_guard
        {
            link_windows(ph.0, hwnd);
        }

        let _ = tx_hwnd.send(SendHwnd(hwnd));
        crate::overlay::result::latency::mark(&trace_id_for_window, "window_published");
        initialize_result_window(hwnd);
        crate::overlay::result::latency::mark(&trace_id_for_window, "renderer_registered");

        if !is_image_block {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNA);
                if let Some((h_input, session)) = input_hwnd_refocus_thread {
                    crate::overlay::text_input::request_focus_for_session(
                        h_input,
                        session,
                        cancel_token_thread.clone(),
                    );
                }
            }
        }
        unsafe {
            if is_input_adapter_image {
                use windows::Win32::UI::WindowsAndMessaging::{
                    LWA_ALPHA, SetLayeredWindowAttributes,
                };
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
            }

            let mut m = MSG::default();
            while GetMessageW(&mut m, None, 0, 0).into() {
                let _ = TranslateMessage(&m);
                DispatchMessageW(&m);
                if !IsWindow(Some(hwnd)).as_bool() {
                    break;
                }
            }
        }
    });

    let my_hwnd = if block.block_type == "input_adapter" {
        None // Don't wait for input adapter window
    } else {
        rx_hwnd.recv().ok().map(|h| h.0)
    };

    // Associate cancellation token and chain_id with this window
    if let Some(h) = my_hwnd {
        let mut s = WINDOW_STATES.lock().unwrap();
        if let Some(st) = s.get_mut(&(h.0 as isize)) {
            st.cancellation_token = Some(cancel_token.clone());
            st.chain_id = Some(chain_id.to_string());
        }
    }

    // Show loading state
    if !skip_execution && let Some(h) = my_hwnd {
        if block.block_type == "input_adapter" {
            let mut s = WINDOW_STATES.lock().unwrap();
            if let Some(st) = s.get_mut(&(h.0 as isize)) {
                st.is_refining = false;
                st.is_streaming_active = false;
            }
        } else if block.block_type != "image" {
            let mut s = WINDOW_STATES.lock().unwrap();
            if let Some(st) = s.get_mut(&(h.0 as isize)) {
                st.input_text = input_text.to_string();
                st.is_refining = true;
                st.is_streaming_active = true;
            }
        } else {
            let mut s = WINDOW_STATES.lock().unwrap();
            if let Some(st) = s.get_mut(&(h.0 as isize)) {
                st.is_streaming_active = true;
            }
        }
        crate::overlay::result::scene_compositor::queue_window_sync(h);
    }

    // Close old processing overlay for text blocks
    if block.block_type != "image"
        && block.block_type != "input_adapter"
        && let Some(h) = processing_indicator_hwnd
    {
        h.close();
        processing_indicator_hwnd = None;
    }

    (my_hwnd, processing_indicator_hwnd)
}

#[cfg(test)]
#[path = "attribution_tests.rs"]
mod attribution_tests;
