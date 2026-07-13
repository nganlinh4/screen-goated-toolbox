use super::*;

pub(super) fn open_fresh_socket_session(
    _session_index: usize,
    _generation: u64,
    settings: &S2sSettings,
    context: &S2sContextSnapshot,
    stop_signal: &Arc<AtomicBool>,
    cancel_signal: Option<&Arc<AtomicBool>>,
) -> Result<ReadyLiveSession> {
    let connected = connect_s2s_socket(settings)?;
    activate_s2s_socket(
        connected,
        settings,
        context,
        Duration::from_millis(50),
        || s2s_should_stop(stop_signal, cancel_signal),
    )
}

pub(super) fn connect_s2s_socket(settings: &S2sSettings) -> Result<ConnectedLiveSocket> {
    ConnectedLiveSocket::connect(&settings.api_key)
}

pub(super) fn activate_s2s_socket(
    connected: ConnectedLiveSocket,
    settings: &S2sSettings,
    context: &S2sContextSnapshot,
    active_read_timeout: Duration,
    cancelled: impl FnMut() -> bool,
) -> Result<ReadyLiveSession> {
    let payload = build_s2s_setup_payload(settings, context);
    connected.activate_with(
        payload,
        OpenOptions {
            active_read_timeout,
            ..OpenOptions::default()
        },
        cancelled,
    )
}

fn build_s2s_setup_payload(
    settings: &S2sSettings,
    context: &S2sContextSnapshot,
) -> serde_json::Value {
    if settings.mode == S2sMode::LiveTranslate {
        return crate::api::realtime_audio::websocket::build_live_translate_setup_value(
            &settings.model,
            &settings.target_language,
        );
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
    crate::api::gemini_live::setup::LiveSetupBuilder::new(&settings.model)
        .media_resolution(crate::api::gemini_live::setup::MediaResolution::Low)
        .voice(&settings.voice)
        .system_instruction(&instruction)
        .transcription(crate::api::gemini_live::setup::TranscriptionMode::Both)
        .context_window_compression()
        .build()
}

fn s2s_should_stop(stop_signal: &Arc<AtomicBool>, cancel_signal: Option<&Arc<AtomicBool>>) -> bool {
    stop_signal.load(Ordering::SeqCst)
        || cancel_signal
            .map(|cancel| cancel.load(Ordering::SeqCst))
            .unwrap_or(false)
}

pub(super) fn process_segment(
    session: &mut ReadyLiveSession,
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
        session.send_audio_pcm(chunk, 16_000)?;
    }
    if mode != S2sMode::LiveTranslate {
        session.end_audio_stream()?;
    }

    let started = Instant::now();
    let log_tag = mode.log_tag();
    let mut last_update = Instant::now();
    let mut last_audio_at: Option<Instant> = None;
    let mut first_audio_ms: Option<u128> = None;
    let mut audio_chunks = 0usize;
    let mut text_updates = 0usize;
    while !s2s_should_stop(stop_signal, cancel_signal) {
        match session.poll() {
            Ok(LivePoll::PeerClosed(frame)) => {
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
            Ok(LivePoll::Frame(frame)) => {
                let response_complete = frame.response_complete();
                let message_update = handle_s2s_frame(segment_id, *frame, event_tx)?;
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
                if response_complete {
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
            Ok(LivePoll::ServerError(error)) => {
                let _ = event_tx.send(S2sEvent::Error {
                    id: segment_id,
                    message: error.message,
                });
                if audio_chunks > 0 {
                    let _ = event_tx.send(S2sEvent::Done { id: segment_id });
                    return Ok(SegmentOutcome::Healthy);
                }
                return Ok(SegmentOutcome::RetryFresh);
            }
            Ok(LivePoll::Unparsed { .. }) => {}
            Ok(LivePoll::Idle) => {
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
            Err(err) => return Err(err),
        }
    }
    Ok(SegmentOutcome::RetryFresh)
}

pub(super) struct HandledS2sMessage {
    pub(super) audio_chunks: usize,
    text_updates: usize,
}

pub(super) fn handle_s2s_frame(
    id: u64,
    frame: LiveServerFrame,
    event_tx: &mpsc::Sender<S2sEvent>,
) -> Result<HandledS2sMessage> {
    let update = parsed_update_from_frame(frame);
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

#[derive(Default)]
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
    let Ok(frame) = crate::api::gemini_live::server_frame::parse_server_frame(message) else {
        return S2sParsedUpdate::default();
    };
    parsed_update_from_frame(frame)
}

fn parsed_update_from_frame(frame: LiveServerFrame) -> S2sParsedUpdate {
    let turn_complete = frame.response_complete();
    S2sParsedUpdate {
        setup_complete: frame.setup_complete,
        input_transcript: trimmed_non_empty(frame.input_transcript),
        output_transcript: trimmed_non_empty(frame.output_transcript),
        audio_chunks: frame.audio_chunks,
        turn_complete,
        interrupted: frame.interrupted,
        error: frame.error,
    }
}

fn trimmed_non_empty(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
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
