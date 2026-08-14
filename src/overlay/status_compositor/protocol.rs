use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct StatusSnapshot {
    pub is_dark: bool,
    pub recording: Option<RecordingScene>,
    pub progress: Option<ProgressScene>,
    pub notification_rect: PhysicalRect,
    pub selection: SelectionScene,
    pub notifications: Vec<NotificationScene>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RecordingScene {
    pub rect: PhysicalRect,
    pub visible: bool,
    pub state: String,
    pub rms: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProgressScene {
    pub order: u64,
    pub title: String,
    pub snippet: String,
    pub progress: f32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SelectionScene {
    pub rect: PhysicalRect,
    pub text_visible: bool,
    pub image_visible: bool,
    pub capture_visible: bool,
    pub selecting: bool,
    pub text: String,
    pub image_text: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NotificationScene {
    pub id: u64,
    pub title: String,
    pub snippet: String,
    pub kind: String,
    pub duration_ms: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostCommand {
    Snapshot {
        scene: StatusSnapshot,
    },
    Theme {
        is_dark: bool,
    },
    RecordingPrepare {
        scene: RecordingScene,
    },
    RecordingShow {
        rect: PhysicalRect,
    },
    RecordingUpdate {
        state: String,
        rms: f32,
    },
    RecordingHide,
    NotificationAdd {
        rect: PhysicalRect,
        notification: NotificationScene,
    },
    ProgressUpsert {
        rect: PhysicalRect,
        progress: ProgressScene,
    },
    ProgressRemove,
    SelectionShow {
        rect: PhysicalRect,
        text: String,
    },
    SelectionHide,
    SelectionUpdate {
        selecting: bool,
        text: String,
    },
    SelectionPosition {
        rect: PhysicalRect,
    },
    ImageBadgeShow {
        rect: PhysicalRect,
        text: String,
    },
    ImageBadgeHide,
    SelectionCapture {
        visible: bool,
        request_id: u64,
    },
    Shutdown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChildEvent {
    Ready,
    Heartbeat,
    RecordingReady,
    RecordingPauseToggle,
    RecordingCancel,
    RecordingMoved { rect: PhysicalRect },
    NotificationFinished { through_id: u64 },
    SelectionCaptureApplied { request_id: u64 },
    ResyncRequested,
    RendererFailure { kind: RendererFailureKind },
    RendererError { source: String, error: String },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererFailureKind {
    BrowserProcessExited,
    RenderProcessExited,
    RenderProcessUnresponsive,
    FrameRenderProcessExited,
    GpuProcessExited,
}

impl RendererFailureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BrowserProcessExited => "browser process exited",
            Self::RenderProcessExited => "render process exited",
            Self::RenderProcessUnresponsive => "render process unresponsive",
            Self::FrameRenderProcessExited => "frame render process exited",
            Self::GpuProcessExited => "GPU process exited",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_round_trips_unicode_and_negative_coordinates() {
        let command = HostCommand::SelectionShow {
            rect: PhysicalRect {
                x: -1200,
                y: 80,
                width: 240,
                height: 140,
            },
            text: "Bôi đen văn bản…".to_string(),
        };
        let encoded = serde_json::to_string(&command).unwrap();
        assert_eq!(
            serde_json::from_str::<HostCommand>(&encoded).unwrap(),
            command
        );
    }
}
