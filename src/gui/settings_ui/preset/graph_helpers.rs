use crate::config::ProcessingBlock;
use crate::model_config::{
    DEFAULT_IMAGE_MODEL_ID, DEFAULT_TEXT_MODEL_ID, PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
};
use egui_snarl::Snarl;

use super::super::node_graph::{ChainNode, blocks_to_snarl, save_graph_to_preset};

/// Creates a default processing block based on preset type.
pub(super) fn create_default_block_for_type(preset_type: &str) -> ProcessingBlock {
    match preset_type {
        "audio" => ProcessingBlock {
            block_type: "audio".to_string(),
            model: PRESET_AUDIO_TRANSCRIBE_MODEL_ID.to_string(),
            prompt: "Transcribe this audio.".to_string(),
            selected_language: "Vietnamese".to_string(),
            auto_copy: true,
            ..Default::default()
        },
        "text" => ProcessingBlock {
            block_type: "text".to_string(),
            model: DEFAULT_TEXT_MODEL_ID.to_string(),
            prompt: "Process this text.".to_string(),
            selected_language: "Vietnamese".to_string(),
            auto_copy: true,
            ..Default::default()
        },
        _ => ProcessingBlock {
            block_type: "image".to_string(),
            model: DEFAULT_IMAGE_MODEL_ID.to_string(),
            prompt: "Extract text from this image.".to_string(),
            selected_language: "Vietnamese".to_string(),
            show_overlay: true,
            auto_copy: true,
            ..Default::default()
        },
    }
}

pub(super) fn sync_graph_type(snarl: &mut Snarl<ChainNode>, preset: &mut crate::config::Preset) {
    if !save_graph_to_preset(preset, snarl) {
        return;
    }
    let default_block = create_default_block_for_type(&preset.preset_type);

    if let Some(index) = preset.first_reachable_processing_index() {
        let first_process = &mut preset.blocks[index];
        first_process.block_type = default_block.block_type;
        first_process.model = default_block.model;
    } else {
        let new_idx = preset.blocks.len();
        let input_idx = preset
            .blocks
            .iter()
            .position(|block| block.block_type == "input_adapter");
        preset.blocks.push(default_block);

        if let Some(input_idx) = input_idx {
            preset.block_connections.push((input_idx, new_idx));
        }
    }

    *snarl = blocks_to_snarl(
        &preset.blocks,
        &preset.block_connections,
        true,
        &preset.preset_type,
    );
}
