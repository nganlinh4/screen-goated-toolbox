use super::*;

pub(super) fn open_fresh_socket_session(
    _session_index: usize,
    _generation: u64,
    settings: &S2sSettings,
    context: &S2sContextSnapshot,
    stop_signal: &Arc<AtomicBool>,
) -> Result<tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>> {
    let mut socket = connect_websocket(&settings.api_key)?;
    send_s2s_setup(&mut socket, settings, context)?;
    set_socket_short_timeout(&mut socket)?;
    wait_for_setup(&mut socket, stop_signal.clone())?;
    set_socket_nonblocking(&mut socket)?;
    Ok(socket)
}

fn send_s2s_setup(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    settings: &S2sSettings,
    context: &S2sContextSnapshot,
) -> Result<()> {
    let payload = build_s2s_setup_payload(settings, context);

    socket.write(Message::Text(payload.to_string().into()))?;
    socket.flush()?;
    Ok(())
}

fn build_s2s_setup_payload(
    settings: &S2sSettings,
    context: &S2sContextSnapshot,
) -> serde_json::Value {
    if settings.mode == S2sMode::LiveTranslate {
        return serde_json::json!({
            "setup": {
                "model": format!("models/{}", settings.model),
                "generationConfig": {
                    "responseModalities": ["AUDIO"],
                    "translationConfig": {
                        "targetLanguageCode": crate::api::realtime_audio::websocket::live_translate_target_language_code(&settings.target_language),
                        "echoTargetLanguage": true
                    }
                },
                "inputAudioTranscription": {},
                "outputAudioTranscription": {}
            }
        });
    }

    let instruction = format!(
        "You are a low-latency live interpreter. Translate every input speech segment into {}. \
         Speak only the translated content. Do not explain, summarize, answer, or add commentary. \
         Keep the output natural and concise, preserving names and technical terms. {}{}{}",
        settings.target_language,
        speed_instruction(&settings.speed),
        if settings.custom_instruction.trim().is_empty() {
            String::new()
        } else {
            format!(
                " Additional speaking instructions: {}",
                settings.custom_instruction.trim()
            )
        },
        context.text
    );
    serde_json::json!({
        "setup": {
            "model": format!("models/{}", settings.model),
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "mediaResolution": "MEDIA_RESOLUTION_LOW",
                "thinkingConfig": { "thinkingBudget": 0 },
                "speechConfig": {
                    "voiceConfig": {
                        "prebuiltVoiceConfig": {
                            "voiceName": settings.voice
                        }
                    }
                }
            },
            "systemInstruction": {
                "parts": [{ "text": instruction }]
            },
            "contextWindowCompression": {
                "slidingWindow": {}
            },
            "inputAudioTranscription": {},
            "outputAudioTranscription": {}
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(model: &str, target_language: &str) -> S2sSettings {
        S2sSettings {
            api_key: "test-key".to_string(),
            model: model.to_string(),
            mode: if model == crate::model_config::GEMINI_LIVE_TRANSLATE_API_MODEL {
                S2sMode::LiveTranslate
            } else {
                S2sMode::LegacyInterpreter
            },
            voice: "Aoede".to_string(),
            speed: "Normal".to_string(),
            custom_instruction: String::new(),
            target_language: target_language.to_string(),
        }
    }

    #[test]
    fn translate_model_setup_uses_translation_config() {
        let payload = build_s2s_setup_payload(
            &settings(
                crate::model_config::GEMINI_LIVE_TRANSLATE_API_MODEL,
                "Vietnamese",
            ),
            &S2sContextSnapshot {
                text: "legacy context must not be sent".to_string(),
            },
        );

        let setup = &payload["setup"];
        let expected_model = format!(
            "models/{}",
            crate::model_config::GEMINI_LIVE_TRANSLATE_API_MODEL
        );
        assert_eq!(setup["model"].as_str(), Some(expected_model.as_str()));
        assert!(setup.get("systemInstruction").is_none());
        assert_eq!(setup["inputAudioTranscription"], serde_json::json!({}));
        assert_eq!(setup["outputAudioTranscription"], serde_json::json!({}));
        assert_eq!(
            setup["generationConfig"]["translationConfig"]["targetLanguageCode"].as_str(),
            Some("vi")
        );
        assert_eq!(
            setup["generationConfig"]["translationConfig"]["echoTargetLanguage"].as_bool(),
            Some(true)
        );
        assert!(
            setup["generationConfig"]
                .get("inputAudioTranscription")
                .is_none()
        );
        assert!(
            setup["generationConfig"]
                .get("outputAudioTranscription")
                .is_none()
        );
    }

    #[test]
    fn translate_target_language_code_preserves_bcp47_variants() {
        let code = crate::api::realtime_audio::websocket::live_translate_target_language_code;
        assert_eq!(code("Chinese"), "zh-Hans");
        assert_eq!(code("Chinese (Traditional)"), "zh-Hant");
        assert_eq!(code("pt-BR"), "pt-BR");
        assert_eq!(code("Portuguese (Portugal)"), "pt-PT");
        assert_eq!(code("Filipino"), "fil");
        assert_eq!(code("Korean"), "ko");
    }
}

fn wait_for_setup(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    stop_signal: Arc<AtomicBool>,
) -> Result<()> {
    let started = Instant::now();
    while !stop_signal.load(Ordering::SeqCst) {
        match socket.read() {
            Ok(Message::Text(msg)) => {
                let update = parse_s2s_update(msg.as_str());
                if let Some(error) = update.error {
                    return Err(anyhow::anyhow!(error));
                }
                if update.setup_complete {
                    return Ok(());
                }
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    let update = parse_s2s_update(&text);
                    if let Some(error) = update.error {
                        return Err(anyhow::anyhow!(error));
                    }
                    if update.setup_complete {
                        return Ok(());
                    }
                }
            }
            Ok(Message::Close(frame)) => {
                return Err(anyhow::anyhow!("S2S setup socket closed: {:?}", frame));
            }
            Ok(_) => {}
            Err(error) if is_transient_socket_read_error(&error) => {
                if started.elapsed() > Duration::from_secs(15) {
                    return Err(anyhow::anyhow!("S2S setup timeout"));
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => return Err(err.into()),
        }
    }
    Err(anyhow::anyhow!("stopped"))
}

fn s2s_should_stop(stop_signal: &Arc<AtomicBool>, cancel_signal: Option<&Arc<AtomicBool>>) -> bool {
    stop_signal.load(Ordering::SeqCst)
        || cancel_signal
            .map(|cancel| cancel.load(Ordering::SeqCst))
            .unwrap_or(false)
}

pub(super) fn process_segment(
    socket: &mut tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>,
    segment: &Segment,
    params: ProcessSegmentParams<'_>,
) -> Result<SegmentOutcome> {
    let ProcessSegmentParams {
        mode,
        session_index,
        generation,
        event_tx,
        stop_signal,
        cancel_signal,
        final_attempt,
    } = params;
    let segment_id = segment.id;
    for chunk in segment.samples.chunks(FRAME_SAMPLES) {
        if s2s_should_stop(stop_signal, cancel_signal) {
            return Ok(SegmentOutcome::RetryFresh);
        }
        send_audio_chunk(socket, chunk)?;
    }
    if mode != S2sMode::LiveTranslate {
        send_audio_stream_end(socket)?;
    }

    let started = Instant::now();
    let log_tag = mode.log_tag();
    let mut last_update = Instant::now();
    let mut last_audio_at: Option<Instant> = None;
    let mut first_audio_ms: Option<u128> = None;
    let mut audio_chunks = 0usize;
    let mut text_updates = 0usize;
    while !s2s_should_stop(stop_signal, cancel_signal) {
        match socket.read() {
            Ok(Message::Text(msg)) => {
                let message_update = handle_s2s_message(segment_id, msg.as_str(), event_tx)?;
                let new_chunks = message_update.audio_chunks;
                text_updates += message_update.text_updates;
                if new_chunks > 0 && first_audio_ms.is_none() {
                    last_audio_at = Some(Instant::now());
                    first_audio_ms = Some(started.elapsed().as_millis());
                }
                if new_chunks > 0 {
                    last_audio_at = Some(Instant::now());
                }
                audio_chunks += new_chunks;
                last_update = Instant::now();
                if parse_s2s_update(msg.as_str()).turn_complete {
                    if audio_chunks == 0 && !final_attempt {
                        return Ok(if text_updates == 0 {
                            SegmentOutcome::EmptyNoInput
                        } else {
                            SegmentOutcome::RetryFresh
                        });
                    }
                    let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    return Ok(if audio_chunks > 0 {
                        SegmentOutcome::Healthy
                    } else if text_updates == 0 {
                        SegmentOutcome::EmptyNoInput
                    } else {
                        SegmentOutcome::RetryFresh
                    });
                }
            }
            Ok(Message::Binary(data)) => {
                if let Ok(text) = String::from_utf8(data.to_vec()) {
                    let message_update = handle_s2s_message(segment_id, &text, event_tx)?;
                    let new_chunks = message_update.audio_chunks;
                    text_updates += message_update.text_updates;
                    if new_chunks > 0 && first_audio_ms.is_none() {
                        last_audio_at = Some(Instant::now());
                        first_audio_ms = Some(started.elapsed().as_millis());
                    }
                    if new_chunks > 0 {
                        last_audio_at = Some(Instant::now());
                    }
                    audio_chunks += new_chunks;
                    last_update = Instant::now();
                    if parse_s2s_update(&text).turn_complete {
                        if audio_chunks == 0 && !final_attempt {
                            return Ok(if text_updates == 0 {
                                SegmentOutcome::EmptyNoInput
                            } else {
                                SegmentOutcome::RetryFresh
                            });
                        }
                        let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                        return Ok(if audio_chunks > 0 {
                            SegmentOutcome::Healthy
                        } else if text_updates == 0 {
                            SegmentOutcome::EmptyNoInput
                        } else {
                            SegmentOutcome::RetryFresh
                        });
                    }
                }
            }
            Ok(Message::Close(frame)) => {
                let detail = frame
                    .map(|f| format!("connection closed ({}: {})", f.code, f.reason))
                    .unwrap_or_else(|| "connection closed".to_string());
                eprintln!(
                    "[{}] socket-close segment={} session={} gen={} elapsed_ms={} detail={} chunks={} text_updates={}",
                    log_tag,
                    segment_id,
                    session_index,
                    generation,
                    started.elapsed().as_millis(),
                    detail,
                    audio_chunks,
                    text_updates
                );
                if audio_chunks > 0 {
                    let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    return Ok(SegmentOutcome::Healthy);
                }
                return Ok(SegmentOutcome::RetryFresh);
            }
            Ok(_) => {}
            Err(error) if is_transient_socket_read_error(&error) => {
                if audio_chunks > 0
                    && last_audio_at
                        .map(|last| last.elapsed().as_millis() >= AUDIO_IDLE_FINISH_MS)
                        .unwrap_or(false)
                {
                    let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    return Ok(SegmentOutcome::Healthy);
                }
                let source_audio_ms = samples_to_ms(segment.samples.len()) as u128;
                let no_first_audio_retry_ms =
                    grouped_first_audio_timeout_ms(source_audio_ms, text_updates);
                if audio_chunks == 0 && started.elapsed().as_millis() >= no_first_audio_retry_ms {
                    if final_attempt {
                        let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    }
                    return Ok(if text_updates == 0 {
                        SegmentOutcome::EmptyNoInput
                    } else {
                        SegmentOutcome::RetryFresh
                    });
                }
                let total_timeout_ms = no_first_audio_retry_ms.max(30_000);
                if (audio_chunks > 0 && last_update.elapsed() > Duration::from_secs(8))
                    || started.elapsed().as_millis() > total_timeout_ms
                {
                    eprintln!(
                        "[{}] done segment={} session={} gen={} elapsed_ms={} reason=timeout idle_ms={} total_timeout_ms={} source_audio_ms={} chunks={} first_audio_ms={}",
                        log_tag,
                        segment_id,
                        session_index,
                        generation,
                        started.elapsed().as_millis(),
                        last_update.elapsed().as_millis(),
                        total_timeout_ms,
                        source_audio_ms,
                        audio_chunks,
                        first_audio_ms
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string())
                    );
                    if audio_chunks == 0 && !final_attempt {
                        return Ok(SegmentOutcome::RetryFresh);
                    }
                    let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    return Ok(SegmentOutcome::Healthy);
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok(SegmentOutcome::RetryFresh)
}

pub(super) struct HandledS2sMessage {
    pub(super) audio_chunks: usize,
    text_updates: usize,
}

pub(super) fn handle_s2s_message(
    id: u64,
    message: &str,
    event_tx: &mpsc::Sender<S2sEvent>,
) -> Result<HandledS2sMessage> {
    let update = parse_s2s_update(message);
    if let Some(error) = update.error {
        let _ = event_tx.send(S2sEvent::Error { id, message: error });
        return Ok(HandledS2sMessage {
            audio_chunks: 0,
            text_updates: 0,
        });
    }
    if update.interrupted {
        let _ = event_tx.send(S2sEvent::Interrupt);
    }
    let mut text_updates = 0usize;
    if let Some(text) = update.input_transcript {
        let _ = event_tx.send(S2sEvent::InputText { id, text });
        text_updates += 1;
    }
    if let Some(text) = update.output_transcript {
        let _ = event_tx.send(S2sEvent::OutputText { id, text });
        text_updates += 1;
    }
    let chunk_count = update.audio_chunks.len();
    for bytes in update.audio_chunks {
        let _ = event_tx.send(S2sEvent::Audio { id, bytes });
    }
    Ok(HandledS2sMessage {
        audio_chunks: chunk_count,
        text_updates,
    })
}

pub(crate) struct S2sParsedUpdate {
    pub(crate) setup_complete: bool,
    pub(crate) input_transcript: Option<String>,
    pub(crate) output_transcript: Option<String>,
    pub(crate) audio_chunks: Vec<Vec<u8>>,
    pub(crate) turn_complete: bool,
    pub(crate) interrupted: bool,
    pub(crate) error: Option<String>,
}

pub(crate) fn parse_s2s_update(message: &str) -> S2sParsedUpdate {
    let mut update = S2sParsedUpdate {
        setup_complete: message.contains("setupComplete"),
        input_transcript: None,
        output_transcript: None,
        audio_chunks: Vec::new(),
        turn_complete: false,
        interrupted: false,
        error: None,
    };

    let Ok(json) = serde_json::from_str::<serde_json::Value>(message) else {
        return update;
    };

    if let Some(error) = json.get("error") {
        update.error = error
            .get("message")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .or_else(|| Some(error.to_string()));
        return update;
    }

    let Some(server_content) = json.get("serverContent") else {
        return update;
    };

    update.turn_complete = server_content
        .get("turnComplete")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || server_content
            .get("generationComplete")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
    update.interrupted = server_content
        .get("interrupted")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    update.input_transcript = server_content
        .get("inputTranscription")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    update.output_transcript = server_content
        .get("outputTranscription")
        .and_then(|value| value.get("text"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(parts) = server_content
        .get("modelTurn")
        .and_then(|value| value.get("parts"))
        .and_then(|value| value.as_array())
    {
        for part in parts {
            if let Some(inline) = part.get("inlineData")
                && let Some(data) = inline.get("data").and_then(|value| value.as_str())
                && let Ok(bytes) = general_purpose::STANDARD.decode(data)
            {
                update.audio_chunks.push(bytes);
            }
        }
    }

    update
}
