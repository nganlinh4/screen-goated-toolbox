//! Preserve the shared source-image and request diagnostics for live batches.
use super::super::{contract::DetectedTextRegion, diagnostics::RunEvidence};
use super::observation::Frame;
use crate::config::types::ScreenTranslateSettings;
#[cfg(debug_assertions)]
use image::ImageEncoder as _;

#[cfg(debug_assertions)]
pub(super) fn begin(
    trace: &str,
    frame: &Frame,
    candidates: &[DetectedTextRegion],
    settings: &ScreenTranslateSettings,
) -> Option<RunEvidence> {
    let mut bytes = Vec::new();
    if image::codecs::png::PngEncoder::new_with_quality(
        &mut bytes,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(
        frame.image.as_raw(),
        frame.image.width(),
        frame.image.height(),
        image::ExtendedColorType::Rgba8,
    )
    .is_err()
    {
        return None;
    }
    let capture = crate::overlay::selection::CapturedRegion {
        image: frame.image.as_ref().clone(),
        width: frame.image.width(),
        height: frame.image.height(),
        left: frame.origin.0,
        top: frame.origin.1,
    };
    let mut evidence = RunEvidence::begin(
        trace,
        &capture,
        &bytes,
        &settings.target_language,
        &settings.translation_model,
        &settings.translation_prompt,
    );
    evidence.detected(candidates, &[]);
    Some(evidence)
}

#[cfg(not(debug_assertions))]
pub(super) fn begin(
    _trace: &str,
    _frame: &Frame,
    _candidates: &[DetectedTextRegion],
    _settings: &ScreenTranslateSettings,
) -> Option<RunEvidence> {
    None
}
