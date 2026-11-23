use crate::config::ProcessingBlock;
use crate::model_config::{
    DEFAULT_IMAGE_MODEL_ID, DEFAULT_TEXT_MODEL_ID, PRESET_AUDIO_TRANSCRIBE_MODEL_ID,
};
use std::collections::HashMap;

/// Node type for the processing chain
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum ChainNode {
    /// Input node (audio/image/text source)
    Input {
        #[serde(skip)]
        original: Option<Box<ProcessingBlock>>,
        #[serde(skip)]
        source_index: Option<usize>,
        id: String,
        block_type: String, // "audio", "image", "text", "input_adapter"
        auto_copy: bool,
        auto_speak: bool,
        show_overlay: bool,
        render_mode: String,
    },
    /// Special Processing Node (First level processor, Presets)
    Special {
        #[serde(skip)]
        original: Option<Box<ProcessingBlock>>,
        #[serde(skip)]
        source_index: Option<usize>,
        id: String,
        block_type: String,
        model: String,
        prompt: String,
        language_vars: HashMap<String, String>,
        show_overlay: bool,
        streaming_enabled: bool,
        #[serde(default)]
        search_enabled: bool,
        render_mode: String,
        auto_copy: bool,
        auto_speak: bool,
    },
    /// Processing node (transforms text)
    Process {
        #[serde(skip)]
        original: Option<Box<ProcessingBlock>>,
        #[serde(skip)]
        source_index: Option<usize>,
        id: String,
        block_type: String,
        model: String,
        prompt: String,
        language_vars: HashMap<String, String>,
        show_overlay: bool,
        streaming_enabled: bool,
        #[serde(default)]
        search_enabled: bool,
        render_mode: String,
        auto_copy: bool,
        auto_speak: bool,
    },
}

impl Default for ChainNode {
    fn default() -> Self {
        ChainNode::Process {
            original: None,
            source_index: None,
            id: format!(
                "{:x}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
            block_type: "text".to_string(),
            model: DEFAULT_TEXT_MODEL_ID.to_string(),
            prompt: "Translate to {language1}. Output ONLY the translation.".to_string(),
            language_vars: HashMap::new(),
            show_overlay: true,
            streaming_enabled: true,
            search_enabled: false,
            render_mode: "markdown_stream".to_string(),
            auto_copy: false,
            auto_speak: false,
        }
    }
}

impl ChainNode {
    /// A node that reads the preset's image or audio input itself, so it is
    /// that kind of block and starts on that input's default model.
    pub fn new_special(preset_type: &str) -> Self {
        let (kind, default_model) = match preset_type {
            "image" => ("image", DEFAULT_IMAGE_MODEL_ID),
            "audio" => ("audio", PRESET_AUDIO_TRANSCRIBE_MODEL_ID),
            _ => ("text", DEFAULT_TEXT_MODEL_ID),
        };
        let ChainNode::Process {
            original,
            source_index,
            id,
            prompt,
            language_vars,
            show_overlay,
            streaming_enabled,
            search_enabled,
            render_mode,
            auto_copy,
            auto_speak,
            ..
        } = Self::default()
        else {
            unreachable!("the default node is a process node");
        };
        ChainNode::Special {
            original,
            source_index,
            id,
            block_type: kind.to_string(),
            model: default_model.to_string(),
            prompt,
            language_vars,
            show_overlay,
            streaming_enabled,
            search_enabled,
            render_mode,
            auto_copy,
            auto_speak,
        }
    }

    pub fn is_input(&self) -> bool {
        matches!(self, ChainNode::Input { .. })
    }

    pub fn is_special(&self) -> bool {
        matches!(self, ChainNode::Special { .. })
    }

    /// Convert to ProcessingBlock for execution
    pub fn to_block(&self) -> ProcessingBlock {
        let mut block = self.to_visible_block();
        let original = match self {
            ChainNode::Input { original, .. }
            | ChainNode::Special { original, .. }
            | ChainNode::Process { original, .. } => original.as_deref(),
        };
        let Some(original) = original else {
            return block;
        };
        let role = if original.block_type == "input_adapter" {
            "input"
        } else {
            "process"
        };
        let baseline = ChainNode::from_block(original, role).to_visible_block();
        if block == baseline {
            return original.clone();
        }
        if original.block_type == "input_adapter" && block.block_type == "input_adapter" {
            block.model = original.model.clone();
            block.prompt = original.prompt.clone();
            block.selected_language = original.selected_language.clone();
            block.language_vars = original.language_vars.clone();
            block.streaming_enabled = original.streaming_enabled;
            block.search_enabled = original.search_enabled;
        } else if block.language_vars == baseline.language_vars {
            block.language_vars = original.language_vars.clone();
            block.selected_language = original.selected_language.clone();
        } else if block.language_vars.get("language1") == baseline.language_vars.get("language1") {
            block.selected_language = original.selected_language.clone();
            if !original.language_vars.contains_key("language1") {
                block.language_vars.remove("language1");
            }
        }
        block
    }

