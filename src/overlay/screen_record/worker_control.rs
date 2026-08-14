use std::io::{BufRead, BufReader, Read as _, Write as _};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use sgt_recorder_protocol::{
    Command, MAX_JSON_BYTES, MAX_REQUEST_LINE_BYTES, PROTOCOL_VERSION, RESPONSE_PREFIX, Request,
    Response, valid_token,
};

const TOKEN_ENV: &str = "SGT_RECORDER_LAUNCH_TOKEN";
const MAX_REPLAY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_MEDIA_INPUT_BYTES: u64 = 64 * 1024 * 1024 * 1024;

pub(crate) fn run() -> Result<()> {
    let token = std::env::var(TOKEN_ENV).context("recorder launch token is missing")?;
    if !valid_token(&token) {
        bail!("recorder launch token is invalid");
    }
    // The token must never remain discoverable through this process's
    // environment after bootstrap.
    unsafe {
        std::env::remove_var(TOKEN_ENV);
    }
    crate::initialization::init_com_and_dpi();
    drop(crate::APP.lock().unwrap());
    crate::api::gemini_live::init_gemini_live();
    crate::api::tts::init_tts();
    crate::model_config::trigger_ollama_model_scan();

    let stdin = std::io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let mut output = std::io::stdout();
    let mut previous_request_id = 0_u64;
    loop {
        let Some(line) = read_bounded_line(&mut input, MAX_REQUEST_LINE_BYTES)? else {
            break;
        };
        let request = match serde_json::from_slice::<Request>(&line) {
            Ok(request) => request,
            Err(error) => bail!("malformed recorder request: {error}"),
        };
        let request_id = request.request_id;
        let mut shutdown = false;
        let response = match request.validate(&token, previous_request_id) {
            Ok(()) => {
                previous_request_id = request_id;
                shutdown = matches!(&request.command, Command::Shutdown);
                match handle(request.command) {
                    Ok(result) => Response {
                        protocol_version: PROTOCOL_VERSION,
                        token: token.clone(),
                        request_id,
                        result: Some(result),
                        error: None,
                    },
                    Err(error) => Response {
                        protocol_version: PROTOCOL_VERSION,
                        token: token.clone(),
                        request_id,
                        result: None,
                        error: Some(format!("{error:#}")),
                    },
                }
            }
            Err(error) => Response {
                protocol_version: PROTOCOL_VERSION,
                token: token.clone(),
                request_id,
                result: None,
                error: Some(error),
            },
        };
        let body = serde_json::to_vec(&response)?;
        if body.len() > MAX_JSON_BYTES {
            bail!("recorder response exceeds protocol limit");
        }
        output.write_all(RESPONSE_PREFIX.as_bytes())?;
        output.write_all(&body)?;
        output.write_all(b"\n")?;
        output.flush()?;
        if shutdown {
            break;
        }
    }
    super::cleanup_on_app_exit();
    Ok(())
}

