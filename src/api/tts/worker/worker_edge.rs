// Edge TTS (Bing Speech) handler.

use minimp3::{Decoder, Frame};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::sync::{Arc, atomic::Ordering};
use std::time::Duration;
use tungstenite::{Message, client};

use super::super::manager::TtsManager;
use super::super::types::AudioEvent;
use super::super::utils::{clear_tts_loading_state, clear_tts_state};
use super::resample_audio;
use crate::APP;
use isolang::Language;

pub(super) fn handle_edge_tts(
    manager: Arc<TtsManager>,
    request: super::super::types::QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    let text = request.req.text.clone();
    let generation = request.generation;
    let manager_clone = manager.clone();

    eprintln!("[TTS Edge] Starting Edge TTS for {} chars", text.len());

    // Get Settings
    let (voice_name, pitch, rate) = {
        let app = APP.lock().unwrap();
        let settings = &app.config.edge_tts_settings;

        let lang_detect = whatlang::detect(&text);

        let mut voice = "en-US-AriaNeural".to_string();

        let code_2 = lang_detect
            .and_then(|info| Language::from_639_3(info.lang().code()))
            .and_then(|l| l.to_639_1())
            .unwrap_or("en");

        for config in &settings.voice_configs {
            if config.language_code == code_2 {
                voice = config.voice_name.clone();
                break;
            }
        }

        (voice, settings.pitch, settings.rate)
    };

    eprintln!(
        "[TTS Edge] Using voice: {}, pitch: {}, rate: {}",
        voice_name, pitch, rate
    );

    let mut socket = match connect_edge_websocket() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[TTS Edge] ERROR: {}", e);
            let _ = tx.send(AudioEvent::End);
            clear_tts_state(request.req.hwnd);
            return;
        }
    };

    let request_id = format!(
        "{:032x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );

    // Send config
    let config_msg = format!(
        "X-Timestamp:{}\r\nContent-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n{{\"context\":{{\"synthesis\":{{\"audio\":{{\"metadataoptions\":{{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"false\"}},\"outputFormat\":\"audio-24khz-48kbitrate-mono-mp3\"}}}}}}}}",
        chrono::Utc::now().format("%a %b %d %Y %H:%M:%S GMT+0000 (Coordinated Universal Time)")
    );

    if socket.send(Message::Text(config_msg.into())).is_err() {
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(request.req.hwnd);
        return;
    }

    // Build SSML
    let pitch_str = if pitch >= 0 {
        format!("+{}Hz", pitch)
    } else {
        format!("{}Hz", pitch)
    };
    let rate_str = if rate >= 0 {
        format!("+{}%", rate)
    } else {
        format!("{}%", rate)
    };

    let escaped_text = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");

    let ssml = format!(
        "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'>\
        <voice name='{}'>\
        <prosody pitch='{}' rate='{}' volume='+0%'>{}</prosody>\
        </voice></speak>",
        voice_name, pitch_str, rate_str, escaped_text
    );

    let ssml_msg = format!(
        "X-RequestId:{}\r\nContent-Type:application/ssml+xml\r\nX-Timestamp:{}Z\r\nPath:ssml\r\n\r\n{}",
        request_id,
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3f"),
        ssml
    );

    if socket.send(Message::Text(ssml_msg.into())).is_err() {
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(request.req.hwnd);
        return;
    }

    clear_tts_loading_state(request.req.hwnd);

    // Read audio data
    let mut mp3_data: Vec<u8> = Vec::new();

    loop {
        if generation < manager_clone.interrupt_generation.load(Ordering::SeqCst) {
            break;
        }

        match socket.read() {
            Ok(Message::Binary(data)) => {
                if data.len() >= 2 {
                    let header_len = u16::from_be_bytes([data[0], data[1]]) as usize;
                    let audio_start = 2 + header_len;
                    if data.len() > audio_start {
                        let header = &data[2..audio_start];
                        if header.windows(11).any(|w| w == b"Path:audio\r") {
                            mp3_data.extend_from_slice(&data[audio_start..]);
                        }
                    }
                }
            }
            Ok(Message::Text(text)) => {
                let text = text.as_str();
                if text.contains("Path:turn.end") {
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
            _ => {}
        }
    }

    let _ = socket.close(None);

    if mp3_data.is_empty() {
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(request.req.hwnd);
        return;
    }

    // Decode MP3 to PCM
    let mut decoder = Decoder::new(Cursor::new(mp3_data));
    let mut all_samples: Vec<i16> = Vec::new();
    let mut source_sample_rate = 24000u32;

    loop {
        if generation < manager_clone.interrupt_generation.load(Ordering::SeqCst) {
            let _ = tx.send(AudioEvent::End);
            clear_tts_state(request.req.hwnd);
            return;
        }
        match decoder.next_frame() {
            Ok(Frame {
                data,
                sample_rate,
                channels,
                ..
            }) => {
                source_sample_rate = sample_rate as u32;
                if channels == 2 {
                    for chunk in data.chunks(2) {
                        let sample = ((chunk[0] as i32 + chunk[1] as i32) / 2) as i16;
                        all_samples.push(sample);
                    }
                } else {
                    all_samples.extend_from_slice(&data);
                }
            }
            Err(minimp3::Error::Eof) => break,
            Err(_) => break,
        }
    }

    let audio_bytes = if source_sample_rate != 24000 {
        let resampled = resample_audio(&all_samples, source_sample_rate, 24000);
        let mut bytes = Vec::with_capacity(resampled.len() * 2);
        for sample in resampled {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    } else {
        let mut bytes = Vec::with_capacity(all_samples.len() * 2);
        for sample in all_samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    };

    let chunk_size = 24000;
    for chunk in audio_bytes.chunks(chunk_size) {
        if generation < manager_clone.interrupt_generation.load(Ordering::SeqCst) {
            break;
        }
        let _ = tx.send(AudioEvent::Data(chunk.to_vec()));
    }

    let _ = tx.send(AudioEvent::End);
    clear_tts_state(request.req.hwnd);
}

fn connect_edge_websocket(
) -> Result<tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>, String> {
    fn generate_sec_ms_gec(trusted_token: &str) -> String {
        let win_epoch_offset: u64 = 11_644_473_600;
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let adjusted = now_secs + win_epoch_offset;
        let rounded = adjusted - (adjusted % 300);
        let ticks = rounded * 10_000_000;
        let input = format!("{}{}", ticks, trusted_token);
        let hash = Sha256::digest(input.as_bytes());
        hash.iter().map(|b| format!("{:02X}", b)).collect()
    }

    let trusted_token = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";
    let sec_ms_gec_version = "1-143.0.3650.75";
    let connection_id = format!(
        "{:032x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let sec_ms_gec = generate_sec_ms_gec(trusted_token);
    let wss_url = format!(
        "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1?TrustedClientToken={}&ConnectionId={}&Sec-MS-GEC={}&Sec-MS-GEC-Version={}",
        trusted_token, connection_id, sec_ms_gec, sec_ms_gec_version
    );

    eprintln!("[TTS Edge] Connecting to Bing Speech WebSocket...");

    let connector = native_tls::TlsConnector::new()
        .map_err(|e| format!("TLS connector creation failed: {:?}", e))?;

    let host = "speech.platform.bing.com";
    let stream = std::net::TcpStream::connect(format!("{}:443", host))
        .map_err(|e| format!("TCP connection to {} failed: {:?}", host, e))?;

    let tls_stream = connector
        .connect(host, stream)
        .map_err(|e| format!("TLS handshake failed: {:?}", e))?;

    let ws_request = tungstenite::http::Request::builder()
        .uri(&wss_url)
        .header("Host", host)
        .header("Origin", "chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold")
        .header("Pragma", "no-cache")
        .header("Cache-Control", "no-cache")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.3650.75 Safari/537.36 Edg/143.0.3650.75")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", tungstenite::handshake::client::generate_key())
        .body(())
        .unwrap();

    let (socket, _) = client(ws_request, tls_stream)
        .map_err(|e| format!("WebSocket connection failed: {:?}", e))?;

    eprintln!("[TTS Edge] WebSocket connected successfully");
    Ok(socket)
}
