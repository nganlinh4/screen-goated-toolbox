// --- CHAIN POST-PROCESSING ---
// Copy, paste, speak, history saving, and chain continuation logic.

use crate::config::{Config, ProcessingBlock};
use crate::overlay::result::{ChainCancelToken, RefineContext};
use crate::overlay::text_input;
use crate::win_types::SendHwnd;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

use super::step::run_chain_step;

// A second auto-copy job must not replace the clipboard while the first job
// is focusing its destination. Other applications can still change it, so the
// sequence is checked again immediately before sending paste keys.
static AUTO_COPY_PASTE: Mutex<()> = Mutex::new(());

fn with_auto_copy_lock<T>(action: impl FnOnce() -> T) -> T {
    let _guard = AUTO_COPY_PASTE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    action()
}

fn auto_paste_options(config: &Config, preset_id: &str) -> Option<(bool, bool)> {
    let mut presets = config
        .presets
        .iter()
        .filter(|preset| preset.id == preset_id);
    let preset = presets.next()?;
    presets
        .next()
        .is_none()
        .then_some((preset.auto_paste_newline, preset.auto_paste))
}

struct PreparedAutoCopy {
    text: String,
    options: Option<(bool, bool)>,
}

impl PreparedAutoCopy {
    fn new(mut text: String, options: Option<(bool, bool)>) -> Self {
        if options.is_some_and(|(newline, _)| newline) {
            text.push('\n');
        }
        Self { text, options }
    }

    fn allows_paste(&self, current: Option<(bool, bool)>) -> bool {
        self.options.is_some_and(|(_, paste)| paste) && current == self.options
    }
}

fn current_auto_paste_options(preset_id: &str) -> Option<(bool, bool)> {
    crate::APP
        .lock()
        .ok()
        .and_then(|app| auto_paste_options(&app.config, preset_id))
}

