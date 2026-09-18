//! Isolated native tool-grounding diagnostics; never executes a desktop effect.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::gemini_live::ready_session::{ConnectedLiveSocket, LivePoll};
use crate::api::gemini_live::setup::{LiveSetupBuilder, MediaResolution, TranscriptionMode};
use crate::overlay::computer_control::vision_contract::encode_jpeg;

#[derive(Deserialize)]
struct Plan {
    models: Vec<String>,
    thinking_level: Option<String>,
    deadline_seconds: Option<u64>,
    ws_base: Option<String>,
    cases: Option<Vec<String>>,
    #[serde(default = "default_repetitions")]
    frame_count: usize,
    speech: Option<BTreeMap<String, PathBuf>>,
    #[serde(default = "default_repetitions")]
    repetitions: usize,
}

fn default_repetitions() -> usize {
    1
}

#[test]
#[ignore = "requires LIVE_GROUNDING_PLAN and real provider credentials"]
fn native_live_grounding_probe() -> Result<()> {
    let plan: Plan =
        serde_json::from_slice(&std::fs::read(std::env::var("LIVE_GROUNDING_PLAN")?)?)?;
    ensure!((1..=5).contains(&plan.repetitions), "invalid repetitions");
    ensure!((1..=4).contains(&plan.frame_count), "invalid frame count");
    ensure!(
        plan.thinking_level
            .as_deref()
            .is_none_or(|level| matches!(level, "LOW" | "MEDIUM" | "HIGH")),
        "invalid thinking level"
    );
    ensure!(
        (1..=180).contains(&plan.deadline_seconds.unwrap_or(45)),
        "invalid deadline"
    );
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::env::var("LIVE_GROUNDING_OUTPUT")?)?;
    let manifest = super::manifest::Manifest::load()?;
    manifest.validate()?;
    let credentials = super::setup::Credentials::load()?;
    for repeat in 0..plan.repetitions {
        for case in &manifest.coordinate_cases {
            if plan
                .cases
                .as_ref()
                .is_some_and(|ids| !ids.contains(&case.id))
            {
                continue;
            }
            let original = image::open(manifest.image_path(&case.image))?;
            let original = image::DynamicImage::ImageRgb8(original.to_rgb8());
            let resized = if original.width().max(original.height()) > 1536 {
                original.resize(1536, 1536, image::imageops::FilterType::Triangle)
            } else {
                original.clone()
            };
            let jpeg = encode_jpeg(&resized)?;
            for model in &plan.models {
                let start = Instant::now();
                let result = credentials
                    .with_provider_key("gemini-live", |key| locate(model, key, &jpeg, case, &plan));
                let mut record = json!({"model":model,"case":case.id,"repeat":repeat,
                    "thinking_level":plan.thinking_level,"deadline_seconds":plan.deadline_seconds.unwrap_or(45),
                    "latency_ms":start.elapsed().as_millis(),"wire_width":resized.width(),
                    "wire_height":resized.height(),"media_resolution":"HIGH",
                    "ground_truth":case.box_px,"source_width":original.width(),"source_height":original.height(),"ws_base":plan.ws_base,"frame_count":plan.frame_count,"spoken_input":plan.speech.is_some()});
                match result {
                    Ok(value) => {
                        let args = &value["args"];
                        let point = args["x"].as_f64().zip(args["y"].as_f64());
                        record["hit"] = (args["visible"] == true
                            && point
                                .map(|(x, y)| {
                                    super::scoring::coordinate_point(
                                        x,
                                        y,
                                        original.width(),
                                        original.height(),
                                        case.box_px,
                                    )
                                    .hit
                                })
                                .unwrap_or(false))
                        .into();
                        record["result"] = value;
                    }
                    Err(error) => record["error"] = format!("{error:#}").into(),
                }
                writeln!(output, "{record}")?;
                output.flush()?;
                println!(
                    "GROUNDING {} {} hit={} error={}",
                    model, case.id, record["hit"], record["error"]
                );
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
    Ok(())
}

fn locate(
    model: &str,
    key: &str,
    jpeg: &[u8],
    case: &super::manifest::CoordinateCase,
    plan: &Plan,
) -> Result<Value> {
    let speech = if let Some(files) = &plan.speech {
        let path = files
            .get(&case.id)
            .ok_or_else(|| anyhow::anyhow!("missing public speech fixture"))?;
        let mut wav = hound::WavReader::open(path)?;
        ensure!(
            wav.spec().sample_rate == 16000
                && wav.spec().channels == 1
                && wav.spec().bits_per_sample == 16,
            "speech must be mono PCM16 at 16kHz"
        );
        Some(
            wav.samples::<i16>()
                .collect::<std::result::Result<Vec<_>, _>>()?,
        )
    } else {
        None
    };
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
    let mut builder = LiveSetupBuilder::new(model)
        .media_resolution(MediaResolution::High).voice("Aoede")
        .transcription(if speech.is_some() { TranscriptionMode::Both } else { TranscriptionMode::Output })
        .system_instruction("Locate the requested target in the current screenshot. Use report_point with its center, normalized to 0-1000 on each image axis. If absent, set visible=false. Do not guess. This only reports a point; it does not click or change anything.")
        .setup_field("tools", json!([{"functionDeclarations":[{
            "name":"report_point","description":"Report the visible target center on the current screenshot.",
            "behavior":if profile.require_interaction_idle { "NON_BLOCKING" } else { "BLOCKING" },"parameters":{"type":"OBJECT","properties":{
                "x":{"type":"NUMBER"},"y":{"type":"NUMBER"},"visible":{"type":"BOOLEAN"}},
                "required":["x","y","visible"]}}]}]));
    if profile.thinking.is_some() {
        builder = builder.thinking_override(
            json!({"thinkingLevel":plan.thinking_level.as_deref().unwrap_or("LOW")}),
        );
    }
    if speech.is_some() {
        builder = builder.setup_field(
            "realtimeInputConfig",
            json!({"automaticActivityDetection":{"disabled":true}}),
        );
    }
    let connection = match plan.ws_base.as_deref() {
        Some(base) => ConnectedLiveSocket::connect_to(base, key)?,
        None => ConnectedLiveSocket::connect(key)?,
    };
    let mut session = connection.activate(builder.build())?;
    if speech.is_some() {
        session.send_json(&json!({"realtimeInput":{"activityStart":{}}}))?;
    }
    for frame_index in 0..plan.frame_count {
        session.send_json(
            &json!({"realtimeInput":{"video":{"mimeType":"image/jpeg","data":STANDARD.encode(jpeg)}}}),
        )?;
        if frame_index + 1 < plan.frame_count {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    std::thread::sleep(Duration::from_millis(500));
    if let Some(samples) = speech {
        for chunk in samples.chunks(320) {
            session.send_audio_pcm(chunk, 16000)?;
            std::thread::sleep(Duration::from_millis(20));
        }
        session.send_json(&json!({"realtimeInput":{"activityEnd":{}}}))?;
    } else {
        session.send_json(&json!({"realtimeInput":{"text":format!("Target: {}\nContext: {}",case.target,case.context)}}))?;
    }
    let start = Instant::now();
    let mut text = String::new();
    let mut input_text = String::new();
    let mut calls = Vec::new();
    let mut statuses = Vec::new();
    let mut wire = Vec::new();
    while start.elapsed() < Duration::from_secs(plan.deadline_seconds.unwrap_or(45)) {
        match session.poll_observed(|raw| {
            if let Ok(mut value) = serde_json::from_str::<Value>(raw) {
                if let Some(resumption) = value
                    .get_mut("sessionResumptionUpdate")
                    .and_then(Value::as_object_mut)
                {
                    resumption.remove("newHandle");
                }
                if let Some(parts) = value
                    .pointer_mut("/serverContent/modelTurn/parts")
                    .and_then(Value::as_array_mut)
                {
                    for part in parts {
                        if let Some(data) = part.get_mut("inlineData") {
                            *data = json!({"omitted":true});
                        }
                    }
                }
                if wire.len() < 300 {
                    wire.push(value);
                }
            }
        })? {
            LivePoll::Frame(frame) => {
                if let Some(transcript) = &frame.input_transcript {
                    input_text.push_str(transcript);
                }
                if let Some(transcript) = &frame.output_transcript {
                    text.push_str(transcript);
                }
                if let Some(status) = &frame.interaction_status {
                    statuses.push(status.clone());
                }
                for call in &frame.tool_calls {
                    calls.push(json!({"name":call.name,"args":call.args,"at_ms":start.elapsed().as_millis()}));
                    session.send_json(&json!({"toolResponse":{"functionResponses":[{
                        "id":call.id,"name":call.name,"response":{"recorded":true}}]}}))?;
                }
                let completed = if profile.require_interaction_idle {
                    frame.interaction_status.as_deref() == Some("IDLE")
                } else {
                    frame.turn_complete
                };
                if completed {
                    let point = calls
                        .iter()
                        .find(|call| call["name"] == "report_point")
                        .ok_or_else(|| anyhow::anyhow!("completed without report_point: transcript={text}, statuses={statuses:?}, wire={wire:?}"))?;
                    return Ok(
                        json!({"args":point["args"],"calls":calls,"transcript":text,"input_transcript":input_text,"statuses":statuses,"wire":wire}),
                    );
                }
            }
            LivePoll::ServerError(error) => bail!("{error}"),
            LivePoll::PeerClosed(close) => bail!("peer closed: {close:?}"),
            _ => {}
        }
    }
    bail!("deadline: calls={calls:?}, transcript={text}, statuses={statuses:?}, wire={wire:?}")
}
