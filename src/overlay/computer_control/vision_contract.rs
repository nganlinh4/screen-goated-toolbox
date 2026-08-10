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
             Output exactly one JSON object with only the keys \"points\" and \
             \"missing\". When visible, use \
             {{\"points\":[{{\"id\":\"target\",\"x\":123,\"y\":456,\"label\":\"short visible label\"}}],\"missing\":[]}}. \
             When not visible, use {{\"points\":[],\"missing\":[\"target\"]}}. \
             x and y are integer CENTER coordinates on a 0-1000 grid (x left \
             to right, y top to bottom). Do not add keys, prose, or Markdown.",
            context_prefix(context)
        ),
        response_schema: Some(named_points_schema(&["target"])),
    }
}

pub(crate) fn marks_request(description: &str, context: &str) -> GroundingRequest {
    GroundingRequest {
        prompt: format!(
            "{}Map every distinct visible actionable target relevant to: {description}\n\
             Output exactly one JSON object with only the key \"points\": \
             {{\"points\":[{{\"x\":123,\"y\":456,\"label\":\"short visible label\"}}]}}. \
             Return points in reading order with at most 30 entries. x and y are \
             integer CENTER coordinates on a 0-1000 grid (x left to right, y \
             top to bottom). Use one point per target and an empty points array \
             when none are visible. Do not add keys, prose, or Markdown.",
            context_prefix(context)
        ),
        response_schema: Some(open_points_schema()),
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
             Output exactly one JSON object with only the keys \"points\" and \
             \"missing\". A fully visible result is \
             {{\"points\":[{{\"id\":\"from\",\"x\":123,\"y\":456,\"label\":\"short start label\"}},{{\"id\":\"to\",\"x\":789,\"y\":654,\"label\":\"short destination label\"}}],\"missing\":[]}}. \
             Put each non-visible endpoint ID in missing instead of points. x \
             and y are integer CENTER coordinates on a 0-1000 grid (x left to \
             right, y top to bottom). Do not add keys, prose, or Markdown.",
            context_prefix(context)
        ),
        response_schema: Some(named_points_schema(&["from", "to"])),
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
    if expected_ids.is_empty() {
        return None;
    }
    let (points, missing) = parse_named_points_object(response, expected_ids)?;
    if !missing.is_empty() {
        return None;
    }
    Some(points)
}

pub(crate) fn parse_open_grounding_records(response: &str) -> Option<Vec<GroundingPoint>> {
    let value = parse_json_response(response)?;
    let object = exact_object(&value, &["points"])?;
    let values = object.get("points")?.as_array()?;
    if values.len() > 30 {
        return None;
    }
    let mut points = Vec::with_capacity(values.len());
    for value in values {
        let point = parse_point_object(value, false)?;
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
    !expected_ids.is_empty()
        && parse_named_points_object(response, expected_ids)
            .is_some_and(|(_, missing)| !missing.is_empty())
}

fn parse_named_points_object(
    response: &str,
    expected_ids: &[&str],
) -> Option<(Vec<GroundingPoint>, Vec<String>)> {
    let value = parse_json_response(response)?;
    let object = exact_object(&value, &["points", "missing"])?;
    let point_values = object.get("points")?.as_array()?;
    let missing_values = object.get("missing")?.as_array()?;
    if point_values.len() + missing_values.len() != expected_ids.len() {
        return None;
    }
    let mut points = Vec::with_capacity(point_values.len());
    let mut seen = Vec::with_capacity(expected_ids.len());
    for value in point_values {
        let point = parse_point_object(value, true)?;
        let id = point.id.as_deref()?;
        if !expected_ids.contains(&id) || seen.iter().any(|seen_id| seen_id == id) {
            return None;
        }
        seen.push(id.to_string());
        points.push(point);
    }
    let mut missing = Vec::with_capacity(missing_values.len());
    for value in missing_values {
        let id = value.as_str()?;
        if !expected_ids.contains(&id) || seen.iter().any(|seen_id| seen_id == id) {
            return None;
        }
        seen.push(id.to_string());
        missing.push(id.to_string());
    }
    expected_ids
        .iter()
        .all(|expected| seen.iter().any(|id| id == expected))
        .then_some((points, missing))
}

fn parse_point_object(value: &serde_json::Value, named: bool) -> Option<GroundingPoint> {
    let expected = if named {
        &["id", "x", "y", "label"][..]
    } else {
        &["x", "y", "label"][..]
    };
    let object = exact_object(value, expected)?;
    let id = if named {
        Some(object.get("id")?.as_str()?.to_string())
    } else {
        None
    };
    Some(GroundingPoint {
        id,
        x: parse_grid_coordinate(object.get("x")?)?,
        y: parse_grid_coordinate(object.get("y")?)?,
        label: parse_label(object.get("label")?.as_str()?)?,
    })
}

fn parse_grid_coordinate(value: &serde_json::Value) -> Option<f64> {
    let parsed = value.as_u64()?;
    (parsed <= 1000).then_some(parsed as f64)
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
    let value = parse_json_response(response)?;
    exact_object(&value, &["matches", "confidence", "what"])?;
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

fn exact_object<'a>(
    value: &'a serde_json::Value,
    expected: &[&str],
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    let object = value.as_object()?;
    (object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key)))
        .then_some(object)
}

