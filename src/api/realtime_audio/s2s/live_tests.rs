//! Explicitly enabled production-route acceptance with public synthetic inputs.
use super::*;

#[test]
#[ignore = "calls live endpoints; requires GEMINI_API_KEY, SGT_LIVE_TEST_MODEL, SGT_PUBLIC_SPEECH_WAV and SGT_LIVE_TEST_OUTPUT"]
fn live_modality_provider_acceptance() -> Result<()> {
    let api_key = std::env::var("GEMINI_API_KEY")?;
    let model = std::env::var("SGT_LIVE_TEST_MODEL")?;
    let mut reader = hound::WavReader::open(std::env::var("SGT_PUBLIC_SPEECH_WAV")?)?;
    let spec = reader.spec();
    anyhow::ensure!(spec.channels == 1 && spec.sample_rate == 16_000 && spec.bits_per_sample == 16);
    let speech = reader
        .samples::<i16>()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let output = std::path::PathBuf::from(std::env::var("SGT_LIVE_TEST_OUTPUT")?);
    let mut results = serde_json::Map::new();
    crate::APP.lock().unwrap().config.gemini_api_key = api_key.clone();
    crate::api::gemini_live::init_gemini_live();

    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
        96,
        64,
        image::Rgb([255, 0, 0]),
    ))
    .write_to(&mut png, image::ImageFormat::Png)?;
    {
        let (kind, prompt, image_data, audio_data) = (
            "vision",
            "Name the dominant color in this image. Reply with one word.",
            Some((png.into_inner(), "image/png".to_string())),
            None,
        );
        let started = Instant::now();
        let text = crate::api::gemini_live::gemini_live_generate(
            crate::api::gemini_live::GeminiLiveGenerateRequest {
                api_key: api_key.clone(),
                model: model.clone(),
                text: prompt.to_string(),
                instruction: String::new(),
                image_data,
                audio_data,
                streaming_enabled: true,
                ui_language: "en",
                cancel_token: None,
                request_timeout: Some(Duration::from_secs(60)),
            },
            |_| {},
        )?;
        anyhow::ensure!(!text.trim().is_empty(), "{kind}: empty response");
        results.insert(
            kind.to_string(),
            serde_json::json!({"elapsed_ms": started.elapsed().as_millis(), "text": text}),
        );
        std::fs::write(&output, serde_json::to_vec_pretty(&results)?)?;
    }

    let audio_model = crate::model_config::get_all_models()
        .iter()
        .find(|row| {
            row.full_name == model
                && row.model_type == crate::model_config::ModelType::Audio
                && !crate::model_config::model_is_non_llm(&row.id)
        })
        .ok_or_else(|| anyhow::anyhow!("custom audio row is required"))?;
    let preset = crate::config::Preset {
        blocks: vec![crate::config::ProcessingBlock {
            block_type: "audio".to_string(),
            model: audio_model.id.clone(),
            prompt: "Transcribe the supplied speech. Return only the transcript.".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let started = Instant::now();
    let transcript = crate::api::audio::execute_audio_processing_logic(
        &preset,
        std::fs::read(std::env::var("SGT_PUBLIC_SPEECH_WAV")?)?,
    )?;
    results.insert(
        "audio-preset".to_string(),
        serde_json::json!({"elapsed_ms": started.elapsed().as_millis(), "text": transcript}),
    );
    std::fs::write(&output, serde_json::to_vec_pretty(&results)?)?;
    anyhow::ensure!(
        !transcript.trim().is_empty(),
        "audio preset returned no text"
    );

    let settings = S2sSettings {
        api_key,
        model: model.clone(),
        mode: S2sMode::LegacyInterpreter,
        voice: "Aoede".to_string(),
        speed: "Normal".to_string(),
        target_language: "Korean".to_string(),
        custom_instruction: String::new(),
    };
    let (event_tx, event_rx) = mpsc::channel();
    let resources = types::S2sSessionResources {
        event_tx,
        stop_signal: Arc::new(AtomicBool::new(false)),
        settings,
        context_memory: Arc::new(Mutex::new(S2sContextMemory::default())),
        adaptive_vad: Arc::new(Mutex::new(AdaptiveS2sVadState::default())),
    };
    let started = Instant::now();
    session::run_single_segment_session(0, 0, Segment::new(1, speech, 100, 0.2), &resources)?;
    let (mut source, mut translated, mut audio_bytes) = (String::new(), String::new(), 0usize);
    for event in event_rx.try_iter() {
        match event {
            S2sEvent::InputText { text, .. } => source = text,
            S2sEvent::OutputText { text, .. } => merge_segment_text(&mut translated, &text),
            S2sEvent::Audio { bytes, .. } => audio_bytes += bytes.len(),
            S2sEvent::Error { message, .. } => anyhow::bail!(message),
            _ => {}
        }
    }
    results.insert("s2s".to_string(), serde_json::json!({"elapsed_ms": started.elapsed().as_millis(), "source": source, "translated": translated, "audio_bytes": audio_bytes}));
    std::fs::write(&output, serde_json::to_vec_pretty(&results)?)?;
    anyhow::ensure!(
        !source.is_empty() && !translated.is_empty() && audio_bytes > 0,
        "S2S requires source, translation and audio"
    );

    let profile = crate::config::TtsPlaygroundSettings {
        gemini_model: model,
        ..Default::default()
    };
    let started = Instant::now();
    let audio = crate::api::tts::worker::synthesize_gemini_live_to_wav_cancel(
        "The report is ready.",
        (&profile).into(),
        Arc::new(AtomicBool::new(false)),
    )?;
    anyhow::ensure!(!audio.pcm_samples.is_empty());
    results.insert("tts".to_string(), serde_json::json!({"elapsed_ms": started.elapsed().as_millis(), "duration_ms": audio.duration_ms, "wav_bytes": audio.wav_data.len()}));
    std::fs::write(output, serde_json::to_vec_pretty(&results)?)?;
    Ok(())
}