/// Handle auto-copy functionality for a block result.
pub fn handle_auto_copy(
    block: &ProcessingBlock,
    result_text: &str,
    context: &RefineContext,
    preset_id: &str,
    disable_auto_paste: bool,
    destination: super::AutoPasteDestination,
    cancel_token: &Arc<ChainCancelToken>,
) {
    let is_input_adapter = block.block_type == "input_adapter";
    let has_content = !result_text.trim().is_empty();

    if !block.auto_copy {
        return;
    }
    let diagnostic_id = crate::overlay::utils::paste_diagnostics::next_id();

    let image = if is_input_adapter {
        match context {
            RefineContext::Image(data) => Some(data.clone()),
            _ => None,
        }
    } else {
        None
    };
    let image_copy = image.is_some();
    if !has_content && !image_copy {
        return;
    }

    // Only trigger paste for:
    // 1. Non-input_adapter blocks with text content (actual processed results)
    // 2. Image copies from input_adapter (intentional image copy)
    let should_trigger_paste = (has_content && !is_input_adapter) || image_copy;
    if should_trigger_paste && crate::overlay::utils::paste_diagnostics::enabled() {
        crate::log_info!(
            "[AutoPasteBatch] id={diagnostic_id} stage=postprocess preset={preset_id} paste_disabled={disable_auto_paste} image={image_copy} chars={}",
            result_text.chars().count()
        );
    }

    let txt_c = result_text.to_string();
    let preset_id_clone = preset_id.to_string();
    let cancel_token = cancel_token.clone();
    // Do not redirect a queued job to a later job's active window.
    let target_window = if should_trigger_paste && !disable_auto_paste {
        if let super::AutoPasteDestination::External(target) = destination {
            target
        } else {
            None
        }
    } else {
        None
    };
    std::thread::spawn(move || {
        with_auto_copy_lock(|| {
            let prepared =
                PreparedAutoCopy::new(txt_c, current_auto_paste_options(&preset_id_clone));
            let before = crate::overlay::utils::paste_diagnostics::clipboard_sequence();
            let Some(sequence) = cancel_token.run_if_active(|| {
                if let Some(image) = image.as_deref() {
                    crate::overlay::utils::copy_image_with_sequence(image, HWND::default()).ok()
                } else {
                    crate::overlay::utils::copy_text_with_sequence(&prepared.text, HWND::default())
                }
            }) else {
                return;
            };
            if crate::overlay::utils::paste_diagnostics::enabled() {
                crate::log_info!(
                    "[AutoPasteBatch] id={diagnostic_id} stage=copy_done copied={} clipboard_seq_before={before} clipboard_seq_after={} image={image_copy} chars={}",
                    sequence.is_some(),
                    crate::overlay::utils::paste_diagnostics::clipboard_sequence(),
                    prepared.text.chars().count()
                );
            }
            if sequence.is_some() {
                if image_copy && !has_content {
                    crate::overlay::auto_copy_badge::show_auto_copy_badge_image();
                } else if !is_input_adapter {
                    crate::overlay::auto_copy_badge::show_auto_copy_badge_text(&prepared.text);
                }
            }
            if !should_trigger_paste || disable_auto_paste {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            if cancel_token.is_cancelled() {
                return;
            }

            let current_options = current_auto_paste_options(&preset_id_clone);
            let should_paste = prepared.allows_paste(current_options);
            let should_add_newline = prepared.options.is_some_and(|(newline, _)| newline);
            let preset_source = if current_options.is_some() {
                "id"
            } else {
                "missing"
            };
            if crate::overlay::utils::paste_diagnostics::enabled() {
                crate::log_info!(
                    "[AutoPasteBatch] id={diagnostic_id} stage=route should_paste={should_paste} newline={should_add_newline} preset_source={preset_source} clipboard_seq={}",
                    crate::overlay::utils::paste_diagnostics::clipboard_sequence()
                );
                crate::overlay::utils::paste_diagnostics::focus(
                    diagnostic_id,
                    "before_route",
                    target_window.map(|window| window.0),
                );
            }

            if should_paste {
                if image_copy {
                    // Image-only paste path
                    if let (Some(target), Some(sequence)) = (target_window, sequence) {
                        crate::overlay::utils::force_focus_and_paste(
                            target.0,
                            diagnostic_id,
                            sequence,
                            &cancel_token,
                        );
                    }
                } else {
                    // Text paste path
                    if let super::AutoPasteDestination::TextInput(session) = destination {
                        text_input::set_editor_text_for_session(
                            &prepared.text,
                            cancel_token.clone(),
                            session,
                        );
                    } else if let super::AutoPasteDestination::Refine(parent, session) = destination
                    {
                        let _ = cancel_token.run_if_active(|| {
                            crate::overlay::result::insert_refine_text_for_session(
                                parent.0,
                                session,
                                &prepared.text,
                            );
                        });
                    } else if let (Some(target), Some(sequence)) = (target_window, sequence) {
                        if crate::overlay::utils::paste_diagnostics::enabled() {
                            crate::log_info!(
                                "[AutoPasteBatch] id={diagnostic_id} stage=destination external_window"
                            );
                        }
                        crate::overlay::utils::force_focus_and_paste(
                            target.0,
                            diagnostic_id,
                            sequence,
                            &cancel_token,
                        );
                    } else if crate::overlay::utils::paste_diagnostics::enabled() {
                        crate::log_info!(
                            "[AutoPasteBatch] id={diagnostic_id} stage=destination missing_target"
                        );
                    }
                }
            } else if crate::overlay::utils::paste_diagnostics::enabled() {
                crate::log_info!(
                    "[AutoPasteBatch] id={diagnostic_id} stage=destination paste_disabled"
                );
            }
        });
    });
}

/// Handle auto-speak functionality.
/// Pass the result window's HWND so the speaker button correctly reflects TTS state.
pub fn handle_auto_speak(
    block: &ProcessingBlock,
    result_text: &str,
    hwnd: Option<HWND>,
    cancel_token: &Arc<ChainCancelToken>,
) {
    if block.auto_speak && !result_text.trim().is_empty() {
        let txt_s = result_text.to_string();
        let hwnd_key = hwnd.map(|h| h.0 as isize).unwrap_or(0);
        let cancel_token = cancel_token.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = crate::api::tts::TTS_MANAGER.speak_cancellable(&txt_s, hwnd_key, cancel_token);
        });
    }
}

/// Save result to history.
pub fn save_to_history(
    block: &ProcessingBlock,
    result_text: &str,
    input_text: &str,
    context: &RefineContext,
    cancel_token: &Arc<ChainCancelToken>,
) {
    if !block.show_overlay || result_text.trim().is_empty() {
        return;
    }

    let text_for_history = result_text.to_string();

    if block.block_type == "text" {
        let input_text_clone = input_text.to_string();
        let cancel_token = cancel_token.clone();
        std::thread::spawn(move || {
            if let Ok(app) = crate::APP.lock() {
                app.history
                    .save_text_cancellable(text_for_history, input_text_clone, cancel_token);
            }
        });
    } else if block.block_type == "image"
        && let RefineContext::Image(img_bytes) = context
    {
        let img_bytes_clone = img_bytes.clone();
        let cancel_token = cancel_token.clone();
        std::thread::spawn(move || {
            if let Ok(img_dynamic) = image::load_from_memory(&img_bytes_clone) {
                let img_buffer = img_dynamic.to_rgba8();
                if let Ok(app) = crate::APP.lock() {
                    app.history
                        .save_image_cancellable(img_buffer, text_for_history, cancel_token);
                }
            }
        });
    }
}

