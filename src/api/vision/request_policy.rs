use crate::api::gemini_generate::GeminiMediaResolution;
use crate::model_config::{
    StructuredOutputPolicy, VisionInputOrder, VisionMediaResolutionPolicy, VisionRequestProfile,
    VisionSamplingPolicy,
};

pub(super) fn gemini_parts(
    profile: VisionRequestProfile,
    prompt: &str,
    mime_type: &str,
    b64_image: &str,
) -> serde_json::Value {
    let text = serde_json::json!({ "text": prompt });
    let image = serde_json::json!({
        "inline_data": {
            "mime_type": mime_type,
            "data": b64_image
        }
    });
    ordered_parts(profile.input_order, text, image)
}

pub(super) fn openai_content(
    profile: VisionRequestProfile,
    prompt: &str,
    mime_type: &str,
    b64_image: &str,
) -> serde_json::Value {
    let text = serde_json::json!({ "type": "text", "text": prompt });
    let image = serde_json::json!({
        "type": "image_url",
        "image_url": { "url": format!("data:{mime_type};base64,{b64_image}") }
    });
    ordered_parts(profile.input_order, text, image)
}

fn ordered_parts(
    input_order: VisionInputOrder,
    text: serde_json::Value,
    image: serde_json::Value,
) -> serde_json::Value {
    match input_order {
        VisionInputOrder::TextFirst => serde_json::json!([text, image]),
        VisionInputOrder::ImageFirst => serde_json::json!([image, text]),
    }
}

/// Instruction appended when plain text has to travel inside a JSON envelope.
pub(super) const PLAIN_TEXT_ENVELOPE_PROMPT: &str =
    "

Respond with a single JSON object of the form {\"text\": \"<all extracted text>\"} and nothing else.";

/// Whether a plain-text extraction must be wrapped in a JSON envelope.
///
/// Qwen 3.6 on Groq deterministically appends a re-tokenized repetition of the
/// text it just emitted when asked for bare text: at temperature 0 the wrong
/// answer is its highest-probability completion, and neither sampling changes
/// nor upscaling the image avoids it. This is the upstream Qwen3-VL repetition
/// defect (QwenLM/Qwen3-VL#1611), not a transport problem. Constraining the
/// reply to a JSON object gives the grammar a closing quote and brace, which
/// terminates generation cleanly; Groq documents JSON object mode as the
/// supported path for models without strict structured outputs.
///
/// Only endpoints the catalog marks `json-object` take this path, and only when
/// the caller wants plain text and is not streaming — a streamed envelope would
/// paint raw JSON into the result window.
pub(super) fn needs_plain_text_envelope(
    profile: VisionRequestProfile,
    streaming: bool,
    has_schema: bool,
) -> bool {
    !streaming && !has_schema && profile.structured_output == StructuredOutputPolicy::JsonObject
}

/// Recovers the text from a [`PLAIN_TEXT_ENVELOPE_PROMPT`] reply.
///
/// Fails open: anything that is not the expected envelope is returned unchanged,
/// so a malformed reply degrades to today's behaviour instead of losing text.
pub(super) fn unwrap_plain_text_envelope(content: &str) -> String {
    serde_json::from_str::<serde_json::Value>(content.trim())
        .ok()
        .as_ref()
        .and_then(|value| value.get("text"))
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| content.to_string(), str::to_string)
}

pub(super) fn media_resolution(profile: VisionRequestProfile) -> Option<GeminiMediaResolution> {
    match profile.media_resolution {
        VisionMediaResolutionPolicy::ProviderDefault => None,
    }
}

pub(super) fn apply_sampling_policy(
    payload: &mut serde_json::Value,
    profile: VisionRequestProfile,
) {
    match profile.sampling {
        VisionSamplingPolicy::ProviderDefault => {}
        VisionSamplingPolicy::Qwen3GroqNonThinking => {
            payload["temperature"] = serde_json::json!(0.7);
            payload["top_p"] = serde_json::json!(0.8);
            payload["presence_penalty"] = serde_json::json!(1.5);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_config::{StructuredOutputPolicy, VisionSamplingPolicy};

    fn profile(order: VisionInputOrder) -> VisionRequestProfile {
        VisionRequestProfile {
            input_order: order,
            media_resolution: VisionMediaResolutionPolicy::ProviderDefault,
            sampling: VisionSamplingPolicy::ProviderDefault,
            max_output_tokens: None,
            structured_output: StructuredOutputPolicy::Unsupported,
        }
    }

    #[test]
    fn part_order_is_structural() {
        let text_first = gemini_parts(profile(VisionInputOrder::TextFirst), "P", "image/png", "AA");
        let image_first = gemini_parts(
            profile(VisionInputOrder::ImageFirst),
            "P",
            "image/png",
            "AA",
        );
        assert_eq!(text_first[0]["text"], "P");
        assert_eq!(image_first[0]["inline_data"]["mime_type"], "image/png");
    }

    #[test]
    fn generated_profiles_match_the_cross_platform_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/preset-system/vision-payload.json"
        ))
        .unwrap();
        for case in fixture["ordinary_llm_profiles"]["cases"]
            .as_array()
            .unwrap()
        {
            let provider = case["provider"].as_str().unwrap();
            let api_model = case["api_model"].as_str().unwrap();
            let profile = crate::model_config::vision_request_profile(provider, api_model);
            assert_eq!(
                input_order_name(profile.input_order),
                case["input_order"].as_str().unwrap()
            );
            assert_eq!(
                media_resolution_name(profile.media_resolution),
                case["media_resolution"].as_str().unwrap()
            );
            assert_eq!(
                sampling_name(profile.sampling),
                case["sampling"].as_str().unwrap()
            );
            assert_eq!(
                profile
                    .max_output_tokens
                    .map_or(serde_json::Value::Null, |value| serde_json::json!(value)),
                case["max_output_tokens"]
            );
            assert_eq!(
                structured_output_name(profile.structured_output),
                case["structured_output"].as_str().unwrap()
            );
        }
    }

    fn input_order_name(value: VisionInputOrder) -> &'static str {
        match value {
            VisionInputOrder::TextFirst => "text-first",
            VisionInputOrder::ImageFirst => "image-first",
        }
    }

    fn media_resolution_name(value: VisionMediaResolutionPolicy) -> &'static str {
        match value {
            VisionMediaResolutionPolicy::ProviderDefault => "provider-default",
        }
    }

    fn sampling_name(value: VisionSamplingPolicy) -> &'static str {
        match value {
            VisionSamplingPolicy::ProviderDefault => "provider-default",
            VisionSamplingPolicy::Qwen3GroqNonThinking => "qwen3-groq-non-thinking",
        }
    }

    fn structured_output_name(value: StructuredOutputPolicy) -> &'static str {
        match value {
            StructuredOutputPolicy::Unsupported => "unsupported",
            StructuredOutputPolicy::PromptOnly => "prompt-only",
            StructuredOutputPolicy::JsonObject => "json-object",
            StructuredOutputPolicy::StrictJsonSchema => "strict-json-schema",
        }
    }
}
