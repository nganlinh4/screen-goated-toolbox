use serde::{Deserialize, Serialize};

use super::{RuntimeOperation, StartJobRequest};

pub(super) fn default_segmentation_level() -> String {
    "detailed".to_string()
}

pub(super) fn segmentation_level(operation: &RuntimeOperation) -> &str {
    match operation {
        RuntimeOperation::Generate { request, .. } => &request.segmentation_level,
        RuntimeOperation::Segment { continuation } | RuntimeOperation::Refine { continuation } => {
            &continuation.segmentation_level
        }
    }
}

const QUALITY_MIN_POLYCOUNT: u32 = 500;
const QUALITY_MAX_POLYCOUNT: u32 = 20_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::overlay::three_d_generator) enum GenerationMode {
    Fast,
    #[default]
    Quality,
}

impl GenerationMode {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Quality => "quality",
        }
    }

    const fn polycount_limits(self) -> (u32, u32) {
        match self {
            Self::Fast => (100, 15_000),
            Self::Quality => (QUALITY_MIN_POLYCOUNT, QUALITY_MAX_POLYCOUNT),
        }
    }
}

pub(super) fn normalize_request(request: &mut StartJobRequest) -> Result<(), String> {
    if request.generation_mode != GenerationMode::Quality {
        return Err("Choose Quality for a new model.".into());
    }
    let (minimum, maximum) = request.generation_mode.polycount_limits();
    request.polycount = request.polycount.clamp(minimum, maximum);
    request.auto_segment =
        request.generation_mode == GenerationMode::Quality && request.auto_segment;
    request.segmentation_mode = if request.auto_segment {
        "parts".to_string()
    } else {
        "none".to_string()
    };
    Ok(())
}

pub(super) fn frozen_settings_valid(
    mode: GenerationMode,
    polycount: u32,
    auto_segment: bool,
) -> bool {
    let (minimum, maximum) = mode.polycount_limits();
    (minimum..=maximum).contains(&polycount) && (mode == GenerationMode::Quality || !auto_segment)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_requests_keep_their_fingerprint_shape_and_explicit_topology_round_trips() {
        let mut value = request(5000, false);
        assert!(
            serde_json::to_value(&value)
                .unwrap()
                .get("topology")
                .is_none()
        );
        value.topology = Some("triangle".into());
        let restored: StartJobRequest =
            serde_json::from_value(serde_json::to_value(&value).unwrap()).unwrap();
        assert_eq!(restored.topology.as_deref(), Some("triangle"));
    }

    fn request(polycount: u32, auto_segment: bool) -> StartJobRequest {
        StartJobRequest {
            image_path: "image.png".to_string(),
            source_descriptors: Vec::new(),
            output_dir: None,
            final_output_dir: None,
            polycount,
            mode: "topology_mesh".to_string(),
            output_format: "glb_plain".to_string(),
            auto_segment,
            segmentation_level: "detailed".to_string(),
            topology: None,
            segmentation_mode: "parts".to_string(),
            generation_mode: GenerationMode::Quality,
            instruction: None,
            output_name: String::new(),
            dispatch_id: String::new(),
        }
    }

    #[test]
    fn normalize_clamps_settings_without_hidden_routing_state() {
        let mut quality = request(100, true);
        normalize_request(&mut quality).unwrap();
        assert_eq!(quality.polycount, QUALITY_MIN_POLYCOUNT);
        assert!(quality.auto_segment);
        assert_eq!(quality.segmentation_mode, "parts");
    }

    #[test]
    fn frozen_settings_accept_valid_quality_values() {
        assert!(frozen_settings_valid(GenerationMode::Quality, 5_000, true));
    }

    #[test]
    fn shared_new_and_frozen_modes_match_the_canonical_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/image-to-3d/generation-mode-contract.json"
        ))
        .unwrap();
        let state: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/image-to-3d/state-contract.json"
        ))
        .unwrap();
        assert_eq!(fixture["minimumPolycount"], QUALITY_MIN_POLYCOUNT);
        assert_eq!(fixture["maximumPolycount"], QUALITY_MAX_POLYCOUNT);
        assert_eq!(
            fixture["minimumPolycount"],
            state["limits"]["minimumPolycount"]
        );
        assert_eq!(
            fixture["maximumPolycount"],
            state["limits"]["maximumPolycount"]
        );
        assert_eq!(
            fixture["currentModes"],
            state["generationModes"]["newRequestModes"]
        );
        for row in fixture["newRequests"].as_array().unwrap() {
            let mut raw = serde_json::to_value(request(5000, false)).unwrap();
            raw["generationMode"] = row["mode"].clone();
            raw["polycount"] = row["polycount"].clone();
            raw["autoSegment"] = row["autoSegment"].clone();
            let result = serde_json::from_value::<StartJobRequest>(raw)
                .map_err(|error| error.to_string())
                .and_then(|mut value| normalize_request(&mut value).map(|()| value));
            assert_eq!(result.is_ok(), !row["normalizedPolycount"].is_null());
            if let Ok(value) = result {
                assert_eq!(
                    value.polycount,
                    row["normalizedPolycount"].as_u64().unwrap() as u32
                );
            }
        }
        for row in fixture["frozenRequests"].as_array().unwrap() {
            let valid =
                serde_json::from_value::<GenerationMode>(row["mode"].clone()).is_ok_and(|mode| {
                    frozen_settings_valid(
                        mode,
                        row["polycount"].as_u64().unwrap() as u32,
                        row["autoSegment"].as_bool().unwrap(),
                    )
                });
            assert_eq!(valid, row["valid"].as_bool().unwrap());
        }
    }
}
