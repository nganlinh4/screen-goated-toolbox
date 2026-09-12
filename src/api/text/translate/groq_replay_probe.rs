//! Opt-in request-mode diagnostic using saved inputs and the production transport.
use super::*;
use crate::catalog_benchmark::setup::Credentials;
use crate::overlay::screen_translate::{contract::*, stream_parser::TranslationStreamParser};
use anyhow::{Context, ensure};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires SGT_TRANSLATION_REPLAY_INPUTS, OUTPUT and existing provider credentials"]
fn replay_structured_translation_modes() -> Result<()> {
    let inputs = std::env::var("SGT_TRANSLATION_REPLAY_INPUTS")?;
    let output = PathBuf::from(std::env::var("SGT_TRANSLATION_REPLAY_OUTPUT")?);
    std::fs::create_dir_all(&output)?;
    let mut report = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join("attempts.jsonl"))?;
    let repeats = std::env::var("SGT_TRANSLATION_REPLAY_REPEATS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2);
    let modes = std::env::var("SGT_TRANSLATION_REPLAY_MODES")
        .unwrap_or_else(|_| "strict,json,prompt".into());
    let pace = std::env::var("SGT_TRANSLATION_REPLAY_PACE_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(60);
    let selected_attempt = std::env::var("SGT_TRANSLATION_REPLAY_ATTEMPT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok());
    let credentials = Credentials::load()?;
    ensure!(
        credentials.supports("groq"),
        "existing Groq credential required"
    );
    let mut failures = 0;
    for repeat in 0..repeats {
        for (input_index, path) in inputs.split(';').enumerate() {
            let run: Value = serde_json::from_slice(&std::fs::read(path)?)?;
            for (attempt_index, attempt) in run["modelAttempts"]
                .as_array()
                .context("saved run modelAttempts")?
                .iter()
                .enumerate()
            {
                if selected_attempt.is_some_and(|index| index != attempt_index) {
                    continue;
                }
                ensure!(attempt["provider"] == "groq", "Groq request required");
                let content = &attempt["content"];
                let prompt = content["prompt"].as_str().context("saved prompt")?;
                let members = prompt.split_once("Members:\n").context("members")?.1;
                let members = serde_json::Deserializer::from_str(members)
                    .into_iter::<Vec<Value>>()
                    .next()
                    .context("member array")??;
                let candidates = members
                    .iter()
                    .map(|member| -> Result<_> {
                        Ok(DetectedTextRegion {
                            id: member["slot"].as_u64().context("slot")? as u16 + 1,
                            bounds: serde_json::from_value::<[u16; 4]>(member["box_2d"].clone())?
                                .into(),
                            source_text: member["text"].as_str().context("text")?.to_owned(),
                            source_alternatives: vec![],
                            recognition: Default::default(),
                            appearance: None,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let model = attempt["apiModel"].as_str().context("api model")?;
                let prompt = format!(
                    "{}\n\n{prompt}",
                    content["instruction"].as_str().context("instruction")?
                );
                let cap = candidates
                    .iter()
                    .fold(64_u32, |n, r| {
                        n.saturating_add(r.source_text.len() as u32 * 2 + 12)
                    })
                    .clamp(256, 8192);
                let transport = TranslateTransportOptions {
                    locally_validated_schema: false,
                    max_output_tokens: Some(cap),
                    streaming_enabled: true,
                    ui_language: "en",
                    cancel_token: &None,
                    request_timeout: Some(crate::api::client::RequestTimeouts::uniform(
                        Duration::from_secs(30),
                    )),
                };
                let base =
                    standard_payload(model, &prompt, true, Some(&content["schema"]), transport);
                let mut modes = modes.split(',').collect::<Vec<_>>();
                if repeat % 2 != 0 {
                    modes.reverse();
                }
                credentials.with_provider_key("groq", |key| -> Result<()> {
                    for mode in modes {
                        let mut payload = base.clone();
                        if mode == "json" {
                            payload["response_format"] = json!({"type": "json_object"});
                        } else if mode == "prompt" || mode == "compact" {
                            payload.as_object_mut().unwrap().remove("response_format");
                            if mode == "compact" {
                                payload["messages"][0]["content"] = format!(
                                    "{prompt}{COMPACT_OUTPUT_INSTRUCTION}"
                                ).into();
                            }
                        } else {
                            ensure!(mode == "strict", "unknown request mode");
                        }
                        let mut parser = TranslationStreamParser::new(&candidates);
                        let mut covered = HashSet::new();
                        let mut first_chunk = None;
                        let mut first_validated = None;
                        let mut chunks = 0;
                        let start = Instant::now();
                        let response = send_standard_payload(key, &payload, true, transport, |chunk| {
                            first_chunk.get_or_insert(start.elapsed().as_secs_f64());
                            chunks += 1;
                            for (_, region) in parser.push(chunk) {
                                first_validated.get_or_insert(start.elapsed().as_secs_f64());
                                covered.extend(region.member_ids);
                            }
                        });
                        let elapsed = start.elapsed().as_secs_f64();
                        let (response, error) = match response {
                            Ok(response) => (response, None),
                            Err(error) => (String::new(), Some(error.to_string())),
                        };
                        let valid_document = parse_response(&response, &candidates)
                            .is_ok_and(|document| document.regions.len() == candidates.len());
                        let complete = error.is_none() && covered.len() == candidates.len();
                        failures += usize::from(!complete);
                        let record = json!({
                            "input": input_index, "attempt": attempt_index, "repeat": repeat, "mode": mode,
                            "model": model, "units": candidates.len(), "covered": covered.len(),
                            "firstChunkSeconds": first_chunk, "firstValidatedSeconds": first_validated,
                            "totalSeconds": elapsed, "chunks": chunks, "complete": complete,
                            "validDocument": valid_document, "rejected": parser.rejected_count(),
                            "requestBytes": payload.to_string().len(), "response": response, "error": error,
                        });
                        writeln!(report, "{record}")?;
                        report.flush()?;
                        println!("REPLAY input={input_index} attempt={attempt_index} repeat={repeat} mode={mode} units={} covered={} first={first_validated:?} total={elapsed:.3}s chunks={chunks} complete={complete}", candidates.len(), covered.len());
                        std::thread::sleep(Duration::from_secs(pace));
                    }
                    Ok(())
                })?;
            }
        }
    }
    ensure!(
        failures == 0,
        "{failures} incomplete responses; inspect private report"
    );
    Ok(())
}
