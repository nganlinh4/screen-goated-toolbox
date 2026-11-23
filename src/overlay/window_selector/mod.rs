mod host;
mod html;
mod script_queue;

use std::sync::Arc;

use serde::Serialize;

pub use host::is_owner_active;
#[cfg(not(feature = "recorder-worker"))]
pub use host::update_theme;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use host::{
    SelectorGuard, close_selector_for_token, post_preview_update_for_token, show_selector_guarded,
};
#[cfg(feature = "recorder-worker")]
pub use host::{post_preview_update_for_owner, show_selector};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectorOwner {
    ScreenRecord,
    RealtimeAppSelection,
    ScreenRecordAudioAppSelection,
}

impl SelectorOwner {
    pub(crate) fn as_u8(self) -> u8 {
        match self {
            Self::ScreenRecord => 1,
            Self::RealtimeAppSelection => 2,
            Self::ScreenRecordAudioAppSelection => 3,
        }
    }

    pub(crate) fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::ScreenRecord),
            2 => Some(Self::RealtimeAppSelection),
            3 => Some(Self::ScreenRecordAudioAppSelection),
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
    #[serde(rename = "selectionNotice")]
    pub selection_notice: Option<SelectorNotice>,
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SelectorNotice {
    pub title: String,
    pub message: String,
    #[serde(rename = "actionLabel")]
    pub action_label: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_entry_renders_an_accessible_in_selector_dialog() {
        let entry = SelectorEntry {
            id: "42".to_string(),
            title: "Presentation".to_string(),
            subtitle: "example.exe".to_string(),
            icon_data_url: None,
            preview_data_url: None,
            width: 1920,
            height: 1080,
            badge_text: Some("DISPLAY ONLY".to_string()),
            selection_notice: Some(SelectorNotice {
                title: "Use Display Capture".to_string(),
                message: "Choose the complete display for this source.".to_string(),
                action_label: "Back to window list".to_string(),
            }),
            disabled: false,
        };
        let text = SelectorText {
            title: "Select a window".to_string(),
            subtitle: "Escape to cancel".to_string(),
            count_label: "1 window".to_string(),
            cancel_label: "Cancel".to_string(),
        };

        let generated = html::generate_html(&[entry], "", true, &text);

        assert!(generated.contains("class=\"notice-layer\""));
        assert!(generated.contains("role=\"dialog\" aria-modal=\"true\""));
        assert!(!generated.contains("class=\"notice-rail\""));
        assert!(generated.contains("--notice-action: #2f6fca;"));
        assert!(generated.contains("background:var(--notice-action)"));
        assert!(generated.contains("if(entry.selectionNotice) openNotice(entry,card);"));
        assert!(generated.contains("\"actionLabel\":\"Back to window list\""));
    }

    #[test]
    fn foreign_window_titles_cannot_change_how_the_script_element_ends() {
        let title = "<!--<script> & </SCRIPT> \u{2028}";
        let entry = SelectorEntry {
            id: "7".to_string(),
            title: title.to_string(),
            subtitle: "</script><script>alert(1)</script>".to_string(),
            icon_data_url: None,
            preview_data_url: None,
            width: 640,
            height: 480,
            badge_text: None,
            selection_notice: None,
            disabled: false,
        };
        let text = SelectorText {
            title: "Select a window".to_string(),
            subtitle: String::new(),
            count_label: String::new(),
            cancel_label: "Cancel".to_string(),
        };

        let generated = html::generate_html(&[entry], "", true, &text);
        let data = generated
            .split_once("var entries=")
            .and_then(|(_, rest)| rest.split_once(";\n"))
            .map(|(json, _)| json)
            .expect("inline entries data");

        // `<!--` followed by `<script` would keep the parser from ever closing
        // the element, so no markup-significant byte may reach the raw data.
        assert!(!data.contains(['<', '>', '&', '\u{2028}']), "{data}");
        let parsed: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(parsed[0]["title"], title);
        assert_eq!(generated.matches("</script>").count(), 1);
    }
}
