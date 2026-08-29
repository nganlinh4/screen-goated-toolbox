use serde::{Deserialize, Serialize};

use super::StartJobRequest;

const FAST_MIN_POLYCOUNT: u32 = 100;
const FAST_MAX_POLYCOUNT: u32 = 15_000;
const QUALITY_MIN_POLYCOUNT: u32 = 500;
// The workspace advertises 50 000 faces, but only a paid provider tier can
// use the top of that range; 20 000 is the ceiling proven to complete on the
// accounts this pipeline provisions.
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
            Self::Fast => (FAST_MIN_POLYCOUNT, FAST_MAX_POLYCOUNT),
            Self::Quality => (QUALITY_MIN_POLYCOUNT, QUALITY_MAX_POLYCOUNT),
        }
    }
}

pub(super) fn normalize_request(request: &mut StartJobRequest) {
    request.generation_mode = GenerationMode::Quality;
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

pub(super) fn continuation_advertised(_is_segmented: bool, runtime_can_continue: bool) -> bool {
    runtime_can_continue
}

#[cfg(test)]
mod tests {
    use super::*;

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
            segmentation_mode: "parts".to_string(),
            generation_mode: GenerationMode::Quality,
            instruction: None,
            output_name: String::new(),
            dispatch_id: String::new(),
        }
    }

    #[test]
    fn unsupported_legacy_mode_normalizes_to_the_available_product_flow() {
        let mut fast = request(20_000, true);
        fast.generation_mode = GenerationMode::Fast;
        normalize_request(&mut fast);
        assert_eq!(fast.generation_mode, GenerationMode::Quality);
        assert_eq!(fast.polycount, QUALITY_MAX_POLYCOUNT);
        assert!(fast.auto_segment);
        assert_eq!(fast.segmentation_mode, "parts");

        let mut quality = request(100, true);
        normalize_request(&mut quality);
        assert_eq!(quality.polycount, QUALITY_MIN_POLYCOUNT);
        assert!(quality.auto_segment);
        assert_eq!(quality.segmentation_mode, "parts");
    }

    #[test]
    fn continuation_depends_only_on_the_runtime_capability_and_result_state() {
        assert!(continuation_advertised(false, true));
        assert!(continuation_advertised(true, true));
        assert!(!continuation_advertised(false, false));
    }

    #[test]
    fn frozen_settings_reject_values_that_would_be_normalized() {
        assert!(frozen_settings_valid(GenerationMode::Quality, 5_000, true));
        assert!(frozen_settings_valid(GenerationMode::Fast, 15_000, false));
        assert!(!frozen_settings_valid(GenerationMode::Fast, 15_001, false));
        assert!(!frozen_settings_valid(GenerationMode::Fast, 5_000, true));
    }
}
