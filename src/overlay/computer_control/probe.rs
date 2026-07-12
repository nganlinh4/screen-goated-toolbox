//! De-risk probe for Computer Control: open a real Gemini Live session, stream
//! one screenshot + a text task, and log the model's tool calls / transcripts /
//! usage. Verifies (against the live endpoint) that the model emits function
//! calls while screen + task are streamed.
//!
//! By default it does NOT execute actions (every tool call is answered with a
//! "not executed" note + a fresh screenshot). Set `CC_EXECUTE=1` to actually
//! drive mouse/keyboard via the executor. Set `CC_PROBE_FULL=1` to declare the
//! production toolkit, and `--cc-turns-json '["...", "..."]'` for a scripted
//! multi-turn session. Single-threaded on purpose.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tungstenite::Message;

use crate::api::realtime_audio::websocket::{
    is_transient_socket_read_error, set_socket_nonblocking, set_socket_short_timeout,
};

use super::executor;
use super::protocol::{
    self, ServerEvent, build_setup, parse_server_message, realtime_text, realtime_video_jpeg_b64,
    tool_response,
};
use super::session::{self, Sock, capture_frame, connect_ws, send};

const PROBE_SECS: u64 = 45;

const SYSTEM_INSTRUCTION: &str = "You are controlling this Windows computer and can see its screen. \
Use the click and type_text tools (coordinates NORMALIZED to a 0-1000 grid over the screenshot: x=0 left, \
x=1000 right, y=0 top, y=1000 bottom) to carry out the user's request, then call done with a short summary. \
This is a TEST probe: do NOT perform destructive actions; if unsure, just describe what you see and call done.";

pub fn run(tasks: &[String]) -> Result<()> {
    let Some(first_task) = tasks.first() else {
        anyhow::bail!("probe needs at least one task");
    };
    let key = session::load_key()?;
    eprintln!(
        "[cc-probe] model={} scripted_turns={} first_task={first_task:?}",
        protocol::MODEL,
        tasks.len()
    );

    let mut socket = connect_ws(&key).context("connect websocket")?;
    let setup_payload = if std::env::var("CC_MINIMAL").is_ok() {
        // Bare repo-shape payload + CC_MODEL override, for endpoint/model bisecting.
        let model = std::env::var("CC_MODEL").unwrap_or_else(|_| protocol::MODEL.to_string());
        eprintln!("[cc-probe] (minimal) model={model}");
        serde_json::json!({"setup": {
            "model": format!("models/{model}"),
            "generationConfig": {"responseModalities": ["AUDIO"]}
        }})
    } else if std::env::var("CC_PROBE_FULL").is_ok() {
        eprintln!("[cc-probe] using full production Computer Control toolkit");
        super::uia_task::build_setup(None, false, false)
    } else {
        build_setup(SYSTEM_INSTRUCTION)
    };
    let (function_count, instruction_chars) = setup_profile(&setup_payload);
    eprintln!(
        "[cc-probe] sending setup ({} bytes, {function_count} functions, {instruction_chars} instruction chars)",
        setup_payload.to_string().len(),
    );
    send(&mut socket, setup_payload).context("send setup")?;

    wait_for_setup(&mut socket)?;
    set_socket_nonblocking(&mut socket)?;

    let execute_enabled = std::env::var("CC_EXECUTE").is_ok();
    eprintln!(
        "[cc-probe] execution {}",
        if execute_enabled {
            "ENABLED (will drive mouse/keyboard)"
        } else {
            "disabled (observe only)"
        }
    );

    let (frame, geom) = capture_frame().context("capture initial frame")?;
    eprintln!(
        "[cc-probe] sending first frame ({} b64 chars, {}x{}) + task",
        frame.len(),
        geom.frame_w,
        geom.frame_h
    );
    send(&mut socket, realtime_video_jpeg_b64(&frame))?;
    send(&mut socket, realtime_text(first_task))?;

    let mut deadline = Instant::now() + Duration::from_secs(PROBE_SECS);
    let mut audio_bytes = 0usize;
    let mut tool_calls = 0usize;
    let mut turn_tool_calls = 0usize;
    let mut turn_index = 0usize;

    while Instant::now() < deadline {
        let text = match socket.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            Ok(Message::Close(frame)) => {
                eprintln!("[cc-probe] socket closed by server: {frame:?}");
                break;
            }
            Ok(_) => continue,
            Err(e) if is_transient_socket_read_error(&e) => continue,
            Err(e) => {
                eprintln!("[cc-probe] read error: {e}");
                break;
            }
        };
        for ev in parse_server_message(&text) {
            match ev {
                ServerEvent::Audio(pcm) => audio_bytes += pcm.len(),
                ServerEvent::ToolCall { id, name, args } => {
                    tool_calls += 1;
                    turn_tool_calls += 1;
                    eprintln!(
                        "[cc-probe] turn {}/{} TOOLCALL #{} (total #{tool_calls}) {name}({args}) id={id}",
                        turn_index + 1,
                        tasks.len(),
                        turn_tool_calls
                    );
                    if name == "done" {
                        send(
                            &mut socket,
                            tool_response(&id, &name, serde_json::json!({"ok": true})),
                        )?;
                        eprintln!(
                            "[cc-probe] turn {}/{} complete after {} tool call(s)",
                            turn_index + 1,
                            tasks.len(),
                            turn_tool_calls
                        );
                        turn_index += 1;
                        if let Some(next_task) = tasks.get(turn_index) {
                            if let Ok((next_frame, _)) = capture_frame() {
                                send(&mut socket, realtime_video_jpeg_b64(&next_frame))?;
                            }
                            eprintln!(
                                "[cc-probe] starting scripted turn {}/{}: {next_task:?}",
                                turn_index + 1,
                                tasks.len()
                            );
                            send(&mut socket, realtime_text(next_task))?;
                            turn_tool_calls = 0;
                            deadline = Instant::now() + Duration::from_secs(PROBE_SECS);
                            continue;
                        }
                        eprintln!("[cc-probe] all scripted turns complete");
                        return finish(audio_bytes, tool_calls, turn_index);
                    }
                    let resp = if execute_enabled {
                        let r = executor::execute(&name, &args);
                        eprintln!("[cc-probe]   executed -> {r}");
                        r
                    } else {
                        serde_json::json!({"ok": true, "note": "probe: not executed (set CC_EXECUTE=1 to act)"})
                    };
                    send(&mut socket, tool_response(&id, &name, resp))?;
                    if let Ok((f, _g)) = capture_frame() {
                        send(&mut socket, realtime_video_jpeg_b64(&f))?;
                    }
                }
                other => log_event(&other),
            }
        }
    }
    finish(audio_bytes, tool_calls, turn_index)
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
            Ok(Message::Close(frame)) => anyhow::bail!("server closed during setup: {frame:?}"),
            Ok(_) => continue,
            Err(e) if is_transient_socket_read_error(&e) => continue,
            Err(e) => anyhow::bail!("setup read error: {e}"),
        };
        for ev in parse_server_message(&text) {
            if matches!(ev, ServerEvent::SetupComplete) {
                eprintln!("[cc-probe] setupComplete");
                return Ok(());
            }
            log_event(&ev);
        }
    }
}