/// Continue chain to next blocks (graph traversal).
pub struct ContinueChainRequest<'a> {
    pub block_idx: usize,
    pub block: &'a ProcessingBlock,
    pub result_text: String,
    pub blocks: Vec<ProcessingBlock>,
    pub connections: Vec<(usize, usize)>,
    pub graph_explicit: bool,
    pub config: Config,
    pub my_hwnd: Option<HWND>,
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
    pub my_rect: RECT,
    pub starting_rect: RECT,
}

fn next_block_indices(
    block_idx: usize,
    block_count: usize,
    connections: &[(usize, usize)],
    graph_explicit: bool,
) -> Vec<usize> {
    if connections.is_empty() && !graph_explicit {
        return (block_idx + 1 < block_count)
            .then_some(block_idx + 1)
            .into_iter()
            .collect();
    }
    connections
        .iter()
        .filter_map(|&(from, to)| (from == block_idx).then_some(to))
        .collect()
}

pub fn continue_chain(request: ContinueChainRequest<'_>) {
    let ContinueChainRequest {
        block_idx,
        block,
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
    } = request;
    // Check cancellation before continuing
    if cancel_token.is_cancelled() {
        if let Some(h) = processing_indicator_hwnd {
            h.close();
        }
        return;
    }

    // For input_adapter blocks, ALWAYS continue even if result_text is empty
    let should_continue = !result_text.trim().is_empty() || block.block_type == "input_adapter";

    if !should_continue {
        if let Some(h) = processing_indicator_hwnd {
            h.close();
        }
        return;
    }

    let next_blocks = next_block_indices(block_idx, blocks.len(), &connections, graph_explicit);

    if next_blocks.is_empty() {
        if let Some(h) = processing_indicator_hwnd {
            h.close();
        }
        return;
    }

    let next_parent = if my_hwnd.is_some() {
        Arc::new(Mutex::new(my_hwnd.map(SendHwnd)))
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
    let next_preprocessed_audio = if block.block_type == "input_adapter" {
        preprocessed_audio
    } else {
        None
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
        let branch_preprocessed_audio = next_preprocessed_audio.clone();
        let branch_visited_blocks = visited_blocks.clone();
        let branch_rect = base_rect;

        // Incremental delay for each branch
        let delay_ms = (branch_index as u64 + 1) * 300;

        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }

            std::thread::sleep(std::time::Duration::from_millis(delay_ms));

            run_chain_step(super::step::ChainStepRequest {
                block_idx: next_idx_copy,
                input_text: result_clone,
                current_rect: branch_rect,
                blocks: blocks_clone,
                connections: conns_clone,
                graph_explicit,
                config: config_clone,
                parent_hwnd: parent_clone,
                context: branch_context,
                skip_execution: next_skip_execution,
                preprocessed_audio: branch_preprocessed_audio,
                visited_blocks: branch_visited_blocks,
                processing_indicator_hwnd: None,
                cancel_token: branch_token,
                preset_id: preset_id_clone,
                disable_auto_paste,
                auto_paste_destination,
                chain_id: chain_id_clone,
                input_hwnd_refocus: None,
            });
        });
    }

    // Continue with first downstream block on current thread — also gets a child token
    let first_token = if parallel_branches.is_empty() {
        cancel_token // No fork — reuse parent token directly
    } else {
        ChainCancelToken::child(&cancel_token) // Fork — isolate this branch
    };

    run_chain_step(super::step::ChainStepRequest {
        block_idx: first_next,
        input_text: result_text,
        current_rect: base_rect,
        blocks,
        connections,
        graph_explicit,
        config,
        parent_hwnd: next_parent,
        context: next_context,
        skip_execution: next_skip_execution,
        preprocessed_audio: next_preprocessed_audio,
        visited_blocks,
        processing_indicator_hwnd,
        cancel_token: first_token,
        preset_id,
        disable_auto_paste,
        auto_paste_destination,
        chain_id,
        input_hwnd_refocus,
    });
}

#[cfg(test)]
#[path = "post_process/auto_copy_tests.rs"]
mod auto_copy_tests;

#[cfg(test)]
mod graph_routing_tests {
    use super::next_block_indices;

    #[test]
    fn empty_explicit_graph_stops_at_root_but_legacy_chain_continues() {
        assert!(next_block_indices(0, 2, &[], true).is_empty());
        assert_eq!(next_block_indices(0, 2, &[], false), [1]);
        assert_eq!(next_block_indices(0, 3, &[(0, 2), (0, 1)], true), [2, 1]);
    }
}
