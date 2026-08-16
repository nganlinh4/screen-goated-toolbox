use super::contract::{DetectedTextRegion, TranslationDocument};
use crate::overlay::selection::CapturedRegion;

pub(crate) struct RunEvidence;

impl RunEvidence {
    pub(crate) fn begin(
        _trace_id: &str,
        _capture: &CapturedRegion,
        _source_jpeg: &[u8],
        _target_language: &str,
        _configured_model: &str,
        _translation_prompt: &str,
    ) -> Self {
        Self
    }

    pub(crate) fn detected(
        &mut self,
        _candidates: &[DetectedTextRegion],
        _raw: &[sgt_screen_text_detector_protocol::DetectedRegion],
    ) {
    }

    pub(crate) fn finish(self, _document: TranslationDocument, _rendered_count: usize) {}
    pub(crate) fn no_text(self) {}
    pub(crate) fn fail(self, _stage: &str, _error: &anyhow::Error) {}
}
