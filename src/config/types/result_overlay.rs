pub const DEFAULT_RESULT_OVERLAY_OPACITY_PERCENT: u8 = 90;
pub const MIN_RESULT_OVERLAY_OPACITY_PERCENT: u8 = 10;
pub const MAX_RESULT_OVERLAY_OPACITY_PERCENT: u8 = 100;

pub fn default_result_overlay_opacity_percent() -> u8 {
    DEFAULT_RESULT_OVERLAY_OPACITY_PERCENT
}

pub fn normalize_result_overlay_opacity_percent(value: u8) -> u8 {
    value.clamp(
        MIN_RESULT_OVERLAY_OPACITY_PERCENT,
        MAX_RESULT_OVERLAY_OPACITY_PERCENT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn default_matches_parity_and_preserves_explicit_saved_values() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/preset-system/result-overlay.json"
        ))
        .unwrap();
        let opacity = &fixture["opacity"];
        assert_eq!(
            opacity["default_percent"].as_u64(),
            Some(default_result_overlay_opacity_percent() as u64)
        );
        assert_eq!(
            opacity["minimum_percent"].as_u64(),
            Some(MIN_RESULT_OVERLAY_OPACITY_PERCENT as u64)
        );
        assert_eq!(
            opacity["maximum_percent"].as_u64(),
            Some(MAX_RESULT_OVERLAY_OPACITY_PERCENT as u64)
        );

        let defaults = Config::default();
        assert_eq!(
            defaults.favorite_overlay_opacity,
            default_result_overlay_opacity_percent()
        );
        let mut serialized = serde_json::to_value(defaults).unwrap();
        serialized["favorite_overlay_opacity"] = serde_json::json!(85);
        let saved: Config = serde_json::from_value(serialized.clone()).unwrap();
        assert_eq!(saved.favorite_overlay_opacity, 85);

        serialized
            .as_object_mut()
            .unwrap()
            .remove("favorite_overlay_opacity");
        let missing: Config = serde_json::from_value(serialized).unwrap();
        assert_eq!(
            missing.favorite_overlay_opacity,
            default_result_overlay_opacity_percent()
        );
    }
}
