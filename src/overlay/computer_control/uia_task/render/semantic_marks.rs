use std::collections::HashMap;

use super::super::super::{detector, telemetry, vision_reader};
use super::super::{VISION_SHORT, View, session};

pub(super) fn semantic_filter_detector_marks(
    capture: &session::Capture,
    view: View,
    frame_id: u64,
    first_id: u32,
    boxes: Vec<detector::DetBox>,
) -> Vec<detector::DetBox> {
    if boxes.is_empty() {
        return boxes;
    }
    let marks: Vec<_> = boxes
        .iter()
        .enumerate()
        .map(|(index, item)| (item.cx, item.cy, first_id.saturating_add(index as u32)))
        .collect();
    let ids: Vec<u32> = marks.iter().map(|(_, _, id)| *id).collect();
    let labels = session::encode_view(capture, view, VISION_SHORT, None, None, Some(&marks))
        .and_then(|(jpeg, _)| vision_reader::label_clickable_marks(&jpeg, &ids));
    let labels = match labels {
        Ok(labels) => labels,
        Err(error) => {
            telemetry::typed_error(
                "ERR_UI_DETECTOR_SEMANTIC_FILTER",
                "detector",
                &error.to_string(),
                serde_json::json!({"frame_id": frame_id, "candidate_count": boxes.len()}),
            );
            return Vec::new();
        }
    };
    let mut labels: HashMap<u32, String> = labels.into_iter().collect();
    let before = boxes.len();
    let filtered: Vec<_> = boxes
        .into_iter()
        .enumerate()
        .filter_map(|(index, mut item)| {
            let id = first_id.saturating_add(index as u32);
            item.label = labels.remove(&id);
            item.label.is_some().then_some(item)
        })
        .collect();
    telemetry::event(
        "detector_semantic_filter",
        "detector",
        // Labels are text read from the user's screen, even when a model
        // generated them from marked pixels.
        telemetry::Privacy::UserText,
        serde_json::json!({
            "frame_id": frame_id,
            "before": before,
            "after": filtered.len(),
            "labels": filtered.iter().filter_map(|item| item.label.as_deref()).collect::<Vec<_>>(),
        }),
    );
    filtered
}
