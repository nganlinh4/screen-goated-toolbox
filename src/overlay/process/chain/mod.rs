// --- CHAIN PIPELINE MODULE ---
// Processing chain execution with graph-based block connections.

mod audio_route;
mod destination;
mod execution;
mod path_state;
mod post_process;
mod step;
mod templates;
mod visibility;

pub(crate) use audio_route::{initial_audio_blocks, streaming_audio_result};
pub use destination::AutoPasteDestination;
pub use step::{ChainStepRequest, run_chain_step};

use crate::config::{Config, Preset};
use crate::overlay::result::{ChainCancelToken, RefineContext};
use crate::win_types::SendHwnd;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::types::{generate_chain_id, get_rect_with_saved_geometry};
use super::window::create_processing_window;

#[derive(Clone, Copy)]
pub struct ProcessingIndicator {
    pub hwnd: SendHwnd,
    pub recording_session_id: Option<usize>,
    pub recording: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum IndicatorCloseRoute {
    Ordinary,
    Recording(usize),
    Ignore,
}

fn indicator_close_route(recording: bool, session_id: Option<usize>) -> IndicatorCloseRoute {
    match (recording, session_id) {
        (false, _) => IndicatorCloseRoute::Ordinary,
        (true, Some(id)) => IndicatorCloseRoute::Recording(id),
        (true, None) => IndicatorCloseRoute::Ignore,
    }
}

impl ProcessingIndicator {
    pub fn ordinary(hwnd: SendHwnd) -> Self {
        Self {
            hwnd,
            recording_session_id: None,
            recording: false,
        }
    }

    pub fn recording(hwnd: SendHwnd, session_id: Option<usize>) -> Self {
        Self {
            hwnd,
            recording_session_id: session_id,
            recording: true,
        }
    }

    pub fn close(self) {
        match indicator_close_route(self.recording, self.recording_session_id) {
            IndicatorCloseRoute::Recording(id) => {
                crate::overlay::recording::close_recording_session(self.hwnd.0, id);
            }
            IndicatorCloseRoute::Ordinary => unsafe {
                let _ = PostMessageW(Some(self.hwnd.0), WM_CLOSE, WPARAM(0), LPARAM(0));
            },
            IndicatorCloseRoute::Ignore => {}
        }
    }
}

// --- CORE PIPELINE LOGIC ---

/// Execute a chain pipeline with a new processing window.
pub fn execute_chain_pipeline(
    initial_input: String,
    rect: RECT,
    config: Config,
    preset: Preset,
    context: RefineContext,
    auto_paste_destination: AutoPasteDestination,
) {
    // Apply saved geometry so processing window appears at the same position as result window
    let corrected_rect = get_rect_with_saved_geometry(&preset.id, rect);

    // 1. Create Processing Window (Gradient Glow)
    let processing_hwnd = unsafe { create_processing_window(corrected_rect) };
    unsafe {
        let _ = SendMessageW(processing_hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
    }

    // 2. Start the chain execution on a BACKGROUND thread
    let conf_clone = config.clone();
    let blocks = preset.blocks.clone();
    let connections = preset.block_connections.clone();
    let graph_explicit = preset.graph_explicit;
    let preset_id = preset.id.clone();

    let processing_hwnd_send = SendHwnd(processing_hwnd);
    std::thread::spawn(move || {
        let chain_id = generate_chain_id();

        run_chain_step(ChainStepRequest {
            block_idx: 0,
            input_text: initial_input,
            current_rect: corrected_rect,
            blocks,
            connections,
            graph_explicit,
            config: conf_clone,
            parent_hwnd: Arc::new(Mutex::new(None)),
            context,
            skip_execution: false,
            preprocessed_audio: None,
            visited_blocks: Vec::new(),
            processing_indicator_hwnd: Some(ProcessingIndicator::ordinary(processing_hwnd_send)),
            cancel_token: ChainCancelToken::new(),
            preset_id,
            disable_auto_paste: false,
            auto_paste_destination,
            chain_id,
            input_hwnd_refocus: None,
        });
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
pub struct ChainPipelineRequest {
    pub initial_input: String,
    pub rect: RECT,
    pub config: Config,
    pub preset: Preset,
    pub context: RefineContext,
    pub cancel_token: Arc<ChainCancelToken>,
    pub input_hwnd_refocus: Option<(SendHwnd, u64)>,
    pub chain_id: String,
    pub auto_paste_destination: AutoPasteDestination,
}

pub fn execute_chain_pipeline_with_token(request: ChainPipelineRequest) {
    let ChainPipelineRequest {
        initial_input,
        rect,
        config,
        preset,
        context,
        cancel_token,
        input_hwnd_refocus,
        chain_id,
        auto_paste_destination,
    } = request;
    let blocks = preset.blocks.clone();
    let connections = preset.block_connections.clone();
    let graph_explicit = preset.graph_explicit;

    run_chain_step(ChainStepRequest {
        block_idx: 0,
        input_text: initial_input,
        current_rect: rect,
        blocks,
        connections,
        graph_explicit,
        config,
        parent_hwnd: Arc::new(Mutex::new(None)),
        context,
        skip_execution: false,
        preprocessed_audio: None,
        visited_blocks: Vec::new(),
        processing_indicator_hwnd: None,
        cancel_token,
        preset_id: preset.id.clone(),
        disable_auto_paste: false,
        auto_paste_destination,
        chain_id,
        input_hwnd_refocus,
    });
}

#[cfg(test)]
mod indicator_tests {
    use super::{IndicatorCloseRoute, indicator_close_route};

    #[test]
    fn recording_closes_are_tagged_and_missing_identity_never_falls_back() {
        assert_eq!(
            indicator_close_route(true, Some(17)),
            IndicatorCloseRoute::Recording(17)
        );
        assert_eq!(
            indicator_close_route(true, None),
            IndicatorCloseRoute::Ignore
        );
        assert_eq!(
            indicator_close_route(false, None),
            IndicatorCloseRoute::Ordinary
        );
    }
}
