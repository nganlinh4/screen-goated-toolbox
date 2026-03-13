mod host;
mod html;

use std::sync::Arc;

use serde::Serialize;

pub use host::{
    close_selector_for_owner, is_owner_active, post_preview_update_for_owner, show_selector,
    update_theme,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectorOwner {
    ScreenRecord,
    RealtimeAppSelection,
}

impl SelectorOwner {
    pub(crate) fn as_u8(self) -> u8 {
        match self {
            Self::ScreenRecord => 1,
            Self::RealtimeAppSelection => 2,
        }
    }

    pub(crate) fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::ScreenRecord),
            2 => Some(Self::RealtimeAppSelection),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SelectorEntry {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    #[serde(rename = "iconDataUrl")]
    pub icon_data_url: Option<String>,
    #[serde(rename = "previewDataUrl")]
    pub preview_data_url: Option<String>,
    #[serde(rename = "winW")]
    pub width: u32,
    #[serde(rename = "winH")]
    pub height: u32,
    #[serde(rename = "badgeText")]
    pub badge_text: Option<String>,
    pub disabled: bool,
}

#[derive(Clone, Debug)]
pub struct SelectorText {
    pub title: String,
    pub subtitle: String,
    pub count_label: String,
    pub cancel_label: String,
}

pub type SelectCallback = Arc<dyn Fn(String) + Send + Sync + 'static>;
pub type CancelCallback = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Clone)]
pub struct SelectorCallbacks {
    pub on_select: SelectCallback,
    pub on_cancel: CancelCallback,
}

impl SelectorCallbacks {
    pub fn new<FS, FC>(on_select: FS, on_cancel: FC) -> Self
    where
        FS: Fn(String) + Send + Sync + 'static,
        FC: Fn() + Send + Sync + 'static,
    {
        Self {
            on_select: Arc::new(on_select),
            on_cancel: Arc::new(on_cancel),
        }
    }
}