fn finish(audio_bytes: usize, tool_calls: usize, turns_completed: usize) -> Result<()> {
    eprintln!(
        "[cc-probe] DONE — completed turns: {turns_completed}, tool calls: {tool_calls}, audio samples: {audio_bytes}"
    );
    if tool_calls == 0 {
        eprintln!(
            "[cc-probe] NOTE: zero tool calls — the model did not emit functionCalls this run."
        );
    }
    Ok(())
}

fn log_event(ev: &ServerEvent) {
    match ev {
        ServerEvent::ModelText(t) => eprintln!("[cc-probe] model: {t}"),
        ServerEvent::Thought(t) => eprintln!("[cc-probe] thinks: {t}"),
        ServerEvent::OutputTranscript(t) => eprintln!("[cc-probe] model says: {t}"),
        ServerEvent::InputTranscript(t) => eprintln!("[cc-probe] heard: {t}"),
        ServerEvent::TurnComplete => eprintln!("[cc-probe] turnComplete"),
        ServerEvent::Interrupted => eprintln!("[cc-probe] interrupted (barge-in)"),
        ServerEvent::ToolCancellation(ids) => eprintln!("[cc-probe] toolCallCancellation {ids:?}"),
        ServerEvent::GoAway { time_left } => eprintln!("[cc-probe] goAway timeLeft={time_left}"),
        ServerEvent::SessionResumption { handle, resumable } => {
            eprintln!("[cc-probe] sessionResumption handle={handle:?} resumable={resumable}")
        }
        ServerEvent::Usage(u) => eprintln!("[cc-probe] usageMetadata {u}"),
        ServerEvent::Other(s) => eprintln!("[cc-probe] (unparsed) {s}"),
        ServerEvent::Audio(_) | ServerEvent::SetupComplete | ServerEvent::ToolCall { .. } => {}
    }
}

fn setup_profile(setup: &serde_json::Value) -> (usize, usize) {
    let function_count = setup
        .pointer("/setup/tools")
        .and_then(serde_json::Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool.get("functionDeclarations"))
                .filter_map(serde_json::Value::as_array)
                .map(Vec::len)
                .sum()
        })
        .unwrap_or(0);
    let instruction_chars = setup
        .pointer("/setup/systemInstruction/parts/0/text")
        .and_then(serde_json::Value::as_str)
        .map(|text| text.chars().count())
        .unwrap_or(0);
    (function_count, instruction_chars)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::setup_profile;

    #[test]
    fn profiles_setup_without_assuming_tool_groups() {
        let setup = json!({"setup": {
            "systemInstruction": {"parts": [{"text": "short"}]},
            "tools": [
                {"functionDeclarations": [{"name": "one"}, {"name": "two"}]},
                {"googleSearch": {}},
                {"functionDeclarations": [{"name": "three"}]}
            ]
        }});
        assert_eq!(setup_profile(&setup), (3, 5));
    }
}
