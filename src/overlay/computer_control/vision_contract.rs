//! Shared request and image contracts for auxiliary visual grounding.
//!
//! Production Computer Control and the catalog benchmark both use these
//! primitives so a benchmark protocol cannot silently drift from the app.

use std::io::Cursor;

use anyhow::{Context, Result};

pub(crate) const CONTROL_VISION_SHORT_EDGE: u32 = 1600;
pub(crate) const GROUNDING_STREAMING_ENABLED: bool = false;
pub(crate) const MIN_VERIFICATION_CONFIDENCE: u64 = 70;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GroundingRequest {
    pub prompt: String,
    pub response_schema: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VerificationDecision {
    pub matches: bool,
    pub confidence: u64,
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GroundingPoint {
    pub id: Option<String>,
    pub x: f64,
    pub y: f64,
    pub label: String,
}

pub(crate) fn point_request(description: &str, context: &str) -> GroundingRequest {
    GroundingRequest {
        prompt: format!(
            "{}Find this visible target in the image: {description}\n\
             Output exactly one line and nothing else:\n\
             M|target|x|y|short visible label\n\
             x and y are integer CENTER coordinates on a 0-1000 grid (x left to \
             right, y top to bottom). If it is not visible, output N|target. \
             Do not use markdown or add a second line.",
            context_prefix(context)
        ),
        response_schema: None,
    }
}

pub(crate) fn marks_request(description: &str, context: &str) -> GroundingRequest {
    GroundingRequest {
        prompt: format!(
            "{}Map every distinct visible actionable target relevant to: {description}\n\
             Output only records in reading order, at most 30:\n\
             M|short visible label|x|y\n\
             x and y are integer CENTER coordinates on a 0-1000 grid (x left to \
             right, y top to bottom). Use one record per target. If none are \
             visible, output N. Do not use markdown, prose, or duplicate points.",
            context_prefix(context)
        ),
        response_schema: None,
    }
}

pub(crate) fn drag_request(
    from_description: &str,
    to_description: &str,
    context: &str,
) -> GroundingRequest {
    GroundingRequest {
        prompt: format!(
            "{}Locate both drag endpoints in this same image.\n\
             Start: {from_description}\nDestination: {to_description}\n\
             Output exactly two lines and nothing else:\n\
             M|from|x|y|short visible label\n\
             M|to|x|y|short visible label\n\
             x and y are integer CENTER coordinates on a 0-1000 grid (x left to \
             right, y top to bottom). If an endpoint is not visible, output only \
             N|from or N|to for that missing endpoint. Do not use markdown.",
            context_prefix(context)
        ),
        response_schema: None,
    }
}

pub(crate) fn verification_request(description: &str, context: &str) -> GroundingRequest {
    GroundingRequest {
        prompt: format!(
            "{}The red crosshair marks a proposed click. Requested target: {description}. \
             Output ONLY JSON {{\"matches\": <bool>, \"confidence\": <0-100 int>, \"what\": \"<what the crosshair is on>\"}}. \
             matches is true only if the CROSSHAIR CENTER is visibly inside the requested target; merely seeing the target \
             elsewhere in the crop is false.",
            context_prefix(context)
        ),
        response_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "matches": {"type": "boolean"},
                "confidence": {"type": "integer", "minimum": 0, "maximum": 100},
                "what": {"type": "string", "minLength": 1, "maxLength": 160}
            },
            "required": ["matches", "confidence", "what"],
            "additionalProperties": false
        })),
    }
}

pub(crate) fn parse_named_grounding_records(
    response: &str,
    expected_ids: &[&str],
) -> Option<Vec<GroundingPoint>> {
    if expected_ids.is_empty() || grounding_reports_not_visible(response, expected_ids) {
        return None;
    }
    let mut points = Vec::with_capacity(expected_ids.len());
    for line in strict_record_lines(response)? {
        let fields = record_fields(line);
        if fields.len() != 5 || fields[0] != "M" {
            return None;
        }
        let id = fields[1];
        if !expected_ids.contains(&id)
            || points
                .iter()
                .any(|point: &GroundingPoint| point.id.as_deref() == Some(id))
        {
            return None;
        }
        points.push(GroundingPoint {
            id: Some(id.to_string()),
            x: parse_grid_coordinate(fields[2])?,
            y: parse_grid_coordinate(fields[3])?,
            label: parse_label(fields[4])?,
        });
    }
    if points.len() != expected_ids.len()
        || expected_ids
            .iter()
            .any(|id| !points.iter().any(|point| point.id.as_deref() == Some(id)))
    {
        return None;
    }
    Some(points)
}

