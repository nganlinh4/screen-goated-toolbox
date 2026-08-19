use crate::overlay::result::ResultPresentation;
use crate::overlay::result::SourceReplacementRegion;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SceneRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneCard {
    pub id: isize,
    pub rect: SceneRect,
    #[serde(default)]
    pub control_rect: SceneRect,
    pub body: String,
    pub document: Option<String>,
    pub refining: bool,
    pub background: String,
    pub opacity: u8,
    pub visible: bool,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub streaming_enabled: bool,
    #[serde(default)]
    pub stack_order: u64,
    #[serde(default)]
    pub controls: SceneControls,
    #[serde(default)]
    pub presentation: ResultPresentation,
    #[serde(default)]
    pub backdrop_data_url: Option<String>,
    #[serde(default)]
    pub foreground_color: Option<String>,
    #[serde(default)]
    pub preferred_font_size: Option<f32>,
    #[serde(default)]
    pub source_replacement: bool,
    #[serde(default)]
    pub source_vertical: bool,
    #[serde(default)]
    pub source_regions: Vec<SourceReplacementRegion>,
    #[serde(default)]
    pub source_segments: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneControls {
    pub hidden: bool,
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
    pub group_ids: Vec<isize>,
    pub onboarding_pulse_token: u8,
}

impl Default for SceneControls {
    fn default() -> Self {
        Self {
            hidden: false,
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
            group_ids: Vec::new(),
            onboarding_pulse_token: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneControlUpdate {
    pub id: isize,
    pub controls: SceneControls,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneGeometry {
    pub id: isize,
    pub rect: SceneRect,
    #[serde(default)]
    pub control_rect: SceneRect,
    pub visible: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneStream {
    pub id: isize,
    pub body: String,
    pub document: Option<String>,
    pub refining: bool,
    pub background: String,
    pub opacity: u8,
    pub visible: bool,
    #[serde(default)]
    pub streaming_enabled: bool,
    #[serde(default)]
    pub controls: SceneControls,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneFinalize {
    pub id: isize,
    pub body: String,
    pub document: Option<String>,
    pub refining: bool,
    pub background: String,
    pub opacity: u8,
    pub visible: bool,
    #[serde(default)]
    pub streaming_enabled: bool,
    #[serde(default)]
    pub controls: SceneControls,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneAppearance {
    pub id: isize,
    pub background: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneTheme {
    pub css: String,
    pub controls_css: String,
    pub cards: Vec<SceneAppearance>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ButtonAction {
    Copy,
    Undo,
    Redo,
    Edit,
    Download,
    Back,
    Forward,
    Speaker,
    SetOpacity { value: u8 },
    SubmitRefine { text: String },
    CancelRefine,
    HistoryUpRefine { text: String },
    HistoryDownRefine { text: String },
    Mic,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DragOutcome {
    Moved,
    CloseOne,
    CloseGroup,
    CloseAll,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostCommand {
    Snapshot {
        cards: Vec<SceneCard>,
    },
    Upsert {
        card: SceneCard,
    },
    Stream {
        card: SceneStream,
    },
    Finalize {
        card: SceneFinalize,
    },
    Geometry {
        cards: Vec<SceneGeometry>,
    },
    Controls {
        cards: Vec<SceneControlUpdate>,
    },
    Opacity {
        id: isize,
        opacity: u8,
    },
    RefineText {
        id: isize,
        text: String,
        is_insert: bool,
    },
    ExternalDrag {
        active: bool,
    },
    Theme {
        theme: SceneTheme,
    },
    Raise {
        id: isize,
        stack_order: u64,
    },
    Remove {
        id: isize,
    },
    NavigateBack {
        id: isize,
    },
    NavigateForward {
        id: isize,
    },
    Shutdown,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChildEvent {
    Ready,
    Heartbeat,
    ResyncRequested,
    RendererFailure {
        kind: RendererFailureKind,
    },
    FontReady {
        duration_ms: f64,
    },
    CardDiagnostic {
        id: isize,
        phase: String,
        revision: u64,
        visible: bool,
        ready: bool,
        payload_len: usize,
        text_len: usize,
        opacity: String,
        error: Option<String>,
    },
    CommandError {
        command: String,
        id: Option<isize>,
        error: String,
    },
    Navigation {
        id: isize,
        depth: usize,
        max_depth: usize,
    },
    Interaction {
        id: isize,
    },
    ButtonAction {
        id: isize,
        action: ButtonAction,
    },
    DragStarted,
    DragFinished {
        id: isize,
        targets: Vec<isize>,
        outcome: DragOutcome,
    },
    FitDiagnostic {
        id: isize,
        payload: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_protocol_round_trips_arbitrary_rendered_content() {
        let command = HostCommand::Upsert {
            card: SceneCard {
                id: 42,
                rect: SceneRect {
                    x: -120,
                    y: 30,
                    width: 640,
                    height: 240,
                },
                control_rect: SceneRect {
                    x: -124,
                    y: 28,
                    width: 648,
                    height: 244,
                },
                body: "<p>line one</p>".to_string(),
                document: Some("<p>line one</p>\n<script>const x = `quoted`;</script>".to_string()),
                refining: false,
                background: "#112233".to_string(),
                opacity: 85,
                visible: true,
                stack_order: 7,
                streaming: false,
                streaming_enabled: false,
                controls: SceneControls::default(),
                presentation: ResultPresentation::Standard,
                backdrop_data_url: None,
                foreground_color: None,
                preferred_font_size: None,
                source_replacement: false,
                source_vertical: false,
                source_regions: Vec::new(),
                source_segments: Vec::new(),
            },
        };

        let encoded = serde_json::to_string(&command).unwrap();
        let decoded: HostCommand = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, command);
        assert!(!encoded.contains('\n'));
    }

    #[test]
    fn geometry_updates_do_not_carry_rendered_html() {
        let command = HostCommand::Geometry {
            cards: vec![SceneGeometry {
                id: 42,
                rect: SceneRect {
                    x: 5,
                    y: 8,
                    width: 900,
                    height: 500,
                },
                control_rect: SceneRect {
                    x: 1,
                    y: 6,
                    width: 908,
                    height: 504,
                },
                visible: true,
            }],
        };

        let encoded = serde_json::to_string(&command).unwrap();
        assert!(!encoded.contains("html"));
        assert_eq!(
            serde_json::from_str::<HostCommand>(&encoded).unwrap(),
            command
        );
    }

    #[test]
    fn stream_updates_only_carry_the_replaceable_body() {
        let command = HostCommand::Stream {
            card: SceneStream {
                id: 42,
                body: "<p>latest words</p>".to_string(),
                document: None,
                refining: false,
                background: "#112233".to_string(),
                opacity: 75,
                visible: true,
                streaming_enabled: true,
                controls: SceneControls::default(),
            },
        };

        let encoded = serde_json::to_string(&command).unwrap();
        assert!(!encoded.contains("__SGT_RUN_FIT__"));
        assert!(encoded.contains("\"document\":null"));
        assert_eq!(
            serde_json::from_str::<HostCommand>(&encoded).unwrap(),
            command
        );
    }

    #[test]
    fn finalization_preserves_the_canonical_document_for_navigation() {
        let command = HostCommand::Finalize {
            card: SceneFinalize {
                id: 42,
                body: "<p>final body</p>".to_string(),
                document: Some("<html><body><p>final body</p></body></html>".to_string()),
                refining: false,
                background: "#112233".to_string(),
                opacity: 90,
                visible: true,
                streaming_enabled: false,
                controls: SceneControls::default(),
            },
        };

        let encoded = serde_json::to_string(&command).unwrap();
        assert_eq!(
            serde_json::from_str::<HostCommand>(&encoded).unwrap(),
            command
        );
    }

    #[test]
    fn card_diagnostics_preserve_lifecycle_evidence() {
        let event = ChildEvent::CardDiagnostic {
            id: 42,
            phase: "document_loaded".to_string(),
            revision: 2,
            visible: true,
            ready: true,
            payload_len: 4096,
            text_len: 321,
            opacity: "1".to_string(),
            error: None,
        };

        let encoded = serde_json::to_string(&event).unwrap();
        assert_eq!(serde_json::from_str::<ChildEvent>(&encoded).unwrap(), event);
    }

    #[test]
    fn font_readiness_crosses_the_process_boundary() {
        let event = ChildEvent::FontReady { duration_ms: 12.5 };
        let encoded = serde_json::to_string(&event).unwrap();

        assert_eq!(serde_json::from_str::<ChildEvent>(&encoded).unwrap(), event);
    }

    #[test]
    fn theme_and_stacking_commands_cross_the_process_boundary() {
        let theme = HostCommand::Theme {
            theme: SceneTheme {
                css: ":root { --text-color: white; }".to_string(),
                controls_css: ":root { --btn-color: white; }".to_string(),
                cards: vec![SceneAppearance {
                    id: 42,
                    background: "#112233".to_string(),
                }],
            },
        };
        let raise = HostCommand::Raise {
            id: 42,
            stack_order: 9,
        };

        for command in [theme, raise] {
            let encoded = serde_json::to_string(&command).unwrap();
            assert_eq!(
                serde_json::from_str::<HostCommand>(&encoded).unwrap(),
                command
            );
        }
    }
}
