//! Task-trace harness: run a multi-step task live, EXECUTE each action, and save
//! the screenshot the model sees at every step to disk — so we can visually
//! ground-truth whether its self-assessment (when it calls `done`) matches
//! reality, and characterize how it verifies / loops / self-corrects.
//!
//! `--cc-task-trace --cc-task "..."`. Env: CC_TRACE_DIR (default `cc-trace`),
//! CC_DRY (don't actually execute), CC_SYS (system-prompt override), CC_MAX_STEPS.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose};
use tungstenite::Message;

use crate::api::realtime_audio::websocket::{
    is_transient_socket_read_error, set_socket_nonblocking, set_socket_short_timeout,
};

use super::executor;
use super::protocol::{
    ServerEvent, build_setup, parse_server_message, realtime_text, realtime_video_jpeg_b64,
    tool_response,
};
use super::session::{self, Sock, capture_frame_jpeg, connect_ws, send};

/// Deliberately neutral baseline prompt — mentions the screenshot loop and
/// "done only when achieved", but does NOT scaffold verification, so we can see
/// the model's NATURAL goal-checking behavior before designing steering.
const BASELINE_SYS: &str = "You control this Windows PC and can see its screen. Carry out the user's \
request by calling the tools. All screen coordinates are normalized 0-1000 (x=0 left..1000 right, \
y=0 top..1000 bottom). After each action you receive a fresh screenshot of the result. Briefly state \
what you observe and your next step, then call the tool. Call `done` only when the goal has actually \
been achieved.";

pub fn run(task: &str) -> Result<()> {
    let dir = std::env::var("CC_TRACE_DIR").unwrap_or_else(|_| "cc-trace".to_string());
    std::fs::create_dir_all(&dir).ok();
    let dry = std::env::var("CC_DRY").is_ok();
    let sys = std::env::var("CC_SYS").unwrap_or_else(|_| BASELINE_SYS.to_string());
    let max_steps: usize = std::env::var("CC_MAX_STEPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(18);

    eprintln!("[trace] task={task:?}");
    eprintln!("[trace] dry={dry} dir={dir} max_steps={max_steps}");

    let key = session::load_key()?;
    let mut socket = connect_ws(&key).context("connect")?;
    send(&mut socket, build_setup(&sys))?;
    wait_for_setup(&mut socket)?;
    set_socket_nonblocking(&mut socket)?;

    let mut step = 0usize;
    save_and_send_frame(&mut socket, &dir, step)?;
    eprintln!("[trace] step {step:02}: initial screen saved");
    send(&mut socket, realtime_text(task))?;

    let deadline = Instant::now() + Duration::from_secs(180);
    let mut reasoning = String::new();

    while Instant::now() < deadline && step < max_steps {
        let frame = match socket.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            Ok(Message::Close(f)) => {
                eprintln!("[trace] socket closed: {f:?}");
                break;
            }
            Ok(_) => continue,
            Err(e) if is_transient_socket_read_error(&e) => continue,
            Err(e) => {
                eprintln!("[trace] read error: {e}");
                break;
            }
        };
        for ev in parse_server_message(&frame) {
            match ev {
                ServerEvent::ModelText(t) | ServerEvent::OutputTranscript(t) => reasoning.push_str(&t),
                ServerEvent::InputTranscript(t) => eprintln!("[trace] heard: {t}"),
                ServerEvent::ToolCall { id, name, args } => {
                    step += 1;
                    let say = reasoning.trim();
                    if !say.is_empty() {
                        eprintln!("[trace] step {step:02} SAYS: {say}");
                    }
                    reasoning.clear();

                    if name == "done" {
                        let summary = args.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                        eprintln!("[trace] step {step:02} DONE: {summary}");
                        send(&mut socket, tool_response(&id, &name, serde_json::json!({"ok": true})))?;
                        save_and_send_frame(&mut socket, &dir, step)?;
                        eprintln!("[trace] FINAL screen saved as step-{step:02}.jpg");
                        return Ok(());
                    }

                    let result = if dry {
                        serde_json::json!({"ok": true, "note": "dry-run, not executed"})
                    } else {
                        executor::execute(&name, &args)
                    };
                    eprintln!("[trace] step {step:02} ACTION {name}({args}) -> {result}");
                    send(&mut socket, tool_response(&id, &name, result))?;
                    std::thread::sleep(Duration::from_millis(350)); // let the UI settle
                    save_and_send_frame(&mut socket, &dir, step)?;
                }
                _ => {}
            }
        }
    }
    eprintln!("[trace] STOPPED at step {step} (timeout or max-steps without `done`)");
    Ok(())
}

fn save_and_send_frame(socket: &mut Sock, dir: &str, step: usize) -> Result<()> {
    let (jpeg, _geom) = capture_frame_jpeg()?;
    let path = format!("{dir}/step-{step:02}.jpg");
    std::fs::write(&path, &jpeg).with_context(|| format!("write {path}"))?;
    let b64 = general_purpose::STANDARD.encode(&jpeg);
    send(socket, realtime_video_jpeg_b64(&b64))
}

fn wait_for_setup(socket: &mut Sock) -> Result<()> {
    set_socket_short_timeout(socket)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if Instant::now() > deadline {
            anyhow::bail!("timed out waiting for setupComplete");
        }
        let text = match socket.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            Ok(Message::Close(f)) => anyhow::bail!("server closed during setup: {f:?}"),
            Ok(_) => continue,
            Err(e) if is_transient_socket_read_error(&e) => continue,
            Err(e) => anyhow::bail!("setup read error: {e}"),
        };
        for ev in parse_server_message(&text) {
            if matches!(ev, ServerEvent::SetupComplete) {
                return Ok(());
            }
        }
    }
}
