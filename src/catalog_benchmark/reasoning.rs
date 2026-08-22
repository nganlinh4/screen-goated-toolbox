use crate::model_config::{ModelConfig, OrdinaryReasoningPolicy};

pub(super) fn reasoning_policy_label(model: &ModelConfig) -> String {
    if let Some(control) = crate::model_feed::store::control_for(&model.provider, &model.full_name)
    {
        return format!("feed:{}", feed_control_label(control));
    }
    match crate::model_config::ordinary_reasoning_policy(&model.provider, &model.full_name) {
        OrdinaryReasoningPolicy::NotApplicable => "not-applicable".to_string(),
        OrdinaryReasoningPolicy::GeminiBudget(budget) => format!("gemini-budget:{budget}"),
        OrdinaryReasoningPolicy::GeminiLevel(level) => {
            format!("gemini-level:{}", level.to_ascii_lowercase())
        }
        OrdinaryReasoningPolicy::OpenAiEffort(effort) => format!("openai-effort:{effort}"),
        OrdinaryReasoningPolicy::ProviderManaged => "provider-managed".to_string(),
        OrdinaryReasoningPolicy::LiveProfile => "live-profile".to_string(),
    }
}

fn feed_control_label(control: crate::model_feed::FeedControl) -> &'static str {
    use crate::model_feed::FeedControl;
    match control {
        FeedControl::Plain => "plain",
        FeedControl::EffortNone => "effort-none",
        FeedControl::EffortLow => "effort-low",
        FeedControl::TemplateKwargs => "template-kwargs",
        FeedControl::NoThink => "no-think",
        FeedControl::ThinkingOff => "thinking-off",
    }
}

#[cfg(test)]
mod tests {
    use super::feed_control_label;
    use crate::model_feed::FeedControl;

    #[test]
    fn every_signed_feed_control_has_a_stable_history_identity() {
        let labels = [
            feed_control_label(FeedControl::Plain),
            feed_control_label(FeedControl::EffortNone),
            feed_control_label(FeedControl::EffortLow),
            feed_control_label(FeedControl::TemplateKwargs),
            feed_control_label(FeedControl::NoThink),
            feed_control_label(FeedControl::ThinkingOff),
        ];
        assert_eq!(
            labels.len(),
            labels
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
        );
    }
}
