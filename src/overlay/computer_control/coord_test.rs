//! Coordinate-system debug harness. Asks the model to "click" three KNOWN screen
//! points (top-left, center, bottom-right) and logs the RAW coordinates it
//! returns, plus how they'd map under competing conventions — so we can prove
//! whether the model uses 0–1000 normalized coords or raw frame pixels. Does NOT
//! actually click. `--cc-coord-test`.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tungstenite::Message;

use crate::api::realtime_audio::websocket::{
    is_transient_socket_read_error, set_socket_nonblocking, set_socket_short_timeout,
};

use super::protocol::{
    ServerEvent, build_setup, parse_server_message, realtime_text, realtime_video_jpeg_b64,
    tool_response,
};
use super::session::{self, Sock, capture_frame, connect_ws, send};

pub fn run() -> Result<()> {
    let key = session::load_key()?;
    let mut socket = connect_ws(&key).context("connect")?;
    send(
        &mut socket,
        build_setup(
            "Report click coordinates as integers normalized to a 0-1000 grid: x=0 left edge, \
             x=1000 right edge, y=0 top edge, y=1000 bottom edge.",
        ),
    )?;
    wait_for_setup(&mut socket)?;
    set_socket_nonblocking(&mut socket)?;

    let (frame, geom) = capture_frame()?;
    let (fw, fh) = (geom.frame_w as f64, geom.frame_h as f64);
    eprintln!(
        "[coord] frame sent = {} x {} px",
        geom.frame_w, geom.frame_h
    );
    eprintln!(
        "[coord] if PIXELS: center≈({:.0},{:.0}) bottom-right≈({:.0},{:.0})",
        fw / 2.0,
        fh / 2.0,
        fw,
        fh
    );
    eprintln!("[coord] if 0-1000: center≈(500,500) bottom-right≈(1000,1000)");
    send(&mut socket, realtime_video_jpeg_b64(&frame))?;
    send(
        &mut socket,
        realtime_text(
            "Look at the screenshot. Call the click tool THREE times in order, reporting the \
             coordinates you would click: (1) the exact TOP-LEFT corner of the screen, (2) the \
             exact CENTER of the screen, (3) the exact BOTTOM-RIGHT corner. Then call done.",
        ),
    )?;

    let deadline = Instant::now() + Duration::from_secs(40);
    let mut n = 0;
    while Instant::now() < deadline {
        let text = match socket.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Binary(b)) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            Ok(Message::Close(f)) => {
                eprintln!("[coord] closed: {f:?}");
                break;
            }
            Ok(_) => continue,
            Err(e) if is_transient_socket_read_error(&e) => continue,
            Err(e) => {
                eprintln!("[coord] read error: {e}");
                break;
            }
        };
        for ev in parse_server_message(&text) {
            match ev {
                ServerEvent::ToolCall { id, name, args } => {
                    if name == "done" {
                        send(
                            &mut socket,
                            tool_response(&id, &name, serde_json::json!({"ok": true})),
                        )?;
                        eprintln!("[coord] done");
                        return Ok(());
                    }
                    n += 1;
                    let label = match n {
                        1 => "top-left ",
                        2 => "center   ",
                        3 => "btm-right",
                        _ => "?        ",
                    };
                    let x = args.get("x").and_then(|v| v.as_f64()).unwrap_or(-1.0);
                    let y = args.get("y").and_then(|v| v.as_f64()).unwrap_or(-1.0);
                    eprintln!(
                        "[coord] {label}: RAW=({x:>6.1},{y:>6.1})  | frac-if-1000=({:.2},{:.2}) | frac-if-px=({:.2},{:.2})",
                        x / 1000.0,
                        y / 1000.0,
                        x / fw,
                        y / fh,
                    );
                    // Actually move the cursor (no click) and read back where it
                    // landed, to verify the 0-1000 -> screen mapping end to end.
                    super::executor::move_to(x, y);
                    std::thread::sleep(Duration::from_millis(60));
                    let mut pt = windows::Win32::Foundation::POINT::default();
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
                    }
                    eprintln!(
                        "[coord]   -> cursor landed at screen px ({}, {})",
                        pt.x, pt.y
                    );
                    send(
                        &mut socket,
                        tool_response(&id, &name, serde_json::json!({"ok": true, "note": "debug"})),
                    )?;
                }
                ServerEvent::ModelText(t) => eprintln!("[coord] model: {t}"),
                _ => {}
            }
        }
    }
    eprintln!("[coord] (timed out / finished)");
    Ok(())
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
