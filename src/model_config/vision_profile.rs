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
    Unsupported,
    // Constructed by the generated catalog rather than by hand, so it reads as
    // dead whenever no enabled endpoint selects it. It stays because the value
    // is part of the shared wire contract that Android, both validators, and
    // catalog/README.md all define.
    #[allow(dead_code)]
    PromptOnly,
    JsonObject,
    StrictJsonSchema,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct VisionRequestProfile {
    pub input_order: VisionInputOrder,
    pub media_resolution: VisionMediaResolutionPolicy,
    pub sampling: VisionSamplingPolicy,
    pub max_output_tokens: Option<u32>,
    pub structured_output: StructuredOutputPolicy,
    /// Smallest image, in pixels, this endpoint answers reliably.
    ///
    /// `None` means no lower bound has been measured, which is the normal case.
    /// An image below the bound is not refused -- it is routed past this endpoint
    /// to the next one in the chain, which is what the chain is for.
    ///
    /// This is a bound, not the mechanism. Area is used because it is available
    /// before the request and provides a stable capability boundary without
    /// coupling routing to language, content, or a particular workflow.
    pub min_reliable_pixels: Option<u32>,
    /// Whether this endpoint is known to re-emit text it has already produced.
    ///
    /// A measured property of one endpoint, kept in the catalog rather than in
    /// the code that acts on it, so recording a newly affected model is a data
    /// change. The salvage it enables edits a reply, and an edit applied to a
    /// model that does not have the fault can only ever remove correct text.
    pub restates_output: bool,
}

impl VisionRequestProfile {
    pub(crate) const SAFE_DEFAULT: Self = Self {
        input_order: VisionInputOrder::TextFirst,
        media_resolution: VisionMediaResolutionPolicy::ProviderDefault,
        sampling: VisionSamplingPolicy::ProviderDefault,
        max_output_tokens: None,
        structured_output: StructuredOutputPolicy::Unsupported,
        min_reliable_pixels: None,
        restates_output: false,
    };
}
