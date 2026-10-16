use crate::overlay::creation_source::InspectedImage;

pub(super) fn error(image: &InspectedImage) -> Option<&'static str> {
    validate(image.width, image.height, image.size_bytes)
}

fn validate(_width: u32, _height: u32, _bytes: u64) -> Option<&'static str> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_input_boundaries() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/image-to-3d/input-contract.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                validate(
                    case["width"].as_u64().unwrap() as u32,
                    case["height"].as_u64().unwrap() as u32,
                    case["bytes"].as_u64().unwrap()
                ),
                case["error"].as_str()
            );
        }
    }
}
