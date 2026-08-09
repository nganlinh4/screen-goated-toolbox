use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    pub html: String,
    pub background: String,
    pub opacity: u8,
    pub visible: bool,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub stack_order: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneGeometry {
    pub id: isize,
    pub rect: SceneRect,
    pub visible: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneStream {
    pub id: isize,
    pub body: String,
    pub background: String,
    pub opacity: u8,
    pub visible: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneFinalize {
    pub id: isize,
    pub body: String,
    pub html: String,
    pub background: String,
    pub opacity: u8,
    pub visible: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneAppearance {
    pub id: isize,
    pub background: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneTheme {
    pub css: String,
    pub cards: Vec<SceneAppearance>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostCommand {
    Snapshot { cards: Vec<SceneCard> },
    Upsert { card: SceneCard },
    Stream { card: SceneStream },
    Finalize { card: SceneFinalize },
    Geometry { cards: Vec<SceneGeometry> },
    Theme { theme: SceneTheme },
    Raise { id: isize, stack_order: u64 },
    Remove { id: isize },
    NavigateBack { id: isize },
    NavigateForward { id: isize },
    Shutdown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChildEvent {
    Ready,
    Heartbeat,
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
                html: "<p>line one</p>\n<script>const x = `quoted`;</script>".to_string(),
                background: "#112233".to_string(),
                opacity: 85,
                visible: true,
                stack_order: 7,
                streaming: false,
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
                background: "#112233".to_string(),
                opacity: 75,
                visible: true,
            },
        };

        let encoded = serde_json::to_string(&command).unwrap();
        assert!(!encoded.contains("__SGT_RUN_FIT__"));
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
                html: "<html><body><p>final body</p></body></html>".to_string(),
                background: "#112233".to_string(),
                opacity: 90,
                visible: true,
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
