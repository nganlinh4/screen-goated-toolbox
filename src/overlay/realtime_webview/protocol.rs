use serde::{Deserialize, Serialize};

use super::layout::{CardRole, CompositorLayout};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardText {
    pub committed: String,
    pub draft: String,
}

/// A change to a card's text: drop `remove_utf16` UTF-16 code units from the
/// end of the committed text, append `append`, and replace the draft.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub remove_utf16: usize,
    pub append: String,
    pub draft: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSettings {
    pub audio_source: String,
    pub target_language: String,
    pub translation_model: String,
    pub transcription_model: String,
    pub transcription_language: String,
    pub font_size: u32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadState {
    pub active: bool,
    pub title: String,
    pub message: String,
    pub progress: f32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeScene {
    #[serde(default)]
    pub session_id: u64,
    pub active: bool,
    pub layout: CompositorLayout,
    pub transcription: CardText,
    pub translation: CardText,
    pub settings: CardSettings,
    pub tts_enabled: bool,
    pub tts_speed: u32,
    pub rms: f32,
    pub translation_model: String,
    pub download: DownloadState,
    pub is_dark: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostCommand {
    Snapshot {
        scene: Box<RealtimeScene>,
    },
    Layout {
        layout: CompositorLayout,
    },
    Text {
        role: CardRole,
        text: CardText,
    },
    TextEdit {
        role: CardRole,
        edit: TextEdit,
    },
    Settings {
        settings: CardSettings,
    },
    Tts {
        enabled: bool,
        speed: u32,
    },
    Volume {
        rms: f32,
    },
    TranslationModel {
        model: String,
    },
    Download {
        state: DownloadState,
    },
    Theme {
        is_dark: bool,
        font_size: u32,
    },
    Script {
        role: Option<CardRole>,
        script: String,
    },
    Shutdown,
}

pub use crate::overlay::compositor_renderer::RendererFailureKind;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChildEvent {
    Ready,
    Heartbeat,
    ResyncRequested,
    RendererFailure {
        kind: RendererFailureKind,
    },
    LayoutChanged {
        session_id: u64,
        layout: CompositorLayout,
    },
    Input {
        session_id: u64,
        role: CardRole,
        body: String,
        scale: f64,
    },
    Close {
        session_id: u64,
    },
    RendererError {
        source: String,
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_round_trips_unicode_and_virtual_desktop_coordinates() {
        let mut scene = RealtimeScene {
            active: true,
            ..Default::default()
        };
        scene.layout.transcription.x = -800;
        scene.transcription.committed = "Xin chào 한국어".to_string();
        let command = HostCommand::Snapshot {
            scene: Box::new(scene),
        };
        let encoded = serde_json::to_string(&command).unwrap();
        assert_eq!(
            serde_json::from_str::<HostCommand>(&encoded).unwrap(),
            command
        );
    }
}
