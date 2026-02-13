// --- CHAIN POST-PROCESSING ---
// Copy, paste, speak, history saving, and chain continuation logic.

use crate::config::{Config, ProcessingBlock};
use crate::overlay::result::{ChainCancelToken, RefineContext};
use crate::overlay::text_input;
use crate::win_types::SendHwnd;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::step::run_chain_step;

/// Handle auto-copy functionality for a block result.
pub fn handle_auto_copy(
    block: &ProcessingBlock,
    result_text: &str,
    context: &RefineContext,
    preset_id: &str,
    _config: &Config,
    disable_auto_paste: bool,
) {
    let is_input_adapter = block.block_type == "input_adapter";
    let has_content = !result_text.trim().is_empty();

    if !block.auto_copy {
        return;
    }

    // CASE 1: Image Input Adapter (Source Copy)
    if is_input_adapter {
        if let RefineContext::Image(img_data) = context {
            let img_data_clone = img_data.clone();
            std::thread::spawn(move || {
                crate::overlay::utils::copy_image_to_clipboard(&img_data_clone);
            });
        }
    }

    // CASE 2: Text Content (Result or Source Text) OR Image Content (Source Copy)
    let image_copied = is_input_adapter && matches!(context, RefineContext::Image(_));

    if has_content {
        let txt_c = result_text.to_string();
        let txt_for_badge = result_text.to_string();
        let should_show_badge = !is_input_adapter;
        std::thread::spawn(move || {
            crate::overlay::utils::copy_to_clipboard(&txt_c, HWND::default());
            if should_show_badge {
                crate::overlay::auto_copy_badge::show_auto_copy_badge_text(&txt_for_badge);
            }
        });
    } else if image_copied {
        crate::overlay::auto_copy_badge::show_auto_copy_badge_image();
    }

    // Only trigger paste for:
    // 1. Non-input_adapter blocks with text content (actual processed results)
    // 2. Image copies from input_adapter (intentional image copy)
    let should_trigger_paste = (has_content && !is_input_adapter) || image_copied;

    if should_trigger_paste && !disable_auto_paste {
        let txt_c = result_text.to_string();
        let preset_id_clone = preset_id.to_string();

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));

            let (should_add_newline, should_paste, target_window) = {
                let app = crate::APP.lock().unwrap();
                if let Some(preset) = app.config.presets.iter().find(|p| p.id == preset_id_clone) {
                    (
                        preset.auto_paste_newline,
                        preset.auto_paste,
                        app.last_active_window,
                    )
                } else {
                    let active_idx = app.config.active_preset_idx;
                    if active_idx < app.config.presets.len() {
                        let preset = &app.config.presets[active_idx];
                        (
                            preset.auto_paste_newline,
                            preset.auto_paste,
                            app.last_active_window,
                        )
                    } else {
                        (false, false, app.last_active_window)
                    }
                }
            };

            let final_text = if !txt_c.trim().is_empty() {
                if should_add_newline {
                    format!("{}\n", txt_c)
                } else {
                    txt_c.clone()
                }
            } else {
                String::new()
            };

            if should_paste {
                if txt_c.trim().is_empty() {
                    // Image-only paste path
                    if let Some(target) = target_window {
                        crate::overlay::utils::force_focus_and_paste(target.0);
                    }
                } else {
                    // Text paste path
                    if text_input::is_active() {
                        text_input::set_editor_text(&final_text);
                        text_input::refocus_editor();
                    } else if crate::overlay::result::is_any_refine_active() {
                        if let Some(parent) = crate::overlay::result::get_active_refine_parent() {
                            crate::overlay::result::set_refine_text(parent, &final_text, true);
                        }
                    } else if let Some(target) = target_window {
                        crate::overlay::utils::force_focus_and_paste(target.0);
                    }
                }
            }
        });
    }
}

/// Handle auto-speak functionality.
/// Pass the result window's HWND so the speaker button correctly reflects TTS state.
pub fn handle_auto_speak(block: &ProcessingBlock, result_text: &str, hwnd: Option<HWND>) {
    if block.auto_speak && !result_text.trim().is_empty() {
        let txt_s = result_text.to_string();
        let hwnd_key = hwnd.map(|h| h.0 as isize).unwrap_or(0);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));

            // Set loading state on the window so the speaker button shows "loading"
            if hwnd_key != 0 {
                let mut states = crate::overlay::result::WINDOW_STATES.lock().unwrap();
                if let Some(state) = states.get_mut(&hwnd_key) {
                    state.tts_loading = true;
                }
            }

            let request_id = crate::api::tts::TTS_MANAGER.speak(&txt_s, hwnd_key);

            // Set request_id so the speaker button can stop this TTS
            if hwnd_key != 0 {
                let mut states = crate::overlay::result::WINDOW_STATES.lock().unwrap();
                if let Some(state) = states.get_mut(&hwnd_key) {
                    state.tts_request_id = request_id;
                }
            }
        });
    }
}

