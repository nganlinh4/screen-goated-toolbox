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
// The workspace advertises 50 000 faces, but only a paid provider tier can
// use the top of that range; 20 000 is the ceiling proven to complete on the
// accounts this pipeline provisions.
const QUALITY_MAX_POLYCOUNT: u32 = 20_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::overlay::three_d_generator) enum GenerationMode {
    #[default]
    Quality,
}

impl GenerationMode {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Quality => "quality",
        }
    }

    const fn polycount_limits(self) -> (u32, u32) {
        match self {
            Self::Quality => (QUALITY_MIN_POLYCOUNT, QUALITY_MAX_POLYCOUNT),
        }
    }
}

pub(super) fn normalize_request(request: &mut StartJobRequest) {
    let (minimum, maximum) = request.generation_mode.polycount_limits();
    request.polycount = request.polycount.clamp(minimum, maximum);
    request.auto_segment =
        request.generation_mode == GenerationMode::Quality && request.auto_segment;
    request.segmentation_mode = if request.auto_segment {
        "parts".to_string()
    } else {
        "none".to_string()
    };
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
        normalize_request(&mut quality);
        assert_eq!(quality.polycount, QUALITY_MIN_POLYCOUNT);
        assert!(quality.auto_segment);
        assert_eq!(quality.segmentation_mode, "parts");
    }

    #[test]
    fn frozen_settings_accept_valid_quality_values() {
        assert!(frozen_settings_valid(GenerationMode::Quality, 5_000, true));
    }
}
