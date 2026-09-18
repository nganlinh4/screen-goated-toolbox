//! Opt-in endpoint diagnostics through the production benchmark request paths.
//! Results deliberately stay outside catalog history and admission metadata.

use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use super::super::{
    manifest::Manifest,
    report::Recorder,
    setup::{Credentials, Pacer},
};

#[derive(Deserialize)]
struct Plan {
    models: Vec<Candidate>,
    suites: Vec<String>,
}

#[derive(Deserialize)]
struct Candidate {
    template_id: String,
    endpoint: String,
}

#[test]
#[ignore = "requires CATALOG_CAPABILITY_PLAN and real provider credentials"]
fn catalog_capability_probe() -> Result<()> {
    let plan_path = std::env::var("CATALOG_CAPABILITY_PLAN")?;
    let plan: Plan = serde_json::from_slice(&std::fs::read(plan_path)?)?;
    ensure!(!plan.models.is_empty(), "no candidate endpoints supplied");
    ensure!(
        plan.suites
            .iter()
            .all(|s| matches!(s.as_str(), "text" | "coordinate" | "ocr")),
        "unknown suite"
    );
    let output = PathBuf::from(std::env::var("CATALOG_CAPABILITY_OUTPUT")?);
    ensure!(!output.exists(), "preserve existing diagnostic evidence");
    let manifest = Manifest::load()?;
    manifest.validate()?;
    let credentials = Credentials::load()?;
    let mut pacer = Pacer::from_env(&credentials)?;
    let timeout = super::super::setup::request_timeout()?;
    let mut models = Vec::new();
    for candidate in plan.models {
        let mut model = crate::model_config::get_model_by_id(&candidate.template_id)
            .context("unknown template model")?;
        ensure!(
            model.provider == "gemini-live",
            "endpoint substitution requires a Live profile"
        );
        ensure!(
            crate::model_config::live_endpoint_profile(&candidate.endpoint).is_some(),
            "missing exact endpoint profile"
        );
        model.id = format!("diagnostic:{}", candidate.endpoint);
        model.full_name = candidate.endpoint;
        models.push(model);
    }
    crate::api::gemini_live::init_gemini_live();
    let mut recorder = Recorder::new(&output)?;
    for round in 1..=manifest.rounds {
        for model in super::rotated(&models, round) {
            for suite in &plan.suites {
                pacer.wait(model);
                let attempt = match suite.as_str() {
                    "text" => super::run_text(
                        model,
                        super::case_at_difficulty(&manifest.text_cases, round),
                        round,
                        &credentials,
                        timeout,
                    ),
                    "coordinate" => super::coordinate::run(
                        model,
                        super::case_at_difficulty(&manifest.coordinate_cases, round),
                        round,
                        &manifest,
                        &credentials,
                        timeout,
                        &mut pacer,
                    ),
                    "ocr" => super::run_ocr(
                        model,
                        super::case_at_difficulty(&manifest.ocr_cases, round),
                        round,
                        &manifest,
                        &credentials,
                    ),
                    _ => unreachable!(),
                };
                recorder.push(attempt)?;
            }
        }
    }
    recorder.finish()
}
