use crate::config::{Config, Preset};
use crate::overlay::result::{ChainCancelToken, RefineContext};
use crate::overlay::text_input;
use crate::win_types::SendHwnd;
use image::{ImageBuffer, Rgba};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{ChainStepRequest, create_processing_window, generate_chain_id, run_chain_step};

type CapturedImagePayload = (ImageBuffer<Rgba<u8>, Vec<u8>>, Vec<u8>);

pub fn start_processing_pipeline(
    cropped_img: ImageBuffer<Rgba<u8>, Vec<u8>>,
    screen_rect: RECT,
    config: Config,
    preset: Preset,
) {
    if preset.prompt_mode == "dynamic" && !preset.blocks.is_empty() {
        let mut png_data = Vec::new();
        let _ = cropped_img.write_to(
            &mut std::io::Cursor::new(&mut png_data),
            image::ImageFormat::Png,
        );

        let ui_lang = config.ui_language.clone();
        let localized_name =
            crate::gui::settings_ui::get_localized_preset_name(&preset.id, &ui_lang);
        let guide_text = format!("{}...", localized_name);
        let cancel_hotkey = preset
            .hotkeys
            .first()
            .map(|h| h.name.clone())
            .unwrap_or_default();

        let png_data = Arc::new(png_data);
        let config = Arc::new(config);
        let preset = Arc::new(preset);

        text_input::show(
            guide_text,
            ui_lang,
            cancel_hotkey,
            false,
            move |user_prompt, input_hwnd| {
                unsafe {
                    let _ = PostMessageW(Some(input_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }

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

                let context = RefineContext::Image((*png_data).clone());
                let config_clone = (*config).clone();
                let graphics_mode = config_clone.graphics_mode.clone();

                let processing_hwnd =
                    unsafe { create_processing_window(screen_rect, graphics_mode) };
                unsafe {
                    let _ =
                        SendMessageW(processing_hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
                }

                let chain_id = generate_chain_id();
                let blocks = modified_preset.blocks.clone();
                let connections = modified_preset.block_connections.clone();
                let preset_id = modified_preset.id.clone();

                let processing_hwnd_send = SendHwnd(processing_hwnd);
                std::thread::spawn(move || {
                    run_chain_step(ChainStepRequest {
                        block_idx: 0,
                        input_text: String::new(),
                        current_rect: screen_rect,
                        blocks,
                        connections,
                        config: config_clone,
                        parent_hwnd: Arc::new(Mutex::new(None)),
                        context,
                        skip_execution: false,
                        processing_indicator_hwnd: Some(processing_hwnd_send),
                        cancel_token: ChainCancelToken::new(),
                        preset_id,
                        disable_auto_paste: false,
                        chain_id,
                        input_hwnd_refocus: None,
                    });
                });

                run_message_loop_until_closed(processing_hwnd);
            },
        );
        return;
    }

    let graphics_mode = config.graphics_mode.clone();
    let processing_hwnd = unsafe { create_processing_window(screen_rect, graphics_mode) };
    unsafe {
        let _ = SendMessageW(processing_hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
    }

    let conf_clone = config.clone();
    let blocks = preset.blocks.clone();
    let connections = preset.block_connections.clone();
    let preset_id = preset.id.clone();

    let processing_hwnd_val = processing_hwnd.0 as usize;
    std::thread::spawn(move || {
        let processing_hwnd = HWND(processing_hwnd_val as *mut std::ffi::c_void);
        let mut png_data = Vec::new();
        let _ = cropped_img.write_to(
            &mut std::io::Cursor::new(&mut png_data),
            image::ImageFormat::Png,
        );
        let context = RefineContext::Image(png_data);
        let chain_id = generate_chain_id();

        run_chain_step(ChainStepRequest {
            block_idx: 0,
            input_text: String::new(),
            current_rect: screen_rect,
            blocks,
            connections,
            config: conf_clone,
            parent_hwnd: Arc::new(Mutex::new(None)),
            context,
            skip_execution: false,
            processing_indicator_hwnd: Some(SendHwnd(processing_hwnd)),
            cancel_token: ChainCancelToken::new(),
            preset_id,
            disable_auto_paste: false,
            chain_id,
            input_hwnd_refocus: None,
        });
    });

    run_message_loop_until_closed(processing_hwnd);
}

pub fn start_processing_pipeline_parallel(
    rx: std::sync::mpsc::Receiver<Option<CapturedImagePayload>>,
    screen_rect: RECT,
    config: Config,
    preset: Preset,
) {
    if preset.prompt_mode == "dynamic" {
        if let Ok(Some((img, _))) = rx.recv() {
            start_processing_pipeline(img, screen_rect, config, preset);
        }
        return;
    }

    let graphics_mode = config.graphics_mode.clone();
    let processing_hwnd = unsafe { create_processing_window(screen_rect, graphics_mode) };
    unsafe {
        let _ = SendMessageW(processing_hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
    }

    let conf_clone = config.clone();
    let blocks = preset.blocks.clone();
    let connections = preset.block_connections.clone();
    let preset_id = preset.id.clone();
    let processing_hwnd_val = processing_hwnd.0 as usize;

    std::thread::spawn(move || {
        let processing_hwnd = HWND(processing_hwnd_val as *mut std::ffi::c_void);

        if let Ok(Some((_cropped_img, original_bytes))) = rx.recv() {
            let context = RefineContext::Image(original_bytes);
            let chain_id = generate_chain_id();

            run_chain_step(ChainStepRequest {
                block_idx: 0,
                input_text: String::new(),
                current_rect: screen_rect,
                blocks,
                connections,
                config: conf_clone,
                parent_hwnd: Arc::new(Mutex::new(None)),
                context,
                skip_execution: false,
                processing_indicator_hwnd: Some(SendHwnd(processing_hwnd)),
                cancel_token: ChainCancelToken::new(),
                preset_id,
                disable_auto_paste: false,
                chain_id,
                input_hwnd_refocus: None,
            });
        } else {
            unsafe {
                let _ = PostMessageW(Some(processing_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    });

    run_message_loop_until_closed(processing_hwnd);
}

fn run_message_loop_until_closed(processing_hwnd: HWND) {
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
