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
    pub response_schema: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VerificationDecision {
    pub matches: bool,
    pub confidence: u64,
    pub note: Option<String>,
}

pub(crate) fn point_request(description: &str, context: &str) -> GroundingRequest {
    GroundingRequest {
        prompt: format!(
            "{}Find this target in the image: {description}. Output ONLY JSON \
             {{\"x\": <int>, \"y\": <int>, \"what\": \"<2-4 words: what is AT that location, e.g. empty cell, an X, a button>\"}} \
             - x,y are the CENTER on a 0-1000 grid (x: 0 left to 1000 right; y: 0 top to 1000 bottom). If the target is not \
             visible, output {{\"error\": \"not visible\"}}.",
            context_prefix(context)
        ),
        response_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "x": {"type": "integer"},
                "y": {"type": "integer"},
                "what": {"type": "string"},
                "error": {"type": "string"}
            }
        }),
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
        response_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "matches": {"type": "boolean"},
                "confidence": {"type": "integer"},
                "what": {"type": "string"}
            },
            "required": ["matches", "confidence", "what"]
        }),
    }
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

pub(crate) fn parse_point(response: &str) -> Option<(f64, f64)> {
    let x = number_after_key(response, b'x')?;
    let y = number_after_key(response, b'y')?;
    Some((x.clamp(0.0, 1000.0), y.clamp(0.0, 1000.0)))
}

pub(crate) fn parse_verification(response: &str) -> Option<VerificationDecision> {
    let start = response.find('{')?;
    let end = response.rfind('}')?;
    let value: serde_json::Value = serde_json::from_str(&response[start..=end]).ok()?;
    Some(VerificationDecision {
        matches: value.get("matches").and_then(serde_json::Value::as_bool)?,
        confidence: value
            .get("confidence")
            .and_then(serde_json::Value::as_u64)?
            .min(100),
        note: value
            .get("what")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    })
}

pub(crate) fn parse_string_field(response: &str, key: &str) -> Option<String> {
    let lower = response.to_ascii_lowercase();
    let key_start = lower.find(&format!("\"{key}\""))?;
    let after_key = &response[key_start..];
    let colon = after_key.find(':')?;
    let rest = &after_key[colon + 1..];
    let opening_quote = rest.find('"')?;
    let closing_quote = rest[opening_quote + 1..].find('"')?;
    let value = rest[opening_quote + 1..opening_quote + 1 + closing_quote].trim();
    (!value.is_empty()).then(|| value.to_string())
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

fn number_after_key(response: &str, key: u8) -> Option<f64> {
    let lower = response.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let key = key.to_ascii_lowercase();
    let mut index = 0;
    let mut found = None;
    while index < bytes.len() {
        if bytes[index] == key && (index == 0 || !bytes[index - 1].is_ascii_alphanumeric()) {
            let mut cursor = index + 1;
            while cursor < bytes.len() && matches!(bytes[cursor], b'"' | b'\'' | b' ' | b'\t') {
                cursor += 1;
            }
            if cursor < bytes.len() && matches!(bytes[cursor], b':' | b'=') {
                cursor += 1;
                while cursor < bytes.len() && matches!(bytes[cursor], b'"' | b'\'' | b' ' | b'\t') {
                    cursor += 1;
                }
                let start = cursor;
                while cursor < bytes.len()
                    && (bytes[cursor].is_ascii_digit() || bytes[cursor] == b'.')
                {
                    cursor += 1;
                }
                if cursor > start
                    && let Ok(value) = lower[start..cursor].parse::<f64>()
                {
                    found = Some(value);
                }
            }
        }
        index += 1;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_and_verification_contracts_are_fail_closed() {
        let point = point_request("Save button", "Save the document");
        assert!(point.prompt.contains("Context (for disambiguation only"));
        assert!(point.prompt.contains(r#"{"error": "not visible"}"#));
        assert!(point.response_schema["properties"]["error"].is_object());

        let verification = verification_request("Save button", "Save the document");
        assert!(verification.prompt.contains("CROSSHAIR CENTER"));
        assert_eq!(
            verification.response_schema["required"],
            serde_json::json!(["matches", "confidence", "what"])
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

    #[test]
    fn point_parser_matches_the_tolerant_production_contract() {
        assert_eq!(
            parse_point("reasoning x=0 y=0; final {\"y\":250,\"x\":150}"),
            Some((150.0, 250.0))
        );
        assert_eq!(parse_point(r#"{"error":"not visible"}"#), None);
    }
}