    fn to_visible_block(&self) -> ProcessingBlock {
        match self {
            ChainNode::Input {
                id,
                block_type: _,
                auto_copy,
                auto_speak,
                show_overlay,
                render_mode,
                ..
            } => {
                ProcessingBlock {
                    id: id.clone(),
                    block_type: "input_adapter".to_string(), // Always adapter for Input Node
                    model: String::new(),
                    prompt: String::new(),
                    selected_language: String::new(),
                    language_vars: HashMap::new(),
                    show_overlay: *show_overlay,
                    streaming_enabled: false,
                    search_enabled: false,
                    render_mode: render_mode.clone(),
                    auto_copy: *auto_copy,
                    auto_speak: *auto_speak,
                }
            }
            ChainNode::Special {
                id,
                block_type,
                model,
                prompt,
                language_vars,
                show_overlay,
                streaming_enabled,
                search_enabled,
                render_mode,
                auto_copy,
                auto_speak,
                ..
            }
            | ChainNode::Process {
                id,
                block_type,
                model,
                prompt,
                language_vars,
                show_overlay,
                streaming_enabled,
                search_enabled,
                render_mode,
                auto_copy,
                auto_speak,
                ..
            } => ProcessingBlock {
                id: id.clone(),
                block_type: block_type.clone(),
                model: model.clone(),
                prompt: prompt.clone(),
                selected_language: language_vars.get("language1").cloned().unwrap_or_default(),
                language_vars: language_vars.clone(),
                show_overlay: *show_overlay,
                streaming_enabled: *streaming_enabled,
                search_enabled: *search_enabled,
                render_mode: render_mode.clone(),
                auto_copy: *auto_copy,
                auto_speak: *auto_speak,
            },
        }
    }

    /// Create from ProcessingBlock
    pub fn from_block(block: &ProcessingBlock, role: &str) -> Self {
        // role: "input", "special", "process"

        // Populate language_vars from selected_language if missing (legacy support)
        let mut language_vars = block.language_vars.clone();
        if !language_vars.contains_key("language1") && !block.selected_language.is_empty() {
            language_vars.insert("language1".to_string(), block.selected_language.clone());
        }

        match role {
            "input" => {
                // For input_adapter blocks: respect the saved show_overlay value
                // For other block types used as input (legacy/virtual): default to false
                // Note: Old presets without explicit show_overlay may show overlay unexpectedly
                // (serde defaults to true), but users can easily disable it via the eye button
                let show_overlay = if block.block_type == "input_adapter" {
                    block.show_overlay
                } else {
                    false
                };

                ChainNode::Input {
                    original: (block.block_type == "input_adapter")
                        .then(|| Box::new(block.clone())),
                    source_index: None,
                    id: block.id.clone(),
                    block_type: block.block_type.clone(),
                    auto_copy: block.auto_copy,
                    auto_speak: block.auto_speak,
                    show_overlay,
                    render_mode: block.render_mode.clone(),
                }
            }
            "special" => ChainNode::Special {
                original: Some(Box::new(block.clone())),
                source_index: None,
                id: block.id.clone(),
                block_type: block.block_type.clone(),
                model: block.model.clone(),
                prompt: block.prompt.clone(),
                language_vars,
                show_overlay: block.show_overlay,
                streaming_enabled: block.streaming_enabled,
                search_enabled: block.search_enabled,
                render_mode: block.render_mode.clone(),
                auto_copy: block.auto_copy,
                auto_speak: block.auto_speak,
            },
            _ => ChainNode::Process {
                original: Some(Box::new(block.clone())),
                source_index: None,
                id: block.id.clone(),
                block_type: block.block_type.clone(),
                model: block.model.clone(),
                prompt: block.prompt.clone(),
                language_vars,
                show_overlay: block.show_overlay,
                streaming_enabled: block.streaming_enabled,
                search_enabled: block.search_enabled,
                render_mode: block.render_mode.clone(),
                auto_copy: block.auto_copy,
                auto_speak: block.auto_speak,
            },
        }
    }

    pub fn id(&self) -> &str {
        match self {
            ChainNode::Input { id, .. }
            | ChainNode::Special { id, .. }
            | ChainNode::Process { id, .. } => id,
        }
    }

    pub fn source_index(&self) -> Option<usize> {
        match self {
            ChainNode::Input { source_index, .. }
            | ChainNode::Special { source_index, .. }
            | ChainNode::Process { source_index, .. } => *source_index,
        }
    }

    pub fn set_source(&mut self, index: usize, block: &ProcessingBlock) {
        match self {
            ChainNode::Input {
                source_index,
                original,
                ..
            }
            | ChainNode::Special {
                source_index,
                original,
                ..
            }
            | ChainNode::Process {
                source_index,
                original,
                ..
            } => {
                *source_index = Some(index);
                *original = Some(Box::new(block.clone()));
            }
        }
    }

    pub fn set_auto_copy(&mut self, val: bool) {
        match self {
            ChainNode::Input { auto_copy, .. } => *auto_copy = val,
            ChainNode::Special { auto_copy, .. } => *auto_copy = val,
            ChainNode::Process { auto_copy, .. } => *auto_copy = val,
        }
    }
}
