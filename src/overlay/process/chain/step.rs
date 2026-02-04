// --- CHAIN STEP EXECUTION ---
// Core logic for running a single step in the processing chain.

use crate::config::{Config, ProcessingBlock};
use crate::overlay::result::{
    create_result_window, get_chain_color, link_windows, RefineContext, WindowType, WINDOW_STATES,
};
use crate::win_types::SendHwnd;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::execution::execute_block;
use super::post_process::{continue_chain, handle_auto_copy, handle_auto_speak, save_to_history};
use super::templates::{generate_audio_player_html, generate_image_display_html};
use crate::overlay::process::types::get_next_window_position_for_chain;

/// Recursive step to run a block in the chain (supports graph with connections).
#[allow(clippy::too_many_arguments)]
pub fn run_chain_step(
    block_idx: usize,
    input_text: String,
    current_rect: RECT,
    blocks: Vec<ProcessingBlock>,
    connections: Vec<(usize, usize)>,
    config: Config,
    parent_hwnd: Arc<Mutex<Option<SendHwnd>>>,
    context: RefineContext,
    skip_execution: bool,
    mut processing_indicator_hwnd: Option<SendHwnd>,
    cancel_token: Arc<AtomicBool>,
    preset_id: String,
    disable_auto_paste: bool,
    chain_id: String,
    input_hwnd_refocus: Option<SendHwnd>,
) {
    // Check if cancelled before starting
    if cancel_token.load(Ordering::Relaxed) {
        if let Some(h) = processing_indicator_hwnd {
            unsafe {
                let _ = PostMessageW(Some(h.0), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }

    if block_idx >= blocks.len() {
        if let Some(h) = processing_indicator_hwnd {
            unsafe {
                let _ = PostMessageW(Some(h.0), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }

    // Clone the block to avoid borrowing issues when passing blocks to continue_chain
    let block = blocks[block_idx].clone();

    // 1. Resolve Model & Prompt
    let model_id = block.model.clone();
    let model_conf = crate::model_config::get_model_by_id(&model_id);
    let provider = model_conf
        .clone()
        .map(|m| m.provider)
        .unwrap_or("groq".to_string());
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
    if block_idx == 0 {
        if let Ok(app) = crate::APP.lock() {
            let found_preset = app.config.presets.iter().find(|p| p.id == preset_id);
            if let Some(p) = found_preset {
                let is_image_category = p.preset_type == "image";
                if !is_image_category {
                    if let Some(geom) = &p.window_geometry {
                        starting_rect = RECT {
                            left: geom.x,
                            top: geom.y,
                            right: geom.x + geom.width,
                            bottom: geom.y + geom.height,
                        };
                    }
                }
            }
        }
    }

    let my_rect = if block.show_overlay {
        get_next_window_position_for_chain(&chain_id, starting_rect)
    } else {
        starting_rect
    };

    let mut my_hwnd: Option<HWND> = None;

    // 3. Create Window (if visible)
    let should_create_window = block.show_overlay;

    if block.block_type == "input_adapter" && !block.show_overlay {
        // Input adapter without overlay - invisible pass-through
    } else if should_create_window {
        let (created_hwnd, new_processing_hwnd) = create_block_window(
            &block,
            block_idx,
            my_rect,
            &context,
            &input_text,
            &model_id,
            &provider,
            &final_prompt,
            bg_color,
            visible_count_before,
            skip_execution,
            &parent_hwnd,
            &cancel_token,
            &preset_id,
            &config,
            processing_indicator_hwnd,
            input_hwnd_refocus.clone(),
        );
        my_hwnd = created_hwnd;
        processing_indicator_hwnd = new_processing_hwnd;
    }

    // 4. Execution (API Call)
    let input_text_for_history = input_text.clone();
    let result_text = execute_block(
        &block,
        block_idx,
        &blocks,
        my_hwnd,
        &input_text,
        &context,
        &model_id,
        &provider,
        &model_full_name,
        &final_prompt,
        skip_execution,
        &config,
        &preset_id,
        processing_indicator_hwnd.clone(),
    );

    // 5. Post-Processing
    handle_auto_copy(
        &block,
        &result_text,
        &context,
        &preset_id,
        &config,
        disable_auto_paste,
    );
    handle_auto_speak(&block, &result_text);
    save_to_history(&block, &result_text, &input_text_for_history, &context);

    // 6. Chain Next Steps
    continue_chain(
        block_idx,
        &block,
        result_text,
        blocks,
        connections,
        config,
        my_hwnd,
        parent_hwnd,
        context,
        skip_execution,
        processing_indicator_hwnd,
        cancel_token,
        preset_id,
        disable_auto_paste,
        chain_id,
        input_hwnd_refocus,
        my_rect,
        starting_rect,
    );
}

/// Create window for a block and return (hwnd, updated processing_indicator_hwnd).
#[allow(clippy::too_many_arguments)]
fn create_block_window(
    block: &ProcessingBlock,
    block_idx: usize,
    my_rect: RECT,
    context: &RefineContext,
    input_text: &str,
    model_id: &str,
    provider: &str,
    final_prompt: &str,
    bg_color: u32,
    visible_count_before: usize,
    skip_execution: bool,
    parent_hwnd: &Arc<Mutex<Option<SendHwnd>>>,
    cancel_token: &Arc<AtomicBool>,
    preset_id: &str,
    config: &Config,
    mut processing_indicator_hwnd: Option<SendHwnd>,
    input_hwnd_refocus: Option<SendHwnd>,
) -> (Option<HWND>, Option<SendHwnd>) {
    let ctx_clone = if block.block_type == "input_adapter" || block_idx == 0 {
        context.clone()
    } else {
        RefineContext::None
    };

    let stream_en = if block.render_mode == "markdown" || skip_execution {
        false
    } else {
        block.streaming_enabled
    };
    let render_md = block.render_mode.clone();
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
    let input_hwnd_refocus_thread = input_hwnd_refocus.clone();
    let preset_id_for_window = preset_id.to_string();
    let m_id = model_id.to_string();
    let prov = provider.to_string();
    let prompt_c = final_prompt.to_string();

    let (tx_hwnd, rx_hwnd) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let is_root = visible_count_before == 0;

        let hwnd = create_result_window(
            my_rect,
            WindowType::Primary,
            ctx_clone,
            m_id,
            prov,
            stream_en,
            false,
            prompt_c,
            bg_color,
            &render_md,
            initial_content_clone,
            Some(preset_id_for_window),
            is_root,
        );

        // Assign cancellation token immediately
        {
            let mut s = WINDOW_STATES.lock().unwrap();
            if let Some(st) = s.get_mut(&(hwnd.0 as isize)) {
                st.cancellation_token = Some(cancel_token_thread.clone());
            }
        }

        if let Ok(p_guard) = parent_clone.lock() {
            if let Some(ph) = *p_guard {
                link_windows(ph.0, hwnd);
            }
        }

        if !is_image_block {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNA);
                if let Some(h_input) = input_hwnd_refocus_thread {
                    let _ = SetForegroundWindow(h_input.0);
                    let _ = SetFocus(Some(h_input.0));
                }
            }
        }
        let _ = tx_hwnd.send(SendHwnd(hwnd));

        unsafe {
            if is_input_adapter_image {
                use windows::Win32::UI::WindowsAndMessaging::{LWA_ALPHA, SetLayeredWindowAttributes};
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

    // Associate cancellation token with this window
    if let Some(h) = my_hwnd {
        let mut s = WINDOW_STATES.lock().unwrap();
        if let Some(st) = s.get_mut(&(h.0 as isize)) {
            st.cancellation_token = Some(cancel_token.clone());
        }
    }

    // Show loading state
    if !skip_execution && my_hwnd.is_some() {
        let h = my_hwnd.unwrap();
        if block.block_type == "input_adapter" {
            let mut s = WINDOW_STATES.lock().unwrap();
            if let Some(st) = s.get_mut(&(h.0 as isize)) {
                st.is_refining = false;
                st.is_streaming_active = false;
                st.font_cache_dirty = true;
            }
        } else if block.block_type != "image" {
            let mut s = WINDOW_STATES.lock().unwrap();
            if let Some(st) = s.get_mut(&(h.0 as isize)) {
                st.input_text = input_text.to_string();
                st.is_refining = true;
                st.is_streaming_active = true;
                st.was_streaming_active = true;
                st.font_cache_dirty = true;
            }
        } else {
            let mut s = WINDOW_STATES.lock().unwrap();
            if let Some(st) = s.get_mut(&(h.0 as isize)) {
                st.is_streaming_active = true;
                st.was_streaming_active = true;
            }
        }
    }

    // Close old processing overlay for text blocks
    if block.block_type != "image" && block.block_type != "input_adapter" {
        if let Some(h) = processing_indicator_hwnd {
            unsafe {
                let _ = PostMessageW(Some(h.0), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            processing_indicator_hwnd = None;
        }
    }

    (my_hwnd, processing_indicator_hwnd)
}