pub(crate) fn parse_open_grounding_records(response: &str) -> Option<Vec<GroundingPoint>> {
    if response.trim() == "N" {
        return Some(Vec::new());
    }
    let lines = strict_record_lines(response)?;
    if lines.len() > 30 {
        return None;
    }
    let mut points = Vec::with_capacity(lines.len());
    for line in lines {
        let fields = record_fields(line);
        if fields.len() != 4 || fields[0] != "M" {
            return None;
        }
        let point = GroundingPoint {
            id: None,
            label: parse_label(fields[1])?,
            x: parse_grid_coordinate(fields[2])?,
            y: parse_grid_coordinate(fields[3])?,
        };
        if points.iter().any(|existing: &GroundingPoint| {
            let dx = existing.x - point.x;
            let dy = existing.y - point.y;
            dx * dx + dy * dy < 100.0
        }) {
            return None;
        }
        points.push(point);
    }
    Some(points)
}

pub(crate) fn grounding_reports_not_visible(response: &str, expected_ids: &[&str]) -> bool {
    let Some(lines) = strict_record_lines(response) else {
        return false;
    };
    if lines.len() == 1 {
        let fields = record_fields(lines[0]);
        if fields.as_slice() == ["N"] {
            return true;
        }
    }
    if expected_ids.is_empty() || lines.len() != expected_ids.len() {
        return false;
    }
    let mut seen_ids = Vec::with_capacity(expected_ids.len());
    let mut missing_count = 0;
    for line in lines {
        let fields = record_fields(line);
        let id = match fields.as_slice() {
            ["N", id] if expected_ids.contains(id) => {
                missing_count += 1;
                *id
            }
            ["M", id, x, y, label]
                if expected_ids.contains(id)
                    && parse_grid_coordinate(x).is_some()
                    && parse_grid_coordinate(y).is_some()
                    && parse_label(label).is_some() =>
            {
                *id
            }
            _ => return false,
        };
        if seen_ids.contains(&id) {
            return false;
        }
        seen_ids.push(id);
    }
    missing_count > 0
        && expected_ids
            .iter()
            .all(|expected| seen_ids.contains(expected))
}

fn strict_record_lines(response: &str) -> Option<Vec<&str>> {
    let trimmed = response.trim();
    if trimmed.is_empty() || trimmed.contains("```") {
        return None;
    }
    let lines = trimmed.lines().map(str::trim).collect::<Vec<_>>();
    (!lines.is_empty() && lines.iter().all(|line| !line.is_empty())).then_some(lines)
}

fn record_fields(line: &str) -> Vec<&str> {
    let mut fields = line.split('|').map(str::trim).collect::<Vec<_>>();
    if fields.last() == Some(&"") {
        fields.pop();
    }
    fields
}

fn parse_grid_coordinate(value: &str) -> Option<f64> {
    let parsed = value.parse::<u16>().ok()?;
    (parsed <= 1000).then_some(f64::from(parsed))
}

fn parse_label(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.chars().count() <= 160).then(|| value.to_string())
}

pub(crate) fn context_prefix(context: &str) -> String {
    let context = context.trim();
    if context.is_empty() {
        String::new()
    } else {
        format!("Context (for disambiguation only; do not echo it): {context}\n")
    }
}

pub(crate) fn resize_to_short_edge(
    mut image: image::DynamicImage,
    max_short: u32,
) -> image::DynamicImage {
    let short = image.width().min(image.height());
    if short > max_short {
        let scale = max_short as f32 / short as f32;
        let width = (image.width() as f32 * scale).round().max(1.0) as u32;
        let height = (image.height() as f32 * scale).round().max(1.0) as u32;
        image = image.resize(width, height, image::imageops::FilterType::Triangle);
    }
    image
}

pub(crate) fn encode_jpeg(image: &image::DynamicImage) -> Result<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .context("jpeg encode")?;
    Ok(output.into_inner())
}

