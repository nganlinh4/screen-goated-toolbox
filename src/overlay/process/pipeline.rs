mod image;

use crate::win_types::SendHwnd;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config::{Config, Preset};
use crate::model_config::model_is_non_llm;
use crate::overlay::preset_wheel;
use crate::overlay::result::{self, ChainCancelToken, RefineContext};
use crate::overlay::text_input;

use super::chain::{
    ChainPipelineRequest, ChainStepRequest, execute_chain_pipeline,
    execute_chain_pipeline_with_token, run_chain_step,
};
use super::types::generate_chain_id;
use super::window::create_processing_window;

pub use image::{start_processing_pipeline, start_processing_pipeline_parallel};

// Track last result window rect for continuous mode snaking
lazy_static::lazy_static! {
    static ref LAST_RESULT_RECT: Arc<Mutex<Option<RECT>>> = Arc::new(Mutex::new(None));
}

// --- ENTRY POINTS ---

pub fn start_text_processing(
    initial_text_content: String,
    screen_rect: RECT,
    config: Config,
    preset: Preset,
    localized_preset_name: String, // Already localized by caller
    cancel_hotkey_name: String,    // The actual hotkey name like "Ctrl+Shift+D"
) {
    println!(
        "[DEBUG start_text_processing] preset_id={} text_input_mode={} prompt_mode={} blocks_count={} initial_text_len={}",
        preset.id,
        preset.text_input_mode,
        preset.prompt_mode,
        preset.blocks.len(),
        initial_text_content.len()
    );
    if preset.text_input_mode == "type" {
        // Use first processing block's prompt (skip input_adapter)
        let first_processing_block = preset
            .blocks
            .iter()
            .find(|b| b.block_type != "input_adapter");

        let first_block_prompt = first_processing_block
            .map(|b| b.prompt.as_str())
            .unwrap_or("");

        // Also check if model is non-LLM (doesn't use prompts)
        let first_block_model = first_processing_block
            .map(|b| b.model.as_str())
            .unwrap_or("");

        let guide_text = if first_block_prompt.is_empty() || model_is_non_llm(first_block_model) {
            localized_preset_name
        } else {
            format!("{}...", localized_preset_name)
        };

        let config_shared = Arc::new(config.clone());
        let preset_shared = Arc::new(preset.clone());
        let ui_lang = config.ui_language.clone();
        // For MASTER presets: always keep window open initially (continuous_mode=true)
        // We'll decide whether to close based on the SELECTED preset after wheel selection
        let continuous_mode = if preset.is_master {
            true
        } else {
            preset.continuous_input
        };

        // For continuous mode: store the previous chain's ID so we can close old windows
        let last_chain_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let last_chain_id_clone = last_chain_id.clone();

        // Check if this is a MASTER preset
        let is_master = preset.is_master;

        // CRITICAL: For MASTER presets, store the selected preset index after first wheel selection.
        // Subsequent Enter presses will use this stored preset directly (no wheel).
        // The text input window "transfers" to the selected preset.
        let selected_preset_idx: Arc<Mutex<Option<usize>>> = Arc::new(Mutex::new(None));
        let selected_preset_idx_clone = selected_preset_idx.clone();

        // Reset snaking state for new session
        *LAST_RESULT_RECT.lock().unwrap() = None;

        text_input::show(
            guide_text,
            ui_lang,
            cancel_hotkey_name,
            continuous_mode,
            move |user_text, input_hwnd| {
                // Check if we already selected a preset from the wheel (subsequent submissions)
                let already_selected = *selected_preset_idx_clone.lock().unwrap();

                let (final_preset, final_config, is_continuous) = if let Some(preset_idx) =
                    already_selected
                {
                    // Already selected from wheel previously - use that preset directly (no wheel)
                    let app = crate::APP.lock().unwrap();
                    let p = app.config.presets[preset_idx].clone();
                    let c = app.config.clone();
                    let continuous = p.continuous_input;

                    // Update UI header just in case (e.g. if it reverted or missed an update)
                    let localized_name =
                        crate::gui::settings_ui::get_localized_preset_name(&p.id, &c.ui_language);
                    text_input::update_ui_text(localized_name);

                    (p, c, continuous)
                } else if is_master {
                    // First time MASTER preset - show the preset wheel
                    let mut cursor_pos = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut cursor_pos);
                    }

                    // Show preset wheel - this blocks until user makes selection
                    let selected =
                        preset_wheel::show_preset_wheel("text", Some("type"), cursor_pos);

                    if let Some(idx) = selected {
                        // Store the selected preset index for subsequent submissions
                        *selected_preset_idx_clone.lock().unwrap() = Some(idx);

                        // Refocus the text input window and editor after wheel closes
                        text_input::refocus_editor();

                        // Get the selected preset from config AND update active_preset_idx
                        let mut app = crate::APP.lock().unwrap();
                        // CRITICAL: Update active_preset_idx so auto_paste logic works!
                        app.config.active_preset_idx = idx;
                        let p = app.config.presets[idx].clone();
                        let c = app.config.clone();
                        let continuous = p.continuous_input;

                        // Update UI header with the new preset's name
                        let localized_name = crate::gui::settings_ui::get_localized_preset_name(
                            &p.id,
                            &c.ui_language,
                        );
                        // Find first hotkey name for this preset if available
                        let hk_name = p
                            .hotkeys
                            .first()
                            .map(|h| h.name.clone())
                            .unwrap_or_default();

                        let new_guide_text = if !hk_name.is_empty() {
                            format!("{} [{}]", localized_name, hk_name)
                        } else {
                            localized_name
                        };
                        text_input::update_ui_text(new_guide_text);

                        (p, c, continuous)
                    } else {
                        // User dismissed wheel - refocus and allow retry
                        text_input::refocus_editor();
                        return;
                    }
                } else {
                    // Normal non-MASTER preset
                    let is_continuous = preset_shared.continuous_input;
                    (
                        (*preset_shared).clone(),
                        (*config_shared).clone(),
                        is_continuous,
                    )
                };

                if !is_continuous {
                    // Normal mode: close input window
                    unsafe {
                        let _ = PostMessageW(Some(input_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                } else {
                    // Continuous mode: close previous result overlays before spawning new ones
                    if let Ok(id_guard) = last_chain_id_clone.lock()
                        && let Some(ref old_id) = *id_guard
                    {
                        // Close windows from previous submission
                        result::close_chain_windows(old_id);
                    }
                }

                let overlay_rect = if is_continuous {
                    if let Some(input_rect) = text_input::get_window_rect() {
                        // Calculate Ideal "Under Input" Position
                        // 1. Height: Same as input window
                        // 2. Width: 90% of input window
                        // 3. Position: Centered below input window
                        let input_w = input_rect.right - input_rect.left;
                        let input_h = input_rect.bottom - input_rect.top;

                        let shrink_total = input_w / 10; // 10% shrink
                        let new_w = input_w - shrink_total;

                        let center_x = input_rect.left + (input_w / 2);
                        let left = center_x - (new_w / 2);
                        let new_h = input_h;

                        let ideal_rect = RECT {
                            left,
                            top: input_rect.bottom + 10, // 10px gap below input window
                            right: left + new_w,
                            bottom: input_rect.bottom + 10 + new_h,
                        };

                        // Get monitor rect for boundary checking
                        let monitor_rect = unsafe {
                            let h_monitor = MonitorFromRect(&input_rect, MONITOR_DEFAULTTONEAREST);
                            let mut mi = MONITORINFO {
                                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                                ..Default::default()
                            };
                            if GetMonitorInfoW(h_monitor, &mut mi).as_bool() {
                                mi.rcMonitor
                            } else {
                                // Fallback to primary screen metrics if monitor detection fails
                                RECT {
                                    left: 0,
                                    top: 0,
                                    right: GetSystemMetrics(SM_CXSCREEN),
                                    bottom: GetSystemMetrics(SM_CYSCREEN),
                                }
                            }
                        };

                        // First window: place at ideal_rect directly (under input)
                        // Subsequent windows: snake from last result window
                        let final_rect = if let Some(last_rect) = *LAST_RESULT_RECT.lock().unwrap()
                        {
                            // Second+ window: snake from previous result window
                            crate::overlay::result::layout::calculate_next_window_rect(
                                last_rect,
                                monitor_rect,
                            )
                        } else {
                            // First window: use ideal position directly
                            ideal_rect
                        };

                        // Store this rect as the last result window for next iteration
                        *LAST_RESULT_RECT.lock().unwrap() = Some(final_rect);

                        final_rect
                    } else {
                        screen_rect
                    }
                } else {
                    screen_rect
                };

                // Start processing and track the new cancellation token for continuous mode
                let config_clone = final_config;
                let preset_clone = final_preset;
                let last_id_update = last_chain_id_clone.clone();

                // Reset last result rect for new submission (prevent stale rects from previous chain)
                // Reset last result rect is REMOVED to allow snaking in continuous mode
                // *LAST_RESULT_RECT.lock().unwrap() = None;

                let input_hwnd_send = SendHwnd(input_hwnd);
                std::thread::spawn(move || {
                    // Create a new cancellation token and chain ID for this chain
                    let new_token = ChainCancelToken::new();
                    let chain_id = generate_chain_id();

                    // Store chain ID for later cleanup (in continuous mode)
                    if let Ok(mut id_guard) = last_id_update.lock() {
                        *id_guard = Some(chain_id.clone());
                    }

                    // Execute the chain
                    execute_chain_pipeline_with_token(ChainPipelineRequest {
                        initial_input: user_text,
                        rect: overlay_rect,
                        config: config_clone,
                        preset: preset_clone,
                        context: RefineContext::None,
                        cancel_token: new_token,
                        input_hwnd_refocus: Some(input_hwnd_send),
                        chain_id,
                    });
                });
            },
        );
    } else if preset.prompt_mode == "dynamic" {
        // Dynamic prompt mode for text selection: show WebView input for user to type command
        let ui_lang = config.ui_language.clone();
        // Header shows just the localized preset name (hotkey goes to footer via cancel_hotkey_name)
        let guide_text = localized_preset_name.clone();

        // Store for use in callback
        let initial_text = Arc::new(initial_text_content);
        let config = Arc::new(config);
        let preset = Arc::new(preset);

        text_input::show_with_options(
            guide_text,
            ui_lang,
            cancel_hotkey_name,
            false,
            text_input::ShowOptions {
                activation_mode: text_input::ActivationMode::Passive,
            },
            move |user_prompt, input_hwnd| {
                println!("[DEBUG dynamic] user_prompt=«{}»", user_prompt);

                // Close the input window
                unsafe {
                    let _ = PostMessageW(Some(input_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }

                // Clone preset and modify the first actual processing block's prompt
                let mut modified_preset = (*preset).clone();
                if let Some(target_block) = modified_preset
                    .blocks
                    .iter_mut()
                    .find(|b| b.block_type != "input_adapter")
                {
                    if target_block.prompt.is_empty() {
                        target_block.prompt = user_prompt.clone();
                    } else {
                        target_block.prompt =
                            format!("{}\n\nUser request: {}", target_block.prompt, user_prompt);
                    }
                    println!(
                        "[DEBUG dynamic] final target_block.prompt=«{}»",
                        target_block.prompt
                    );
                } else {
                    println!("[DEBUG dynamic] WARNING: no processing block found in preset!");
                }

                let config_clone = (*config).clone();
                let initial_text_clone = (*initial_text).clone();

                // Execute the chain with modified preset
                std::thread::spawn(move || {
                    execute_chain_pipeline(
                        initial_text_clone,
                        screen_rect,
                        config_clone,
                        modified_preset,
                        RefineContext::None,
                    );
                });
            },
        );
    } else {
        execute_chain_pipeline(
            initial_text_content,
            screen_rect,
            config,
            preset,
            RefineContext::None,
        );
    }
}

pub fn show_audio_result(
    preset: Preset,
    transcription_text: String,
    wav_data: Vec<u8>, // Audio data for input overlay
    rect: RECT,
    _unused_rect: Option<RECT>,
    recording_hwnd: HWND, // Recording overlay window - keep alive until first visible block
    is_streaming_result: bool, // Explicit flag: if true, we disable auto-paste (real-time typing assumed)
) {
    let config = {
        let app = crate::APP.lock().unwrap();
        app.config.clone()
    };

    // Audio processing already completed Block 0 (audio recording/transcription).
    // Start at block 0 with skip_execution=true so it can display its overlay (if configured),
    // then the chain naturally continues to block 1, 2, etc.
    //
    // Pass the recording_hwnd as processing_indicator_hwnd - it will keep animating
    // until the first visible block appears (same behavior as image pipeline).
    let processing_hwnd = if unsafe {
        windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(recording_hwnd)).as_bool()
    } {
        Some(recording_hwnd)
    } else {
        None
    };

    // Generate unique chain ID for this processing chain
    let chain_id = generate_chain_id();

    run_chain_step(ChainStepRequest {
        block_idx: 0,
        input_text: transcription_text,
        current_rect: rect,
        blocks: preset.blocks.clone(),
        connections: preset.block_connections.clone(),
        config,
        parent_hwnd: Arc::new(Mutex::new(None)),
        context: RefineContext::Audio(wav_data),
        skip_execution: true,
        processing_indicator_hwnd: processing_hwnd.map(SendHwnd),
        cancel_token: ChainCancelToken::new(),
        preset_id: preset.id.clone(),
        disable_auto_paste: is_streaming_result,
        chain_id,
        input_hwnd_refocus: None,
    });
}
