use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Cursor;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use image::ImageFormat;
use serde::Deserialize;

use super::{parse_point, point_schema};
use crate::api::{TranslateImageRequest, translate_image_streaming};

#[derive(Deserialize)]
struct Manifest {
    models: Vec<Model>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Model {
    id: String,
    provider: String,
    full_name: String,
    #[serde(default)]
    min_interval_ms: u64,
}

#[derive(Deserialize)]
struct Case {
    image: String,
    target: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_true")]
    visible: bool,
    box_px: Option<[f64; 4]>,
}

fn default_true() -> bool {
    true
}

fn env_key(name: &str) -> String {
    std::env::var(name).unwrap_or_default().trim().to_string()
}

fn computer_control_jpeg(path: &Path) -> anyhow::Result<Vec<u8>> {
    let image = image::open(path)?;
    let mut cursor = Cursor::new(Vec::new());
    image.write_to(&mut cursor, ImageFormat::Jpeg)?;
    Ok(cursor.into_inner())
}

fn point_prompt(target: &str) -> String {
    format!(
        "Find this target in the image: {target}. Output ONLY JSON \
{{\"x\": <int>, \"y\": <int>, \"what\": \"<2-4 words: what is AT that location, e.g. empty cell, an X, a button>\"}} \
- x,y are the CENTER on a 0-1000 grid (x: 0 left to 1000 right; y: 0 top to 1000 bottom). If the target is not \
visible, output {{\"error\": \"not visible\"}}."
    )
}

fn wait_for_interval(last: &mut HashMap<String, Instant>, model: &Model) {
    let interval = Duration::from_millis(model.min_interval_ms);
    if let Some(previous) = last.get(&model.provider) {
        std::thread::sleep(interval.saturating_sub(previous.elapsed()));
    }
    last.insert(model.provider.clone(), Instant::now());
}

#[test]
#[ignore = "requires live provider keys and CC_VISION_BENCH_MANIFEST"]
fn live_coordinate_benchmark() {
    let manifest_path = std::env::var("CC_VISION_BENCH_MANIFEST")
        .expect("CC_VISION_BENCH_MANIFEST must point to a benchmark manifest");
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("read benchmark manifest"))
            .expect("parse benchmark manifest");
    let output_path = std::env::var("CC_VISION_BENCH_OUTPUT").ok();
    if let Some(path) = &output_path {
        std::fs::write(path, b"").expect("clear benchmark output");
    }
    let groq_key = env_key("GROQ_API_KEY");
    let gemini_key = env_key("GEMINI_API_KEY");
    let cerebras_key = env_key("CEREBRAS_API_KEY");
    {
        let mut app = crate::APP.lock().expect("app state");
        app.config.api_key.clone_from(&groq_key);
        app.config.gemini_api_key.clone_from(&gemini_key);
        app.config.cerebras_api_key = cerebras_key;
    }
    if manifest.models.iter().any(|m| m.provider == "gemini-live") {
        crate::api::gemini_live::init_gemini_live();
    }

    let mut last_call = HashMap::new();
    for case in &manifest.cases {
        let jpeg = computer_control_jpeg(Path::new(&case.image)).expect("encode benchmark frame");
        let decoded = image::load_from_memory(&jpeg)
            .expect("decode benchmark frame")
            .to_rgba8();
        let width = f64::from(decoded.width());
        let height = f64::from(decoded.height());
        for model in &manifest.models {
            wait_for_interval(&mut last_call, model);
            let started = Instant::now();
            let result = translate_image_streaming(
                TranslateImageRequest {
                    groq_api_key: &groq_key,
                    gemini_api_key: &gemini_key,
                    prompt: point_prompt(&case.target),
                    model: model.full_name.clone(),
                    provider: model.provider.clone(),
                    image: decoded.clone(),
                    original_bytes: Some(jpeg.clone()),
                    streaming_enabled: false,
                    use_json_format: false,
                    response_schema: Some(point_schema()),
                    cancel_token: None,
                },
                |_| {},
            );
            let latency_ms = started.elapsed().as_millis();
            let record = match result {
                Ok(answer) => match parse_point(&answer) {
                    Some((x, y)) => {
                        let px = x / 1000.0 * width;
                        let py = y / 1000.0 * height;
                        if !case.visible {
                            serde_json::json!({
                                "model": model.id, "image": case.image,
                                "target": case.target, "category": case.category,
                                "tags": case.tags, "latency_ms": latency_ms,
                                "parsed": true, "hit": false, "expected_visible": false,
                                "x_1000": x, "y_1000": y, "answer": answer,
                            })
                        } else {
                            let [bx, by, bw, bh] =
                                case.box_px.expect("visible benchmark cases require box_px");
                            let center_x = bx + bw / 2.0;
                            let center_y = by + bh / 2.0;
                            serde_json::json!({
                                "model": model.id, "image": case.image,
                                "target": case.target,
                                "category": case.category, "tags": case.tags,
                                "latency_ms": latency_ms,
                                "parsed": true,
                                "hit": px >= bx && px <= bx + bw && py >= by && py <= by + bh,
                                "expected_visible": true,
                                "x_1000": x,
                                "y_1000": y,
                                "error_px": ((px-center_x).powi(2)+(py-center_y).powi(2)).sqrt(),
                                "answer": answer,
                            })
                        }
                    }
                    None => serde_json::json!({
                        "model": model.id, "image": case.image, "target": case.target,
                        "category": case.category, "tags": case.tags, "latency_ms": latency_ms,
                        "parsed": false, "hit": !case.visible,
                        "expected_visible": case.visible, "answer": answer,
                    }),
                },
                Err(error) => serde_json::json!({
                    "model": model.id, "image": case.image, "target": case.target,
                    "category": case.category, "tags": case.tags, "latency_ms": latency_ms,
                    "parsed": false, "hit": false, "expected_visible": case.visible,
                    "error": error.to_string(),
                }),
            };
            println!("BENCH_RESULT {record}");
            if let Some(path) = &output_path {
                let mut output = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .expect("open benchmark output");
                writeln!(output, "{record}").expect("write benchmark record");
            }
        }
    }
}
