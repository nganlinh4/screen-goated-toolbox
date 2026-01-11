use crate::win_types::SendHwnd;
use image::{ImageBuffer, Rgba};
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config::{Config, Preset};
use crate::model_config::model_is_non_llm;
use crate::overlay::preset_wheel;
use crate::overlay::result::{self, RefineContext};
use crate::overlay::text_input;

use super::chain::{execute_chain_pipeline, execute_chain_pipeline_with_token, run_chain_step};
use super::types::generate_chain_id;
use super::window::create_processing_window;

// --- ENTRY POINTS ---

pub fn start_text_processing(
    initial_text_content: String,
    screen_rect: RECT,
    config: Config,
    preset: Preset,
    localized_preset_name: String, // Already localized by caller
    cancel_hotkey_name: String,    // The actual hotkey name like "Ctrl+Shift+D"
) {
    if preset.text_input_mode == "type" {
        // Use blocks[0].prompt instead of legacy preset.prompt
        let first_block_prompt = preset
            .blocks
            .first()
            .map(|b| b.prompt.as_str())
            .unwrap_or("");

        // Also check if model is non-LLM (doesn't use prompts)
        let first_block_model = preset
            .blocks
            .first()
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

        // For continuous mode: store the previous chain's cancellation token so we can close old windows
        let last_cancel_token: Arc<Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>> =
            Arc::new(Mutex::new(None));
        let last_cancel_token_clone = last_cancel_token.clone();

        // Check if this is a MASTER preset
        let is_master = preset.is_master;

        // CRITICAL: For MASTER presets, store the selected preset index after first wheel selection.
        // Subsequent Enter presses will use this stored preset directly (no wheel).
        // The text input window "transfers" to the selected preset.
        let selected_preset_idx: Arc<Mutex<Option<usize>>> = Arc::new(Mutex::new(None));
        let selected_preset_idx_clone = selected_preset_idx.clone();

        text_input::show(
            guide_text,
            ui_lang,
            cancel_hotkey_name,
            continuous_mode,
            move |user_text, input_hwnd| {
                // Check if we already selected a preset from the wheel (subsequent submissions)
                let already_selected = selected_preset_idx_clone.lock().unwrap().clone();

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
                    let is_continuous = (*preset_shared).continuous_input;
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
                    if let Ok(token_guard) = last_cancel_token_clone.lock() {
                        if let Some(ref old_token) = *token_guard {
                            // Close windows from previous submission
                            result::close_windows_with_token(old_token);
                        }
                    }
                }

                let overlay_rect = if is_continuous {
                    if let Some(input_rect) = text_input::get_window_rect() {
                        // Calculate Ideal "Under Input" Position
                        // 1. Height: 280px (Reasonable bubble size)
                        // 2. Width: 90% of input window
                        // 3. Position: Centered below input window
                        let input_w = input_rect.right - input_rect.left;
                        let shrink_total = input_w / 10;
                        let new_w = input_w - shrink_total;
                        let center_x = input_rect.left + (input_w / 2);
                        let left = center_x - (new_w / 2);

                        let ideal_rect = RECT {
                            left,
                            top: input_rect.bottom + 10, // 10px gap below input window
                            right: left + new_w,
                            bottom: input_rect.bottom + 10 + 280,
                        };

                        // Smart Placement Logic:
                        // 1. Try the ideal "Under Input" spot first.
                        // 2. If occupied, ask layout engine for the next best spot relative to it.
                        //    - This ensures Result 1 goes under input.
                        //    - Result 2 detects Result 1 is there, and goes Right (standard snake).
                        if crate::overlay::result::layout::is_rect_occupied(&ideal_rect) {
                            // Multi-monitor aware monitor detection
                            let monitor_rect = unsafe {
                                let h_monitor =
                                    MonitorFromRect(&input_rect, MONITOR_DEFAULTTONEAREST);
                                let mut mi = MONITORINFO::default();
                                mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
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

                            crate::overlay::result::layout::calculate_next_window_rect(
                                ideal_rect,
                                monitor_rect,
                            )
                        } else {
                            ideal_rect
                        }
                    } else {
                        screen_rect
                    }
                } else {
                    screen_rect
                };

                // Start processing and track the new cancellation token for continuous mode
                let config_clone = final_config;
                let preset_clone = final_preset;
                let last_token_update = last_cancel_token_clone.clone();

                let input_hwnd_send = SendHwnd(input_hwnd);
                std::thread::spawn(move || {
                    // Create a new cancellation token for this chain
                    let new_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

                    // Store it for later cleanup (in continuous mode)
                    if let Ok(mut token_guard) = last_token_update.lock() {
                        *token_guard = Some(new_token.clone());
                    }

                    // Execute the chain
                    execute_chain_pipeline_with_token(
                        user_text,
                        overlay_rect,
                        config_clone,
                        preset_clone,
                        RefineContext::None,
                        new_token,
                        Some(input_hwnd_send),
                    );
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

        text_input::show(
            guide_text,
            ui_lang,
            cancel_hotkey_name,
            false,
            move |user_prompt, input_hwnd| {
                // Close the input window
                unsafe {
                    let _ = PostMessageW(Some(input_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }

                // Clone preset and modify the first block's prompt with user's input
                let mut modified_preset = (*preset).clone();
                if let Some(block0) = modified_preset.blocks.get_mut(0) {
                    if block0.prompt.is_empty() {
                        block0.prompt = user_prompt.clone();
                    } else {
                        block0.prompt =
                            format!("{}\n\nUser request: {}", block0.prompt, user_prompt);
                    }
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

    run_chain_step(
        0,
        transcription_text,
        rect,
        preset.blocks.clone(),
        preset.block_connections.clone(), // Graph connections
        config,
        Arc::new(Mutex::new(None)),
        RefineContext::Audio(wav_data), // Pass audio data for input overlay
        true, // skip_execution: audio already done, just display and chain forward
        processing_hwnd.map(SendHwnd), // Pass recording overlay - will close when first visible block appears
        Arc::new(AtomicBool::new(false)), // New chains start with cancellation = false
        preset.id.clone(),
        // Check if we should disable auto-paste (e.g. for Gemini Live real-time typing)
        is_streaming_result,
        chain_id, // Per-chain position tracking
        None,     // No input refocus for audio results
    );
}

pub fn start_processing_pipeline(
    cropped_img: ImageBuffer<Rgba<u8>, Vec<u8>>,
    screen_rect: RECT,
    config: Config,
    preset: Preset,
) {
    // If dynamic prompt mode, use WebView-based text input
    if preset.prompt_mode == "dynamic" && !preset.blocks.is_empty() {
        // For dynamic mode, encode PNG first (user will type prompt)
        let mut png_data = Vec::new();
        let _ = cropped_img.write_to(
            &mut std::io::Cursor::new(&mut png_data),
            image::ImageFormat::Png,
        );

        // Get localized UI elements
        let ui_lang = config.ui_language.clone();
        let localized_name =
            crate::gui::settings_ui::get_localized_preset_name(&preset.id, &ui_lang);
        let guide_text = format!("{}...", localized_name);
        let cancel_hotkey = preset
            .hotkeys
            .first()
            .map(|h| h.name.clone())
            .unwrap_or_default();

        // Store for use in callback
        let png_data = Arc::new(png_data);
        let config = Arc::new(config);
        let preset = Arc::new(preset);

        // Use WebView-based text input
        text_input::show(
            guide_text,
            ui_lang,
            cancel_hotkey,
            false,
            move |user_prompt, input_hwnd| {
                // Close the input window
                unsafe {
                    let _ = PostMessageW(Some(input_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }

                // Clone preset and modify the first actual processing block's prompt with user's input
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
                }

                // Context with image data
                let context = RefineContext::Image((*png_data).clone());
                let config_clone = (*config).clone();
                let graphics_mode = config_clone.graphics_mode.clone();

                // Create processing window IMMEDIATELY
                let processing_hwnd =
                    unsafe { create_processing_window(screen_rect, graphics_mode) };
                unsafe {
                    let _ =
                        SendMessageW(processing_hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
                }

                // Generate unique chain ID for this processing chain
                let chain_id = generate_chain_id();

                // Spawn chain execution - reusing existing run_chain_step!
                let blocks = modified_preset.blocks.clone();
                let connections = modified_preset.block_connections.clone();
                let preset_id = modified_preset.id.clone();

                let processing_hwnd_send = SendHwnd(processing_hwnd);
                std::thread::spawn(move || {
                    run_chain_step(
                        0,
                        String::new(),
                        screen_rect,
                        blocks,
                        connections,
                        config_clone,
                        Arc::new(Mutex::new(None)),
                        context,
                        false,
                        Some(processing_hwnd_send),
                        Arc::new(AtomicBool::new(false)),
                        preset_id,
                        false,    // disable_auto_paste
                        chain_id, // Per-chain position tracking
                        None,     // No input refocus
                    );
                });

                // Keep processing window alive until closed
                unsafe {
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).into() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                        if !IsWindow(Some(processing_hwnd)).as_bool() {
                            break;
                        }
                    }
                }
            },
        );
        return;
    }

    // STANDARD PIPELINE: Create processing window IMMEDIATELY, then encode PNG in background
    // This eliminates the delay between selection and animation appearing

    // 1. Create Processing Window FIRST (instant, no delay)
    let graphics_mode = config.graphics_mode.clone();
    let processing_hwnd = unsafe { create_processing_window(screen_rect, graphics_mode) };
    unsafe {
        let _ = SendMessageW(processing_hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
    }

    // 2. Spawn background thread to encode PNG and start chain execution
    let conf_clone = config.clone();
    let blocks = preset.blocks.clone();
    let connections = preset.block_connections.clone();
    let preset_id = preset.id.clone();

    let processing_hwnd_val = processing_hwnd.0 as usize;
    std::thread::spawn(move || {
        let processing_hwnd = HWND(processing_hwnd_val as *mut std::ffi::c_void);
        // Heavy work: PNG encoding happens here, while animation plays
        let mut png_data = Vec::new();
        let _ = cropped_img.write_to(
            &mut std::io::Cursor::new(&mut png_data),
            image::ImageFormat::Png,
        );
        let context = RefineContext::Image(png_data);

        // Generate unique chain ID for this processing chain
        let chain_id = generate_chain_id();

        // Start chain execution with the pre-created processing window
        run_chain_step(
            0,
            String::new(),
            screen_rect,
            blocks,
            connections, // Graph connections
            conf_clone,
            Arc::new(Mutex::new(None)),
            context,
            false,
            Some(SendHwnd(processing_hwnd)), // Pass the handle to be closed later
            Arc::new(AtomicBool::new(false)), // New chains start with cancellation = false
            preset_id,
            false,    // disable_auto_paste
            chain_id, // Per-chain position tracking
            None,     // No input refocus
        );
    });

    // 3. Keep the Processing Window alive on this thread until it is destroyed by the worker
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if !IsWindow(Some(processing_hwnd)).as_bool() {
                break;
            }
        }
    }
}

pub fn start_processing_pipeline_parallel(
    rx: std::sync::mpsc::Receiver<Option<(ImageBuffer<Rgba<u8>, Vec<u8>>, Vec<u8>)>>,
    screen_rect: RECT,
    config: Config,
    preset: Preset,
) {
    // Dynamic prompt mode optimization is complex due to text input dependency
    // Fallback to blocking wait for dynamic mode (usually user input is the bottleneck anyway)
    if preset.prompt_mode == "dynamic" {
        if let Ok(Some((img, _))) = rx.recv() {
            start_processing_pipeline(img, screen_rect, config, preset);
        }
        return;
    }

    // STANDARD PIPELINE PARALLEL
    // 1. Create Processing Window FIRST (instant, no delay)
    let graphics_mode = config.graphics_mode.clone();
    let processing_hwnd = unsafe { create_processing_window(screen_rect, graphics_mode) };
    unsafe {
        let _ = SendMessageW(processing_hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
    }

    // 2. Spawn background thread to wait for data, encode PNG and start chain execution
    let conf_clone = config.clone();
    let blocks = preset.blocks.clone();
    let connections = preset.block_connections.clone();
    let preset_id = preset.id.clone();
    let processing_hwnd_val = processing_hwnd.0 as usize;

    std::thread::spawn(move || {
        let processing_hwnd = HWND(processing_hwnd_val as *mut std::ffi::c_void);

        // WAIT FOR DATA - delays here won't freeze UI!
        if let Ok(Some((_cropped_img, original_bytes))) = rx.recv() {
            // Use original bytes directly (Zero-Copy/Zero-Encode)
            // This preserves JPEG format if input was JPEG
            let context = RefineContext::Image(original_bytes);

            // Generate unique chain ID for this processing chain
            let chain_id = generate_chain_id();

            // Start chain execution with the pre-created processing window
            run_chain_step(
                0,
                String::new(),
                screen_rect,
                blocks,
                connections,
                conf_clone,
                Arc::new(Mutex::new(None)),
                context,
                false,
                Some(SendHwnd(processing_hwnd)), // Pass the handle to be closed later
                Arc::new(AtomicBool::new(false)),
                preset_id,
                false,    // disable_auto_paste
                chain_id, // Per-chain position tracking
                None,     // No input refocus
            );
        } else {
            // Load failed or cancelled -> Close window immediately
            unsafe {
                let _ = PostMessageW(Some(processing_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    });

    // 3. Keep the Processing Window alive on this thread until it is destroyed by the worker
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if !IsWindow(Some(processing_hwnd)).as_bool() {
                break;
            }
        }
    }
}
