use crate::api::gemini_generate::GeminiMediaResolution;
use crate::model_config::{
    VisionInputOrder, VisionMediaResolutionPolicy, VisionRequestProfile, VisionSamplingPolicy,
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
            min_reliable_pixels: None,
            restates_output: false,
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
            assert_eq!(
                profile.restates_output,
                case["restates_output"].as_bool().unwrap()
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
