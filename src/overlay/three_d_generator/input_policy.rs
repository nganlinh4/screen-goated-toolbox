use crate::overlay::creation_source::InspectedImage;

pub(super) fn error(image: &InspectedImage, fast: bool) -> Option<&'static str> {
    validate(image.width, image.height, image.size_bytes, fast)
}

fn validate(width: u32, height: u32, bytes: u64, fast: bool) -> Option<&'static str> {
    if fast && (width < 32 || height < 32) {
        Some("image_too_small")
    } else if fast && bytes > 20_000_000 {
        Some("image_too_large")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_specific_input_boundaries() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/image-to-3d/input-contract.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                validate(
                    case["width"].as_u64().unwrap() as u32,
                    case["height"].as_u64().unwrap() as u32,
                    case["bytes"].as_u64().unwrap(),
                    case["mode"] == "fast"
                ),
                case["error"].as_str()
            );
        }
    }
}