fn handle(command: Command) -> Result<serde_json::Value> {
    match command {
        Command::Ping => Ok(serde_json::json!({ "status": "ready" })),
        Command::Show => {
            super::show_screen_record();
            for attempt in 0..100 {
                if super::is_screen_record_visible() {
                    return Ok(serde_json::json!({ "status": "shown" }));
                }
                if attempt == 99 {
                    bail!("recorder window did not become visible");
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            unreachable!()
        }
        Command::Toggle => {
            super::toggle_recording();
            Ok(serde_json::json!({ "status": "toggled" }))
        }
        Command::UpdateSettings => {
            crate::APP.lock().unwrap().config = crate::load_config();
            super::update_settings();
            Ok(serde_json::json!({ "status": "updated" }))
        }
        Command::EvaluateScript { script } => {
            for attempt in 0..100 {
                if super::post_script(script.clone()) {
                    return Ok(serde_json::json!({ "status": "dispatched" }));
                }
                if attempt == 99 {
                    bail!("recorder window is not ready");
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            unreachable!()
        }
        Command::QueueVideoDrop { path, action } => {
            super::queue_video_drop_action(path, action.as_str().to_string());
            Ok(serde_json::json!({ "status": "queued" }))
        }
        Command::QueueAudioDrop { path } => {
            super::queue_audio_drop_action(path);
            Ok(serde_json::json!({ "status": "queued" }))
        }
        Command::QueueSubtitleDrop { path } => {
            super::queue_subtitle_drop_action(path);
            Ok(serde_json::json!({ "status": "queued" }))
        }
        Command::NotifyAudioReleased { reason } => {
            super::notify_external_audio_capture_released(&reason);
            Ok(serde_json::json!({ "status": "scheduled" }))
        }
        Command::Cleanup | Command::Shutdown => {
            super::cleanup_on_app_exit();
            Ok(serde_json::json!({ "status": "stopped" }))
        }
        Command::ExportReplay {
            path,
            runs,
            keep_outputs,
        } => run_export_replay(&path, runs, keep_outputs),
        Command::GtNarrationTest {
            input_wav,
            target_language,
        } => {
            require_regular_input(Path::new(&input_wav), MAX_MEDIA_INPUT_BYTES)?;
            super::run_gt_narration_test_cli(&input_wav, &target_language)
                .map_err(anyhow::Error::msg)?;
            Ok(serde_json::json!({ "status": "complete" }))
        }
    }
}

fn run_export_replay(path: &str, runs: u16, keep_outputs: bool) -> Result<serde_json::Value> {
    let path = Path::new(path);
    require_regular_input(path, MAX_REPLAY_BYTES)?;
    let file = std::fs::File::open(path)?;
    let mut raw = String::new();
    file.take(MAX_REPLAY_BYTES + 1).read_to_string(&mut raw)?;
    if raw.len() as u64 > MAX_REPLAY_BYTES {
        bail!("recorder replay exceeds its size limit");
    }
    let payload: serde_json::Value = serde_json::from_str(&raw)?;
    let mut results = Vec::with_capacity(runs as usize);
    for _ in 0..runs {
        let started = Instant::now();
        match super::native_export::start_native_export(payload.clone()) {
            Ok(result) => {
                let output_path = result
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                results.push(serde_json::json!({
                    "wallSeconds": started.elapsed().as_secs_f64(),
                    "result": result,
                }));
                if !keep_outputs && let Some(output_path) = output_path {
                    let _ = std::fs::remove_file(output_path);
                }
            }
            Err(error) => results.push(serde_json::json!({
                "wallSeconds": started.elapsed().as_secs_f64(),
                "error": error,
            })),
        }
    }
    Ok(serde_json::json!({ "runs": results }))
}

fn require_regular_input(path: &Path, maximum: u64) -> Result<()> {
    if !path.is_absolute() {
        bail!("recorder input path must be absolute");
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > maximum || is_reparse_point(&metadata) {
        bail!("recorder input is not a safe regular file");
    }
    Ok(())
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

fn read_bounded_line(reader: &mut impl BufRead, maximum: usize) -> Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                bail!("recorder request ended before newline")
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        if line.len().saturating_add(take) > maximum {
            reader.consume(take);
            while newline.is_none() {
                let available = reader.fill_buf()?;
                if available.is_empty() {
                    break;
                }
                let next = available.iter().position(|byte| *byte == b'\n');
                let consumed = next.map_or(available.len(), |index| index + 1);
                reader.consume(consumed);
                if next.is_some() {
                    break;
                }
            }
            bail!("recorder request exceeds protocol limit");
        }
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline.is_some() {
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

#[cfg(test)]
mod framing_tests {
    use super::*;

    #[test]
    fn request_reader_accepts_exact_limit_and_rejects_one_more() {
        let mut exact = vec![b'x'; MAX_REQUEST_LINE_BYTES - 1];
        exact.push(b'\n');
        assert_eq!(
            read_bounded_line(&mut std::io::Cursor::new(exact), MAX_REQUEST_LINE_BYTES)
                .unwrap()
                .unwrap()
                .len(),
            MAX_REQUEST_LINE_BYTES - 1
        );

        let mut oversized = vec![b'x'; MAX_REQUEST_LINE_BYTES];
        oversized.push(b'\n');
        assert!(
            read_bounded_line(&mut std::io::Cursor::new(oversized), MAX_REQUEST_LINE_BYTES)
                .is_err()
        );
    }
}