fn parse_json_response(response: &str) -> Option<serde_json::Value> {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return None;
    }
    let payload = if trimmed.starts_with("```") {
        let mut lines = trimmed.lines();
        let opener = lines.next()?.trim();
        if !matches!(opener, "```" | "```json" | "```JSON") {
            return None;
        }
        let mut body = lines.collect::<Vec<_>>();
        if body.pop()?.trim() != "```" || body.iter().any(|line| line.contains("```")) {
            return None;
        }
        body.join("\n")
    } else {
        if trimmed.contains("```") {
            return None;
        }
        trimmed.to_string()
    };
    serde_json::from_str(payload.trim()).ok()
}

fn named_points_schema(ids: &[&str]) -> serde_json::Value {
    let id_values = ids
        .iter()
        .map(|id| serde_json::json!(id))
        .collect::<Vec<_>>();
    serde_json::json!({
        "type": "object",
        "properties": {
            "points": {
                "type": "array",
                "minItems": 0,
                "maxItems": ids.len(),
                "items": point_schema(Some(&id_values))
            },
            "missing": {
                "type": "array",
                "minItems": 0,
                "maxItems": ids.len(),
                "items": {"type": "string", "enum": id_values}
            }
        },
        "required": ["points", "missing"],
        "additionalProperties": false
    })
}

fn open_points_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "points": {
                "type": "array",
                "minItems": 0,
                "maxItems": 30,
                "items": point_schema(None)
            }
        },
        "required": ["points"],
        "additionalProperties": false
    })
}

fn point_schema(ids: Option<&[serde_json::Value]>) -> serde_json::Value {
    let mut properties = serde_json::json!({
        "x": {"type": "integer", "minimum": 0, "maximum": 1000},
        "y": {"type": "integer", "minimum": 0, "maximum": 1000},
        "label": {"type": "string", "minLength": 1, "maxLength": 160}
    });
    let mut required = vec!["x", "y", "label"];
    if let Some(ids) = ids {
        properties["id"] = serde_json::json!({"type": "string", "enum": ids});
        required.insert(0, "id");
    }
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
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
        assert!(
            point
                .prompt
                .contains("only the keys \"points\" and \"missing\"")
        );
        assert!(point.prompt.contains("\"id\":\"target\""));
        let point_schema = point.response_schema.expect("point schema");
        assert_eq!(
            point_schema["required"],
            serde_json::json!(["points", "missing"])
        );

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
            parse_verification(
                "```json\n{\"matches\":true,\"confidence\":91,\"what\":\"Save button\"}\n```"
            )
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
