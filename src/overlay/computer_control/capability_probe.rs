//! Opt-in provider protocol diagnostics with the production full tool catalog.
//! Only explicitly listed fixture files can be read; no device effects execute.
//! This is not a substitute for visible-runtime acceptance.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use anyhow::{Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::gemini_live::ready_session::{ConnectedLiveSocket, LivePoll};

#[derive(Deserialize)]
struct Plan {
    models: Vec<String>,
    thinking_level: Option<String>,
    cases: Vec<Case>,
    ws_base: Option<String>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    prompt: String,
    files: Vec<PathBuf>,
    delay_ms: u64,
}

#[test]
#[ignore = "requires CONTROL_CAPABILITY_PLAN and real provider credentials"]
fn control_provider_capability_probe() -> Result<()> {
    let plan: Plan =
        serde_json::from_slice(&std::fs::read(std::env::var("CONTROL_CAPABILITY_PLAN")?)?)?;
    ensure!(
        plan.thinking_level
            .as_deref()
            .is_none_or(|level| matches!(level, "LOW" | "MEDIUM" | "HIGH")),
        "invalid thinking level"
    );
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::env::var("CONTROL_CAPABILITY_OUTPUT")?)?;
    let credentials = crate::catalog_benchmark::setup::Credentials::load()?;
    let cancelled = AtomicBool::new(false);
    let _engine = super::engine::SessionGuard::start(&cancelled)?;
    let baseline = super::uia_task::build_setup(None, false, false)?;
    for case in &plan.cases {
        ensure!(case.delay_ms <= 10_000, "unbounded tool delay");
        let files = case
            .files
            .iter()
            .map(std::fs::canonicalize)
            .collect::<std::io::Result<Vec<_>>>()?;
        for model in &plan.models {
            let started = Instant::now();
            let result = credentials.with_provider_key("gemini-live", |key| {
                run(model, key, &baseline, case, &files, &plan)
            });
            let record = match result {
                Ok(value) => {
                    json!({"model":model,"thinking_level":plan.thinking_level,"case":case.id,"latency_ms":started.elapsed().as_millis(),"result":value})
                }
                Err(error) => {
                    json!({"model":model,"case":case.id,"latency_ms":started.elapsed().as_millis(),"error":format!("{error:#}")})
                }
            };
            writeln!(output, "{record}")?;
            output.flush()?;
            println!(
                "CONTROL_CAPABILITY {} {} error={}",
                model, case.id, record["error"]
            );
        }
    }
    Ok(())
}

fn run(
    model: &str,
    key: &str,
    baseline: &Value,
    case: &Case,
    files: &[PathBuf],
    plan: &Plan,
) -> Result<Value> {
    let profile = crate::model_config::live_endpoint_profile(model)
        .ok_or_else(|| anyhow::anyhow!("missing endpoint profile"))?;
    ensure!(
        profile.protocol == Some("native-audio"),
        "probe requires native audio protocol"
    );
    ensure!(
        matches!(
            profile.thinking,
            None | Some(crate::model_config::LiveThinkingConfig::Level(
                "minimal" | "low"
            ))
        ),
        "probe requires LOW-compatible or unconfigured thinking"
    );
    let mut setup = baseline.clone();
    setup["setup"]["model"] = format!("models/{model}").into();
    let generation = setup["setup"]["generationConfig"].as_object_mut().unwrap();
    generation.remove("thinkingConfig");
    if profile.thinking.is_some() {
        generation.insert(
            "thinkingConfig".into(),
            json!({"thinkingLevel":plan.thinking_level.as_deref().unwrap_or("LOW"),"includeThoughts":true}),
        );
    }
    {
        for tool in setup["setup"]["tools"].as_array_mut().unwrap() {
            if let Some(declarations) = tool["functionDeclarations"].as_array_mut() {
                for declaration in declarations {
                    declaration["behavior"] = if profile.require_interaction_idle {
                        "NON_BLOCKING"
                    } else {
                        "BLOCKING"
                    }
                    .into();
                }
            }
        }
    }
    let connection = match plan.ws_base.as_deref() {
        Some(base) => ConnectedLiveSocket::connect_to(base, key)?,
        None => ConnectedLiveSocket::connect(key)?,
    };
    let mut session = connection.activate(setup)?;
    session.send_json(&super::protocol::realtime_text(&case.prompt))?;
    let start = Instant::now();
    let mut text = String::new();
    let mut calls = Vec::new();
    let mut boundaries = Vec::new();
    let mut pending: Vec<(Instant, Value)> = Vec::new();
    let mut completed_at = None;
    while start.elapsed() < Duration::from_secs(90) {
        let now = Instant::now();
        let mut ready = Vec::new();
        pending.retain(|(at, response)| {
            if now >= *at {
                ready.push(response.clone());
                false
            } else {
                true
            }
        });
        for response in ready {
            session.send_json(&response)?;
        }
        match session.poll()? {
            LivePoll::Frame(frame) => {
                if let Some(transcript) = &frame.output_transcript {
                    text.push_str(transcript);
                }
                for call in &frame.tool_calls {
                    let path = call.args["path"]
                        .as_str()
                        .and_then(|p| std::fs::canonicalize(p).ok());
                    let allowed = call.name == "read_text_file"
                        && path.as_ref().is_some_and(|p| files.contains(p));
                    let result = if allowed {
                        super::executor::execute_ex(
                            &call.name,
                            &call.args,
                            &super::human_input::HumanProfile::instant(),
                            &AtomicBool::new(false),
                        )
                    } else {
                        json!({"ok":false,"error":"Diagnostic boundary: only the supplied fixture files may be read; no effects execute."})
                    };
                    calls.push(json!({"name":call.name,"args":call.args,"allowed":allowed,"result":result,"at_ms":start.elapsed().as_millis(),"after_completion":completed_at.is_some()}));
                    ensure!(calls.len() <= 20, "too many calls");
                    pending.push((
                        Instant::now() + Duration::from_millis(case.delay_ms),
                        super::protocol::tool_response(&call.id, &call.name, result),
                    ));
                }
                if frame.turn_complete
                    || frame.generation_complete
                    || frame.interaction_status.is_some()
                {
                    boundaries.push(json!({"at_ms":start.elapsed().as_millis(),"turn":frame.turn_complete,"generation":frame.generation_complete,"status":frame.interaction_status,"pending":pending.len()}));
                }
                if frame.finite_response_complete(profile.require_interaction_idle)
                    && pending.is_empty()
                    && !text.is_empty()
                {
                    completed_at.get_or_insert_with(Instant::now);
                }
            }
            LivePoll::ServerError(error) => bail!("{error}"),
            LivePoll::PeerClosed(close) => bail!("peer closed: {close:?}"),
            _ => {}
        }
        if completed_at.is_some_and(|at: Instant| at.elapsed() >= Duration::from_secs(2)) {
            return Ok(json!({"text":text,"calls":calls,"boundaries":boundaries,"completed":true}));
        }
    }
    Ok(json!({"text":text,"calls":calls,"boundaries":boundaries,"completed":false,"deadline":true}))
}