pub(crate) fn crosshair_crop(jpeg: &[u8], x_1000: f64, y_1000: f64) -> Result<Vec<u8>> {
    let source = image::load_from_memory(jpeg)?.to_rgb8();
    let (width, height) = source.dimensions();
    let target_x = (x_1000 / 1000.0 * f64::from(width)).round() as i64;
    let target_y = (y_1000 / 1000.0 * f64::from(height)).round() as i64;
    let crop_width = (width / 4).max(240).min(width);
    let crop_height = (height / 4).max(180).min(height);
    let left = (target_x - i64::from(crop_width) / 2)
        .clamp(0, i64::from(width.saturating_sub(crop_width))) as u32;
    let top = (target_y - i64::from(crop_height) / 2)
        .clamp(0, i64::from(height.saturating_sub(crop_height))) as u32;
    let mut crop =
        image::imageops::crop_imm(&source, left, top, crop_width, crop_height).to_image();
    let center_x =
        (target_x - i64::from(left)).clamp(0, i64::from(crop_width.saturating_sub(1))) as u32;
    let center_y =
        (target_y - i64::from(top)).clamp(0, i64::from(crop_height.saturating_sub(1))) as u32;
    let red = image::Rgb([255, 32, 32]);
    for offset in 4..=14 {
        if let Some(x) = center_x.checked_sub(offset) {
            crop.put_pixel(x, center_y, red);
        }
        if center_x + offset < crop_width {
            crop.put_pixel(center_x + offset, center_y, red);
        }
        if let Some(y) = center_y.checked_sub(offset) {
            crop.put_pixel(center_x, y, red);
        }
        if center_y + offset < crop_height {
            crop.put_pixel(center_x, center_y + offset, red);
        }
    }
    encode_jpeg(&image::DynamicImage::ImageRgb8(crop))
}

pub(crate) fn parse_verification(response: &str) -> Option<VerificationDecision> {
    let value: serde_json::Value = serde_json::from_str(response.trim()).ok()?;
    let object = value.as_object()?;
    if object.len() != 3
        || !object.contains_key("matches")
        || !object.contains_key("confidence")
        || !object.contains_key("what")
    {
        return None;
    }
    let note = value.get("what").and_then(serde_json::Value::as_str)?;
    if note.trim().is_empty() || note.chars().count() > 160 {
        return None;
    }
    Some(VerificationDecision {
        matches: value.get("matches").and_then(serde_json::Value::as_bool)?,
        confidence: value
            .get("confidence")
            .and_then(serde_json::Value::as_u64)
            .filter(|confidence| *confidence <= 100)?,
        note: Some(note.trim().to_string()),
    })
}

pub(crate) fn response_reports_not_visible(response: &str) -> bool {
    let (Some(start), Some(end)) = (response.find('{'), response.rfind('}')) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&response[start..=end])
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(str::to_string))
        .is_some_and(|error| !error.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_and_verification_contracts_are_fail_closed() {
        let point = point_request("Save button", "Save the document");
        assert!(point.prompt.contains("Context (for disambiguation only"));
        assert!(point.prompt.contains("M|target|x|y|short visible label"));
        assert!(point.prompt.contains("N|target"));
        assert!(point.response_schema.is_none());

        let verification = verification_request("Save button", "Save the document");
        assert!(verification.prompt.contains("CROSSHAIR CENTER"));
        assert_eq!(
            verification.response_schema.unwrap()["required"],
            serde_json::json!(["matches", "confidence", "what"])
        );
        assert!(
            parse_verification(r#"{"matches":true,"confidence":91,"what":"Save button"}"#)
                .is_some()
        );
        assert!(
            parse_verification(r#"prose {"matches":true,"confidence":91,"what":"Save button"}"#)
                .is_none()
        );
        assert!(
            parse_verification(r#"{"matches":true,"confidence":101,"what":"Save button"}"#)
                .is_none()
        );
        assert!(
            parse_verification(
                r#"{"matches":true,"confidence":91,"what":"Save button","extra":1}"#
            )
            .is_none()
        );
    }

    #[test]
    fn production_frame_preparation_never_upscales() {
        let small = resize_to_short_edge(
            image::DynamicImage::new_rgb8(320, 200),
            CONTROL_VISION_SHORT_EDGE,
        );
        let bytes = encode_jpeg(&small).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (320, 200));

        let large = resize_to_short_edge(
            image::DynamicImage::new_rgb8(2400, 1800),
            CONTROL_VISION_SHORT_EDGE,
        );
        let bytes = encode_jpeg(&large).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2133, 1600));
    }
}
