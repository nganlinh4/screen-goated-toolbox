//! Final task review artifacts and cumulative click overlay.

use super::render::window_view;
use super::vision::{VISION_SHORT, read_view};
use super::*;

fn read_click_points(dir: &str) -> Vec<(i32, i32)> {
    let mut points = Vec::new();
    let path = std::path::Path::new(dir).join("clicks.jsonl");
    if let Ok(contents) = std::fs::read_to_string(path) {
        for line in contents.lines() {
            if let Ok(value) = serde_json::from_str::<Value>(line)
                && let Some(point) = value.get("screen_px").and_then(Value::as_array)
                && point.len() == 2
            {
                points.push((
                    point[0].as_i64().unwrap_or(0) as i32,
                    point[1].as_i64().unwrap_or(0) as i32,
                ));
            }
        }
    }
    points
}

pub(super) fn final_review(dir: &str, target: Option<&str>, task: &str, note: &str) {
    use super::super::telemetry::{self, Privacy};
    let view = window_view(target, false);
    let reading = read_view(
        view,
        "Assess the final visible state against the requested task. State the exact on-screen \
evidence that supports completion, plus any contradiction, modal interruption, blocker, or \
unfinished state. Do not propose new actions. Keep the report concise and factual.",
        &format!("task: {task}"),
        &AtomicBool::new(false),
    )
    .unwrap_or_else(|error| format!("(vision read failed: {error})"));
    let review_id = telemetry::next_artifact_id();
    let text_name = format!("final-review-{review_id:06}.txt");
    let text_path = std::path::Path::new(dir).join(&text_name);
    match std::fs::write(
        &text_path,
        format!("TASK: {task}\nNOTE: {note}\n\nFINAL VISION READING:\n{reading}\n"),
    ) {
        Ok(()) => telemetry::event(
            "final_review_ready",
            "artifact",
            Privacy::UserText,
            json!({"review_id": review_id, "artifact_path": text_name}),
        ),
        Err(error) => telemetry::artifact_write_failed("final_review", &text_path, None, &error),
    }
    eprintln!("[cc] FINAL REVIEW ({note}):\n{reading}");

    let frame_id = telemetry::next_frame_for("final_review_clicks", None);
    if let Err(error) = save_click_frame(dir, view, frame_id) {
        telemetry::typed_error(
            "ERR_FINAL_REVIEW_FRAME_FAILED",
            "capture",
            "failed to build the cumulative final-review frame",
            json!({"frame_id": frame_id, "error": error.to_string()}),
        );
    }
}

fn save_click_frame(dir: &str, view: View, frame_id: u64) -> Result<()> {
    use super::super::telemetry::{self, Privacy};
    let cap = session::capture_virtual().context("capture")?;
    let (jpeg, clamped) =
        session::encode_view(&cap, view, VISION_SHORT, None, None, None).context("encode")?;
    let image = image::load_from_memory(&jpeg).context("decode")?;
    let mut rgb = image.to_rgb8();
    for (screen_x, screen_y) in read_click_points(dir) {
        let x = ((screen_x - clamped.x) as f64 / clamped.w.max(1) as f64 * rgb.width() as f64)
            .round() as i32;
        let y = ((screen_y - clamped.y) as f64 / clamped.h.max(1) as f64 * rgb.height() as f64)
            .round() as i32;
        super::super::grid::draw_click_marker(&mut rgb, x, y);
    }
    let mut buffer = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rgb)
        .write_to(&mut buffer, image::ImageFormat::Jpeg)
        .context("re-encode")?;
    let bytes = buffer.into_inner();
    let image_name = format!("final-clicks-frame-{frame_id:06}.jpg");
    let image_path = std::path::Path::new(dir).join(&image_name);
    std::fs::write(&image_path, &bytes).context("write")?;
    telemetry::event(
        "frame_ready",
        "capture",
        Privacy::Safe,
        json!({
            "frame_id": frame_id,
            "reason": "final_review_clicks",
            "byte_count": bytes.len(),
            "artifact_path": image_name,
            "artifact_write_ok": true,
            "view": [clamped.x, clamped.y, clamped.w, clamped.h],
        }),
    );
    Ok(())
}
