// --- CHAIN PIPELINE MODULE ---
// Processing chain execution with graph-based block connections.

mod execution;
mod post_process;
mod step;
mod templates;

pub use step::run_chain_step;

use crate::config::{Config, Preset};
use crate::overlay::result::{ChainCancelToken, RefineContext};
use crate::win_types::SendHwnd;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::types::{generate_chain_id, get_rect_with_saved_geometry};
use super::window::create_processing_window;

// --- CORE PIPELINE LOGIC ---

/// Execute a chain pipeline with a new processing window.
pub fn execute_chain_pipeline(
    initial_input: String,
    rect: RECT,
    config: Config,
    preset: Preset,
    context: RefineContext,
) {
    // Apply saved geometry so processing window appears at the same position as result window
    let corrected_rect = get_rect_with_saved_geometry(&preset.id, rect);

    // 1. Create Processing Window (Gradient Glow)
    let graphics_mode = config.graphics_mode.clone();
    let processing_hwnd = unsafe { create_processing_window(corrected_rect, graphics_mode) };
    unsafe {
        let _ = SendMessageW(processing_hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
    }

    // 2. Start the chain execution on a BACKGROUND thread
    let conf_clone = config.clone();
    let blocks = preset.blocks.clone();
    let connections = preset.block_connections.clone();
    let preset_id = preset.id.clone();

    let processing_hwnd_send = SendHwnd(processing_hwnd);
    std::thread::spawn(move || {
        let chain_id = generate_chain_id();

        run_chain_step(
            0,
            initial_input,
            corrected_rect,
            blocks,
            connections,
            conf_clone,
            Arc::new(Mutex::new(None)),
            context,
            false,
            Some(processing_hwnd_send),
            ChainCancelToken::new(),
            preset_id,
            false,
            chain_id,
            None,
        );
    });

    // 3. Keep the Processing Window alive on this thread
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

/// Execute chain pipeline with a pre-created cancellation token.
/// Used for continuous input mode to track and close previous chain windows.
/// NOTE: For text presets, we don't create a processing window (gradient glow).
/// Instead, we rely on the refining animation baked into the result window.
pub fn execute_chain_pipeline_with_token(
    initial_input: String,
    rect: RECT,
    config: Config,
    preset: Preset,
    context: RefineContext,
    cancel_token: Arc<ChainCancelToken>,
    input_hwnd_refocus: Option<SendHwnd>,
    chain_id: String,
) {
    let blocks = preset.blocks.clone();
    let connections = preset.block_connections.clone();

    run_chain_step(
        0,
        initial_input,
        rect,
        blocks,
        connections,
        config,
        Arc::new(Mutex::new(None)),
        context,
        false,
        None, // No processing window for text presets
        cancel_token,
        preset.id.clone(),
        false,
        chain_id,
        input_hwnd_refocus,
    );
}
