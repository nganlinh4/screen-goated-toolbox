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
    auto_paste_destination: super::super::chain::AutoPasteDestination,
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
            .map(|h| h.display_name())
            .unwrap_or_default();

        let png_data = Arc::new(png_data);
        let config = Arc::new(config);
        let preset = Arc::new(preset);

        text_input::show(
            guide_text,
            ui_lang,
            cancel_hotkey,
            false,
            move |user_prompt, _input_hwnd, _session| {
                let mut modified_preset = (*preset).clone();
                if let Some(index) = modified_preset.first_reachable_processing_index()
                    && let Some(target_block) = modified_preset.blocks.get_mut(index)
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
                // The text-input callback must return: the processing window
                // lives and pumps on its own thread, never nested in this one.
                spawn_processing_window(screen_rect, move |processing_hwnd| {
                    run_chain_step(ChainStepRequest {
                        block_idx: 0,
                        input_text: String::new(),
                        current_rect: screen_rect,
                        blocks: modified_preset.blocks,
                        connections: modified_preset.block_connections,
                        graph_explicit: modified_preset.graph_explicit,
                        config: config_clone,
                        parent_hwnd: Arc::new(Mutex::new(None)),
                        context,
                        skip_execution: false,
                        preprocessed_audio: None,
                        visited_blocks: Vec::new(),
                        processing_indicator_hwnd: Some(
                            crate::overlay::process::chain::ProcessingIndicator::ordinary(
                                processing_hwnd,
                            ),
                        ),
                        cancel_token: ChainCancelToken::new(),
                        preset_id: modified_preset.id,
                        disable_auto_paste: false,
                        auto_paste_destination,
                        chain_id: generate_chain_id(),
                        input_hwnd_refocus: None,
                    });
                });
            },
        );
        return;
    }

    let processing_hwnd = unsafe { create_processing_window(screen_rect) };
    unsafe {
        let _ = SendMessageW(processing_hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
    }

    let conf_clone = config.clone();
    let blocks = preset.blocks.clone();
    let connections = preset.block_connections.clone();
    let graph_explicit = preset.graph_explicit;
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
            graph_explicit,
            config: conf_clone,
            parent_hwnd: Arc::new(Mutex::new(None)),
            context,
            skip_execution: false,
            preprocessed_audio: None,
            visited_blocks: Vec::new(),
            processing_indicator_hwnd: Some(
                crate::overlay::process::chain::ProcessingIndicator::ordinary(SendHwnd(
                    processing_hwnd,
                )),
            ),
            cancel_token: ChainCancelToken::new(),
            preset_id,
            disable_auto_paste: false,
            auto_paste_destination,
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
    auto_paste_destination: super::super::chain::AutoPasteDestination,
) {
    if preset.prompt_mode == "dynamic" {
        if let Ok(Some((img, _))) = rx.recv() {
            start_processing_pipeline(img, screen_rect, config, preset, auto_paste_destination);
        }
        return;
    }

    let processing_hwnd = unsafe { create_processing_window(screen_rect) };
    unsafe {
        let _ = SendMessageW(processing_hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
    }

    let conf_clone = config.clone();
    let blocks = preset.blocks.clone();
    let connections = preset.block_connections.clone();
    let graph_explicit = preset.graph_explicit;
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
                graph_explicit,
                config: conf_clone,
                parent_hwnd: Arc::new(Mutex::new(None)),
                context,
                skip_execution: false,
                preprocessed_audio: None,
                visited_blocks: Vec::new(),
                processing_indicator_hwnd: Some(
                    crate::overlay::process::chain::ProcessingIndicator::ordinary(SendHwnd(
                        processing_hwnd,
                    )),
                ),
                cancel_token: ChainCancelToken::new(),
                preset_id,
                disable_auto_paste: false,
                auto_paste_destination,
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

/// Creates a processing window on a new thread that owns it, starts `run` on
/// a worker thread, and pumps the owner's messages until the window closes.
/// Returns at once; the handle finishes when the window is gone.
fn spawn_processing_window(
    screen_rect: RECT,
    run: impl FnOnce(SendHwnd) + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let processing_hwnd = unsafe { create_processing_window(screen_rect) };
        if !unsafe { IsWindow(Some(processing_hwnd)).as_bool() } {
            return;
        }
        unsafe {
            let _ = SendMessageW(processing_hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
        }
        let window = SendHwnd(processing_hwnd);
        std::thread::spawn(move || run(window));
        run_message_loop_until_closed(processing_hwnd);
    })
}

fn run_message_loop_until_closed(processing_hwnd: HWND) {
    unsafe {
        let mut msg = MSG::default();
        while IsWindow(Some(processing_hwnd)).as_bool() && GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn processing_window_runs_on_its_own_thread_and_the_caller_returns_at_once() {
        let (returned, caller_returned) = std::sync::mpsc::channel::<()>();
        let (started, window_started) = std::sync::mpsc::channel();
        let owner = spawn_processing_window(
            RECT {
                left: 0,
                top: 0,
                right: 64,
                bottom: 64,
            },
            move |window| {
                // Close only after the caller is back, as a submitted prompt's
                // chain finishes long after the text-input callback returned.
                caller_returned.recv().unwrap();
                started.send(window.0.0 as usize).unwrap();
                unsafe {
                    let _ = PostMessageW(Some(window.0), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            },
        );
        returned.send(()).unwrap();
        let window =
            HWND(window_started.recv_timeout(Duration::from_secs(5)).unwrap()
                as *mut std::ffi::c_void);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !owner.is_finished() {
            assert!(Instant::now() < deadline, "processing window never closed");
            std::thread::sleep(Duration::from_millis(10));
        }
        owner.join().unwrap();
        assert!(!unsafe { IsWindow(Some(window)) }.as_bool());
    }
}
