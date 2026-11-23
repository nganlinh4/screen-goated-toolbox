//! How one vision endpoint wants its request shaped, and what is known
//! about how it answers.
//!
//! Separated from the registry because these are per-endpoint measurements
//! rather than facts about which models exist.

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisionInputOrder {
    TextFirst,
    ImageFirst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisionMediaResolutionPolicy {
    ProviderDefault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisionSamplingPolicy {
    ProviderDefault,
    Qwen3GroqNonThinking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StructuredOutputPolicy {
    /// No schema is attached. The catalog's `prompt-only` value also maps
    /// here: asking for structure in the prompt alone sends the same request.
    Unsupported,
    JsonObject,
    StrictJsonSchema,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct VisionRequestProfile {
    pub input_order: VisionInputOrder,
    pub media_resolution: VisionMediaResolutionPolicy,
    pub sampling: VisionSamplingPolicy,
    pub max_output_tokens: Option<u32>,
    pub minimum_dimension: Option<u32>,
    pub structured_output: StructuredOutputPolicy,
    /// Whether this endpoint is known to re-emit text it has already produced.
    ///
    /// A measured property of one endpoint, kept in the catalog rather than in
    /// the code that acts on it, so recording a newly affected model is a data
    /// change. The salvage it enables edits a reply, and an edit applied to a
    /// model that does not have the fault can only ever remove correct text.
    pub restates_output: bool,
}

/// Whether structured requests to this endpoint use strict JSON-schema mode:
/// its catalog profile declares that policy for text or for vision requests.
pub fn supports_strict_json_schema(provider: &str, api_model: &str) -> bool {
    super::text_structured_output_policy(provider, api_model)
        == Some(StructuredOutputPolicy::StrictJsonSchema)
        || super::vision_request_profile(provider, api_model).structured_output
            == StructuredOutputPolicy::StrictJsonSchema
}

impl VisionRequestProfile {
    pub(crate) const SAFE_DEFAULT: Self = Self {
        input_order: VisionInputOrder::TextFirst,
        media_resolution: VisionMediaResolutionPolicy::ProviderDefault,
        sampling: VisionSamplingPolicy::ProviderDefault,
        max_output_tokens: super::DEFAULT_VISION_MAX_OUTPUT_TOKENS,
        minimum_dimension: None,
        structured_output: StructuredOutputPolicy::Unsupported,
        restates_output: false,
    };
}
