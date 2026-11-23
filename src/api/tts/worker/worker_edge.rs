// Edge TTS (Bing Speech) handler.

use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Instant;
use tungstenite::Message;

use super::super::manager::TtsManager;
use super::super::types::AudioEvent;
use super::open_weights::{fail_request, stream_pcm_samples};
use crate::APP;

#[path = "edge_transport.rs"]
mod edge_transport;

pub(super) fn handle_edge_tts(
    manager: Arc<TtsManager>,
    request: super::super::types::QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    let text = request.req.text.clone();
    let manager_clone = manager.clone();

    eprintln!("[TTS Edge] Starting Edge TTS for {} chars", text.len());

    let (voice_name, pitch, rate) =
        edge_voice_settings(&request, &text, crate::lang_detect::detect_language);

    eprintln!(
        "[TTS Edge] Using voice: {}, pitch: {}, rate: {}",
        voice_name, pitch, rate
    );

    if request.is_cancelled(&manager) {
        let _ = tx.send(AudioEvent::End);
        return;
    }
    let started = Instant::now();
    let mut socket = match connect_edge_websocket(started, Arc::clone(&manager), request.clone()) {
        Ok(s) => s,
        Err(e) => {
            if request.is_cancelled(&manager) {
                let _ = tx.send(AudioEvent::End);
            } else {
                fail_request("Edge", request.req.hwnd, &tx, e);
            }
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

    if edge_transport::send_message(
        &mut socket,
        Message::Text(config_msg.into()),
        started,
        &|| request.is_cancelled(&manager),
    )
    .is_err()
    {
        if request.is_cancelled(&manager) {
            let _ = tx.send(AudioEvent::End);
        } else {
            fail_request(
                "Edge",
                request.req.hwnd,
                &tx,
                "Edge connection failed while sending configuration",
            );
        }
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

    if edge_transport::send_message(
        &mut socket,
        Message::Text(ssml_msg.into()),
        started,
        &|| request.is_cancelled(&manager),
    )
    .is_err()
    {
        if request.is_cancelled(&manager) {
            let _ = tx.send(AudioEvent::End);
        } else {
            fail_request(
                "Edge",
                request.req.hwnd,
                &tx,
                "Edge connection failed while sending speech request",
            );
        }
        return;
    }

    let received = edge_transport::receive_audio(
        || socket.read(),
        || request.is_cancelled(&manager_clone),
        || started.elapsed() >= edge_transport::SESSION_BUDGET,
    );
    drop(socket);
    if request.is_cancelled(&manager_clone) {
        let _ = tx.send(AudioEvent::End);
        return;
    }
    let mp3_data = match received {
        Ok(audio) => audio,
        Err(error) => {
            fail_request("Edge", request.req.hwnd, &tx, error);
            return;
        }
    };

    let (all_samples, source_sample_rate) =
        match super::audio_utils::decode_edge_mp3_to_pcm(mp3_data, || {
            request.is_cancelled(&manager_clone)
        }) {
            Ok(Some(audio)) => audio,
            Ok(None) => {
                let _ = tx.send(AudioEvent::End);
                return;
            }
            Err(error) => {
                fail_request(
                    "Edge",
                    request.req.hwnd,
                    &tx,
                    format!("Edge MP3 decode failed: {error:#}"),
                );
                return;
            }
        };

    stream_pcm_samples(
        &manager_clone,
        &request,
        &tx,
        all_samples,
        source_sample_rate,
    );
}

/// Voice, pitch and rate for one request. Language detection may wait on the
/// network, so it runs before the app settings are locked and the lock is held
/// only to copy the settings.
fn edge_voice_settings(
    request: &super::super::types::QueuedRequest,
    text: &str,
    detect: impl FnOnce(&str) -> Option<String>,
) -> (String, i32, i32) {
    if let Some(profile) = request.req.profile.as_ref() {
        let detected_code = profile
            .language_code_override
            .clone()
            .unwrap_or_else(|| detect(text).unwrap_or_else(|| "eng".to_string()));
        let (voice, code_2, _, source) =
            crate::api::tts::utils::resolve_edge_voice_for_language(profile, Some(&detected_code));
        eprintln!(
            "[TTS Edge][VoiceRoute] detected_6393={} edge_lang={} source={} voice={}",
            detected_code, code_2, source, voice
        );
        return (
            voice,
            profile.edge_settings.pitch,
            profile.edge_settings.rate,
        );
    }
    let detected_code = detect(text).unwrap_or_else(|| "eng".to_string());
    let code_2 = isolang::Language::from_639_3(&detected_code)
        .and_then(|language| language.to_639_1())
        .unwrap_or("en");
    let app = APP.lock().unwrap();
    let settings = &app.config.edge_tts_settings;
    let voice = settings
        .voice_configs
        .iter()
        .find(|config| config.language_code == code_2)
        .map_or_else(
            || "en-US-AriaNeural".to_string(),
            |config| config.voice_name.clone(),
        );
    (voice, settings.pitch, settings.rate)
}

fn connect_edge_websocket(
    started: Instant,
    manager: Arc<TtsManager>,
    request: super::super::types::QueuedRequest,
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
    let stream = crate::api::gemini_live::transport::connect_tcp_until(
        host,
        443,
        started + edge_transport::SETUP_BUDGET,
        &|| request.is_cancelled(&manager),
    )
    .map_err(|e| format!("TCP connection to {} failed: {:?}", host, e))?;
    if request.is_cancelled(&manager) || started.elapsed() >= edge_transport::SETUP_BUDGET {
        return Err("Edge connection setup timed out or was cancelled".to_owned());
    }
    stream.set_nonblocking(true).map_err(|e| e.to_string())?;
    let cancelled = || request.is_cancelled(&manager);
    let setup_deadline = started + edge_transport::SETUP_BUDGET;
    let tls_stream = crate::api::gemini_live::transport::tls_handshake(
        &connector,
        host,
        stream,
        setup_deadline,
        &cancelled,
    )
    .map_err(|failure| format!("Edge {failure}"))?;

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

    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(edge_transport::MAX_AUDIO_BYTES + 65536))
        .max_frame_size(Some(edge_transport::MAX_AUDIO_BYTES + 65536));
    let socket = crate::api::gemini_live::transport::websocket_handshake(
        ws_request,
        tls_stream,
        Some(config),
        setup_deadline,
        &cancelled,
    )
    .map_err(|failure| format!("Edge {failure}"))?;
    if request.is_cancelled(&manager) || started.elapsed() >= edge_transport::SETUP_BUDGET {
        return Err("Edge connection setup timed out or was cancelled".to_owned());
    }
    eprintln!("[TTS Edge] WebSocket connected successfully");
    Ok(socket)
}

#[cfg(test)]
pub(super) mod settings_tests {
    use super::edge_voice_settings;
    use crate::APP;
    use crate::api::tts::types::{QueuedRequest, TtsRequest};
    use std::time::Duration;

    /// Whether another thread can take the app settings lock now, so a lock
    /// held by the calling thread shows up as a failure instead of a hang.
    pub(crate) fn app_lock_is_free() -> bool {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _guard = APP.lock();
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(5)).is_ok()
    }

    #[test]
    fn app_settings_are_not_locked_while_the_language_is_detected() {
        let request = QueuedRequest {
            req: TtsRequest {
                _id: 1,
                text: "xin chào".to_string(),
                hwnd: 0,
                is_realtime: false,
                profile: None,
            },
            generation: 0,
            cancel: Default::default(),
            artifact: false,
        };
        let mut detected = false;
        let (voice, _, _) = edge_voice_settings(&request, "xin chào", |text| {
            assert!(
                app_lock_is_free(),
                "the app settings lock is held during detection"
            );
            detected = text == "xin chào";
            Some("vie".to_string())
        });
        assert!(detected);
        assert!(!voice.is_empty());
    }
}
