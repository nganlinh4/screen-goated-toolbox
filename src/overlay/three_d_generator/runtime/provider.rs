use serde::{Deserialize, Serialize};

use super::StartJobRequest;

const FAST_MIN_POLYCOUNT: u32 = 100;
const FAST_MAX_POLYCOUNT: u32 = 15_000;
const QUALITY_MIN_POLYCOUNT: u32 = 500;
const QUALITY_MAX_POLYCOUNT: u32 = 20_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::overlay::three_d_generator) enum GenerationMode {
    Fast,
    #[default]
    Quality,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::overlay::three_d_generator) enum ModelProvider {
    Meshy,
    #[default]
    Tripo,
}

impl ModelProvider {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Meshy => "meshy",
            Self::Tripo => "tripo",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProviderRoute {
    polycount: u32,
    provider: ModelProvider,
    auto_segment: bool,
}

impl GenerationMode {
    const fn provider(self) -> ModelProvider {
        match self {
            Self::Fast => ModelProvider::Meshy,
            Self::Quality => ModelProvider::Tripo,
        }
    }

    const fn polycount_limits(self) -> (u32, u32) {
        match self {
            Self::Fast => (FAST_MIN_POLYCOUNT, FAST_MAX_POLYCOUNT),
            Self::Quality => (QUALITY_MIN_POLYCOUNT, QUALITY_MAX_POLYCOUNT),
        }
    }
}

fn route(mode: GenerationMode, polycount: u32, requested_auto_segment: bool) -> ProviderRoute {
    let (minimum, maximum) = mode.polycount_limits();
    ProviderRoute {
        polycount: polycount.clamp(minimum, maximum),
        provider: mode.provider(),
        auto_segment: mode == GenerationMode::Quality && requested_auto_segment,
    }
}

pub(super) fn normalize_request(request: &mut StartJobRequest) {
    let route = route(
        request.generation_mode,
        request.polycount,
        request.auto_segment,
    );
    request.polycount = route.polycount;
    request.provider = route.provider;
    request.auto_segment = route.auto_segment;
    request.segmentation_mode = if route.auto_segment {
        "parts".to_string()
    } else {
        "none".to_string()
    };
}

pub(super) fn can_offer_continuation(
    provider: ModelProvider,
    is_segmented: bool,
    runtime_can_segment: bool,
) -> bool {
    runtime_can_segment && !is_segmented && provider == ModelProvider::Tripo
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(polycount: u32, auto_segment: bool, provider: ModelProvider) -> StartJobRequest {
        StartJobRequest {
            image_path: "image.png".to_string(),
            output_dir: None,
            polycount,
            mode: "topology_mesh".to_string(),
            output_format: "glb_plain".to_string(),
            auto_segment,
            segmentation_mode: "parts".to_string(),
            generation_mode: GenerationMode::Quality,
            provider,
        }
    }

    #[test]
    fn explicit_mode_never_changes_with_topology_or_separation() {
        let cases = [
            (
                GenerationMode::Fast,
                100,
                false,
                ModelProvider::Meshy,
                false,
            ),
            (
                GenerationMode::Fast,
                20_000,
                true,
                ModelProvider::Meshy,
                false,
            ),
            (
                GenerationMode::Quality,
                100,
                false,
                ModelProvider::Tripo,
                false,
            ),
            (
                GenerationMode::Quality,
                5_000,
                true,
                ModelProvider::Tripo,
                true,
            ),
        ];
        for (mode, polycount, auto_segment, provider, expected_auto_segment) in cases {
            let route = route(mode, polycount, auto_segment);
            assert_eq!(route.provider, provider);
            assert_eq!(route.auto_segment, expected_auto_segment);
        }
    }

    #[test]
    fn normalization_clamps_to_the_selected_mode_without_switching() {
        let mut low = request(20, true, ModelProvider::Tripo);
        low.generation_mode = GenerationMode::Fast;
        normalize_request(&mut low);
        assert_eq!(low.polycount, FAST_MIN_POLYCOUNT);
        assert_eq!(low.provider, ModelProvider::Meshy);
        assert!(!low.auto_segment);
        assert_eq!(low.segmentation_mode, "none");

        let mut high = request(25_000, true, ModelProvider::Meshy);
        high.generation_mode = GenerationMode::Quality;
        normalize_request(&mut high);
        assert_eq!(high.polycount, QUALITY_MAX_POLYCOUNT);
        assert_eq!(high.provider, ModelProvider::Tripo);
        assert!(high.auto_segment);
        assert_eq!(high.segmentation_mode, "parts");
    }

    #[test]
    fn continuation_is_only_available_for_unsegmented_tripo_results() {
        assert!(can_offer_continuation(ModelProvider::Tripo, false, true));
        assert!(!can_offer_continuation(ModelProvider::Tripo, true, true));
        assert!(!can_offer_continuation(ModelProvider::Meshy, false, true));
        assert!(!can_offer_continuation(ModelProvider::Tripo, false, false));
    }
}
