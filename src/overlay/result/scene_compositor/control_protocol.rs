use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneControls {
    pub hidden: bool,
    #[serde(default)]
    pub input_passthrough: bool,
    #[serde(default)]
    pub copy_image: bool,
    pub control_anchor: Option<[i32; 4]>,
    pub control_color: Option<String>,
    pub control_scale_percent: u16,
    pub group_actions: bool,
    pub edit_enabled: bool,
    pub copy_success: bool,
    pub has_undo: bool,
    pub has_redo: bool,
    pub nav_depth: usize,
    pub max_nav_depth: usize,
    pub tts_loading: bool,
    pub tts_speaking: bool,
    pub is_browsing: bool,
    pub is_editing: bool,
    pub input_text: String,
    pub opacity_percent: u8,
    /// API model name that produced this result, shown beside the controls.
    pub model_label: String,
    pub group_ids: Vec<isize>,
    pub onboarding_pulse_token: u8,
}

impl Default for SceneControls {
    fn default() -> Self {
        Self {
            hidden: false,
            input_passthrough: false,
            copy_image: false,
            control_anchor: None,
            control_color: None,
            control_scale_percent: 100,
            group_actions: false,
            edit_enabled: true,
            copy_success: false,
            has_undo: false,
            has_redo: false,
            nav_depth: 0,
            max_nav_depth: 0,
            tts_loading: false,
            tts_speaking: false,
            is_browsing: false,
            is_editing: false,
            input_text: String::new(),
            opacity_percent: 100,
            model_label: String::new(),
            group_ids: Vec::new(),
            onboarding_pulse_token: 0,
        }
    }
}