/// Save result to history.
pub fn save_to_history(
    block: &ProcessingBlock,
    result_text: &str,
    input_text: &str,
    context: &RefineContext,
) {
    if !block.show_overlay || result_text.trim().is_empty() {
        return;
    }

    let text_for_history = result_text.to_string();

    if block.block_type == "text" {
        let input_text_clone = input_text.to_string();
        std::thread::spawn(move || {
            if let Ok(app) = crate::APP.lock() {
                app.history.save_text(text_for_history, input_text_clone);
            }
        });
    } else if block.block_type == "image" {
        if let RefineContext::Image(img_bytes) = context {
            let img_bytes_clone = img_bytes.clone();
            std::thread::spawn(move || {
                if let Ok(img_dynamic) = image::load_from_memory(&img_bytes_clone) {
                    let img_buffer = img_dynamic.to_rgba8();
                    if let Ok(app) = crate::APP.lock() {
                        app.history.save_image(img_buffer, text_for_history);
                    }
                }
            });
        }
    }
}

/// Continue chain to next blocks (graph traversal).
#[allow(clippy::too_many_arguments)]
pub fn continue_chain(
    block_idx: usize,
    block: &ProcessingBlock,
    result_text: String,
    blocks: Vec<ProcessingBlock>,
    connections: Vec<(usize, usize)>,
    config: Config,
    my_hwnd: Option<HWND>,
    parent_hwnd: Arc<Mutex<Option<SendHwnd>>>,
    context: RefineContext,
    skip_execution: bool,
    processing_indicator_hwnd: Option<SendHwnd>,
    cancel_token: Arc<ChainCancelToken>,
    preset_id: String,
    disable_auto_paste: bool,
    chain_id: String,
    input_hwnd_refocus: Option<SendHwnd>,
    my_rect: RECT,
    starting_rect: RECT,
) {
    // Check cancellation before continuing
    if cancel_token.is_cancelled() {
        if let Some(h) = processing_indicator_hwnd {
            unsafe {
                let _ = PostMessageW(Some(h.0), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }

    // For input_adapter blocks, ALWAYS continue even if result_text is empty
    let should_continue = !result_text.trim().is_empty() || block.block_type == "input_adapter";

    if !should_continue {
        if let Some(h) = processing_indicator_hwnd {
            unsafe {
                let _ = PostMessageW(Some(h.0), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }

    // Find all downstream blocks from connections
    let downstream_indices: Vec<usize> = connections
        .iter()
        .filter(|(from, _)| *from == block_idx)
        .map(|(_, to)| *to)
        .collect();

    // Determine next blocks
    let next_blocks: Vec<usize> = if connections.is_empty() {
        // Legacy mode: no graph connections defined, use linear chain
        if block_idx + 1 < blocks.len() {
            vec![block_idx + 1]
        } else {
            vec![]
        }
    } else {
        // Graph mode: use only explicit connections
        downstream_indices
    };

    if next_blocks.is_empty() {
        if let Some(h) = processing_indicator_hwnd {
            unsafe {
                let _ = PostMessageW(Some(h.0), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }

    let next_parent = if my_hwnd.is_some() {
        Arc::new(Mutex::new(my_hwnd.map(|h| SendHwnd(h))))
    } else {
        parent_hwnd
    };

    let base_rect = if my_hwnd.is_some() {
        my_rect
    } else {
        starting_rect
    };

    let first_next = next_blocks[0];
    let parallel_branches: Vec<usize> = next_blocks.into_iter().skip(1).collect();

    let next_context = if block.block_type == "input_adapter" {
        context.clone()
    } else {
        RefineContext::None
    };

    let next_skip_execution = if skip_execution {
        block.block_type == "input_adapter"
    } else {
        false
    };

    // Spawn parallel threads for additional branches — each gets a CHILD cancel token
    // so closing one branch doesn't affect siblings.
    for (branch_index, next_idx) in parallel_branches.iter().enumerate() {
        let result_clone = result_text.clone();
        let blocks_clone = blocks.clone();
        let conns_clone = connections.clone();
        let config_clone = config.clone();
        let branch_token = ChainCancelToken::child(&cancel_token);
        let parent_clone = next_parent.clone();
        let preset_id_clone = preset_id.clone();
        let chain_id_clone = chain_id.clone();
        let next_idx_copy = *next_idx;
        let branch_context = next_context.clone();
        let branch_rect = base_rect;

        // Incremental delay for each branch
        let delay_ms = (branch_index as u64 + 1) * 300;

        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }

            std::thread::sleep(std::time::Duration::from_millis(delay_ms));

            run_chain_step(
                next_idx_copy,
                result_clone,
                branch_rect,
                blocks_clone,
                conns_clone,
                config_clone,
                parent_clone,
                branch_context,
                next_skip_execution,
                None, // No processing indicator for parallel branches
                branch_token,
                preset_id_clone,
                disable_auto_paste,
                chain_id_clone,
                None,
            );
        });
    }

    // Continue with first downstream block on current thread — also gets a child token
    let first_token = if parallel_branches.is_empty() {
        cancel_token // No fork — reuse parent token directly
    } else {
        ChainCancelToken::child(&cancel_token) // Fork — isolate this branch
    };

    run_chain_step(
        first_next,
        result_text,
        base_rect,
        blocks,
        connections,
        config,
        next_parent,
        next_context,
        next_skip_execution,
        processing_indicator_hwnd,
        first_token,
        preset_id,
        disable_auto_paste,
        chain_id,
        input_hwnd_refocus,
    );
}
