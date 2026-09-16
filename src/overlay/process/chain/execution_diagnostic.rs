//! Opt-in production block-chain diagnostics using caller-supplied image inputs.

use super::{ExecuteBlockRequest, execute_block};
use crate::catalog_benchmark::setup::Credentials;
use crate::overlay::result::{ChainCancelToken, RefineContext};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;

#[derive(Deserialize)]
struct Plan {
    models: Vec<String>,
    image: std::path::PathBuf,
    prompt: String,
    reference: String,
    #[serde(default = "single_run")]
    repetitions: usize,
}

fn single_run() -> usize {
    1
}

#[test]
#[ignore = "requires OCR_CHAIN_PLAN and provider credentials; calls the production preset chain"]
fn preset_image_chain_diagnostic() -> Result<()> {
    let plan: Plan = serde_json::from_slice(&std::fs::read(std::env::var("OCR_CHAIN_PLAN")?)?)?;
    ensure!(!plan.models.is_empty());
    ensure!((1..=3).contains(&plan.repetitions));
    let context = RefineContext::Image(std::fs::read(&plan.image)?);
    let credentials = Credentials::load()?;
    if credentials.supports("nvidia") {
        crate::model_feed::store::refresh()?;
    }
    let model =
        crate::model_config::get_model_by_id(&plan.models[0]).context("Unknown initial model")?;
    let mut preset = crate::config::preset::defaults::create_image_presets()
        .into_iter()
        .find(|preset| preset.id == "preset_ocr")
        .context("OCR preset")?;
    let block = preset.blocks.first_mut().context("OCR block")?;
    block.model = model.id.clone();
    block.prompt = plan.prompt.clone();
    for run in 1..=plan.repetitions {
        let started = std::time::Instant::now();
        let token = ChainCancelToken::new();
        let response = credentials.with_provider_key("nvidia", |nvidia| {
            credentials.with_provider_key("google", |google| {
                credentials.with_provider_key("groq", |groq| {
                    let mut config = crate::config::Config {
                        api_key: groq.to_string(),
                        gemini_api_key: google.to_string(),
                        nvidia_api_key: nvidia.to_string(),
                        ..Default::default()
                    };
                    config.model_priority_chains.image_to_text = plan.models.clone();
                    config.adaptive_model_priority.image_to_text = false;
                    execute_block(ExecuteBlockRequest {
                        block: &preset.blocks[0],
                        block_idx: 0,
                        blocks: &preset.blocks,
                        my_hwnd: None,
                        input_text: "",
                        context: &context,
                        model_id: &model.id,
                        provider: &model.provider,
                        model_full_name: &model.full_name,
                        final_prompt: &plan.prompt,
                        skip_execution: false,
                        config: &config,
                        preset_id: &preset.id,
                        processing_hwnd_shared: None,
                        cancel_token: &token,
                    })
                })
            })
        });
        ensure!(
            response.trim() == plan.reference.trim(),
            "Preset chain output did not match the reference"
        );
        println!(
            "OCR_CHAIN run={run} elapsed_ms={} result=exact_match output_chars={}",
            started.elapsed().as_millis(),
            response.chars().count()
        );
    }
    Ok(())
}
