//! A cancelled subtitle translation job never starts another provider request.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::translation_providers::{SUBTITLE_TRANSLATION_CANCELLED, translate_subtitle_chunk};
use crate::config::Config;
use crate::model_config::{ModelConfig, ModelType};

fn model(provider: &str) -> ModelConfig {
    ModelConfig::new(
        "test-model",
        provider,
        "Test",
        "Test",
        "Test",
        "test-model",
        ModelType::Text,
        true,
        "",
        "",
        "",
        false,
        false,
        1,
        1,
        "test",
    )
}

#[test]
fn cancelled_job_returns_before_dispatching_a_provider_request() {
    let config = Config::default();
    // An unknown provider fails at dispatch, so reaching it shows that the
    // request would have been sent.
    let model = model("not-a-provider");
    let live = Arc::new(AtomicBool::new(false));
    let dispatched = translate_subtitle_chunk(&config, &model, "en", None, &[], &[], &live)
        .expect_err("unknown provider");
    assert!(dispatched.contains("Unsupported"), "{dispatched}");

    let cancelled = Arc::new(AtomicBool::new(true));
    let result = translate_subtitle_chunk(&config, &model, "en", None, &[], &[], &cancelled)
        .expect_err("cancelled");
    assert_eq!(result, SUBTITLE_TRANSLATION_CANCELLED);
}
