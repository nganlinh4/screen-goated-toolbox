//! Opt-in provider acceptance with explicitly supplied, generated public speech.

use super::*;

#[test]
#[ignore = "requires Gemini credentials and SGT_PUBLIC_SPEECH_WAV containing generated public speech"]
fn continuous_transcription_provider_acceptance() -> Result<()> {
    let path = std::env::var_os("SGT_PUBLIC_SPEECH_WAV")
        .ok_or_else(|| anyhow::anyhow!("SGT_PUBLIC_SPEECH_WAV is required"))?;
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    anyhow::ensure!(
        spec.channels == 1 && spec.sample_rate == 16_000 && spec.bits_per_sample == 16,
        "generated speech must be 16 kHz mono PCM16"
    );
    let speech = reader
        .samples::<i16>()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    anyhow::ensure!(!speech.is_empty(), "generated speech is empty");
    let model = std::env::var("SGT_TRANSCRIPTION_TEST_MODEL")
        .unwrap_or_else(|_| crate::model_config::GEMINI_LIVE_API_MODEL_3_1.to_owned());
    let uses_interim_transcripts = crate::api::gemini_transcribe::is_live_transcribe(&model);
    let api_key = std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .unwrap_or_else(|| crate::APP.lock().unwrap().config.gemini_api_key.clone());
    anyhow::ensure!(
        !api_key.trim().is_empty(),
        "Gemini credentials are required"
    );
    let session = open_ready_session(&api_key, &model, &[], None, || false)?;
    let buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let observations = Arc::new(Mutex::new(main_loop::SessionObservations::default()));
    let producer_buffer = buffer.clone();
    let producer_stop = stop.clone();
    let producer = std::thread::spawn(move || {
        let started = Instant::now();
        for frame_index in 0..450 {
            if producer_stop.load(Ordering::Relaxed) {
                break;
            }
            // Repeat public speech in separated turns, with a nonzero quiet input
            // floor between them. No device or user audio is read by this test.
            let offset = frame_index % 180 * 1600;
            let frame = (0..1600)
                .map(|index| speech.get(offset + index).copied().unwrap_or(200))
                .collect::<Vec<_>>();
            producer_buffer.lock().unwrap().extend(frame);
            let next = started + Duration::from_millis((frame_index as u64 + 1) * 100);
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
        }
        producer_stop.store(true, Ordering::SeqCst);
    });
    let state = Arc::new(Mutex::new(super::super::state::RealtimeState::new()));
    let result = main_loop::run_main_loop(main_loop::RealtimeMainLoop {
        session,
        audio_buffer: buffer,
        stop_signal: stop.clone(),
        overlay_hwnd: HWND::default(),
        state: state.clone(),
        gemini_live_model: &model,
        gemini_api_key: &api_key,
        capture_label: "synthetic",
        reconnect_on_no_results: !uses_interim_transcripts,
        uses_interim_transcripts,
        observations: Some(observations.clone()),
    });
    stop.store(true, Ordering::SeqCst);
    producer.join().unwrap();
    let observations = observations.lock().unwrap();
    if let Some(path) = std::env::var_os("SGT_TRANSCRIPTION_TEST_OUTPUT") {
        let report = serde_json::json!({
            "model": model,
            "setup": build_realtime_transcription_setup(&model, &[], None),
            "observations": &*observations,
            "transcript": state.lock().unwrap().full_transcript,
            "error": result.as_ref().err().map(ToString::to_string),
        });
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    result?;
    eprintln!(
        "[TranscriptionAcceptance] model={} updates={} updates_after_first_flush={} first_no_result_reconnect_ms={:?} interim_updates={} final_updates={}",
        model,
        observations.transcript_updates,
        observations.transcript_updates_after_first_flush,
        observations.first_no_result_reconnect_ms,
        observations.interim_updates,
        observations.final_updates
    );
    anyhow::ensure!(
        observations.transcript_updates > 1,
        "expected multiple transcript updates"
    );
    anyhow::ensure!(
        observations.transcript_updates_after_first_flush > 0,
        "expected transcription to continue after the first input flush"
    );
    anyhow::ensure!(
        observations
            .first_no_result_reconnect_ms
            .is_none_or(|elapsed| elapsed >= 30_000),
        "recovery abandoned input before its flush and response grace"
    );
    if uses_interim_transcripts {
        anyhow::ensure!(
            observations.interim_updates > 0 && observations.final_updates > 0,
            "dedicated transcription must deliver both interim and final updates"
        );
    }
    Ok(())
}
