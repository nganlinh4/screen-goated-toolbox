use crate::config::ProcessingBlock;
use egui_snarl::Snarl;

use super::super::node_graph::{ChainNode, blocks_to_snarl, snarl_to_graph};

/// Creates a default processing block based on preset type.
pub(super) fn create_default_block_for_type(preset_type: &str) -> ProcessingBlock {
    match preset_type {
        "audio" => ProcessingBlock {
            block_type: "audio".to_string(),
            model: "whisper-accurate".to_string(),
            prompt: "Transcribe this audio.".to_string(),
            selected_language: "Vietnamese".to_string(),
            auto_copy: true,
            ..Default::default()
        },
        "text" => ProcessingBlock {
            block_type: "text".to_string(),
            model: "gemma-4-26b-a4b".to_string(),
            prompt: "Process this text.".to_string(),
            selected_language: "Vietnamese".to_string(),
            auto_copy: true,
            ..Default::default()
        },
        _ => ProcessingBlock {
            block_type: "image".to_string(),
            model: crate::model_config::DEFAULT_IMAGE_MODEL_ID.to_string(),
            prompt: "Extract text from this image.".to_string(),
            selected_language: "Vietnamese".to_string(),
            show_overlay: true,
            auto_copy: true,
            ..Default::default()
        },
    }
}

pub(super) fn sync_graph_type(snarl: &mut Snarl<ChainNode>, preset_type: &str) {
    let (mut blocks, mut connections) = snarl_to_graph(snarl);
    let default_block = create_default_block_for_type(preset_type);

    if let Some(first_process) = blocks
        .iter_mut()
        .find(|block| block.block_type != "input_adapter")
    {
        first_process.block_type = default_block.block_type;
        first_process.model = default_block.model;
    } else {
        let new_idx = blocks.len();
        let input_idx = blocks
            .iter()
            .position(|block| block.block_type == "input_adapter");
        blocks.push(default_block);

        if let Some(input_idx) = input_idx {
            connections.push((input_idx, new_idx));
        }
    }

    *snarl = blocks_to_snarl(&blocks, &connections, preset_type);
}
