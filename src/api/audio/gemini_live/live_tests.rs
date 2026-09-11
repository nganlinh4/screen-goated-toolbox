//! Explicit provider acceptance using supplied public speech, never a microphone.

use super::*;
use std::time::Instant;

#[test]
#[ignore = "requires credentials and explicitly supplied public speech"]
fn transcription_commit_latency_probe() -> anyhow::Result<()> {
    use crate::api::gemini_live::ready_session::LivePoll;
    let path = std::env::var("SGT_PUBLIC_SPEECH_WAV")?;
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    anyhow::ensure!(spec.channels == 1 && spec.sample_rate == 16_000 && spec.bits_per_sample == 16);
    let mut speech = reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?;
    if let Ok(value) = std::env::var("SGT_TEST_SPEECH_PEAK") {
        let target: f64 = value.parse()?;
        anyhow::ensure!(target > 0.0 && target <= 1.0, "invalid test speech peak");
        let peak = speech.iter().map(|s| (*s as f64).abs()).fold(0.0, f64::max);
        anyhow::ensure!(peak > 0.0, "synthetic speech fixture is silent");
        for sample in &mut speech {
            *sample = (*sample as f64 * target * 32767.0 / peak).round() as i16;
        }
        eprintln!("[SpeechAcceptance] normalized_fixture_peak={target}");
    }
    let model = std::env::var("SGT_TRANSCRIPTION_TEST_MODEL")?;
    let mode = std::env::var("SGT_PROBE_MODE").unwrap_or_else(|_| "SMART".into());
    let boundary_ms: usize = std::env::var("SGT_PROBE_BOUNDARY_MS")
        .unwrap_or_else(|_| "0".into())
        .parse()?;
    let key = std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| APP.lock().unwrap().config.gemini_api_key.clone());
    anyhow::ensure!(!key.is_empty(), "credentials required");
    let mut setup = crate::api::gemini_transcribe::build_live_setup(&model, &[], None);
    if crate::api::gemini_transcribe::is_live_transcribe(&model) {
        setup["setup"]["inputAudioTranscription"]["mode"] = mode.clone().into();
    }
    let mut session = ConnectedLiveSocket::connect(&key)
        .map_err(|_| anyhow::anyhow!("provider connection failed"))?
        .activate_with(
            setup,
            OpenOptions {
                active_read_timeout: Duration::from_millis(10),
                ..Default::default()
            },
            || false,
        )?;
    let started = Instant::now();
    let mut offset = 0;
    let mut ended = false;
    let mut final_count = 0;
    let audio_ms = speech.len() / 16;
    eprintln!("PROBE model={model} mode={mode} boundary_ms={boundary_ms} audio_ms={audio_ms}");
    while started.elapsed() < Duration::from_millis(audio_ms as u64 + 8_000) {
        let elapsed = started.elapsed().as_millis() as usize;
        if offset < speech.len() && elapsed >= offset / 16 {
            let end = (offset + 1600).min(speech.len());
            session.send_audio_pcm(&speech[offset..end], 16_000)?;
            offset = end;
            if boundary_ms > 0 && (offset / 16).is_multiple_of(boundary_ms) {
                session.end_audio_stream()?;
                eprintln!("BOUNDARY ms={elapsed}");
            }
        } else if offset == speech.len() && !ended {
            session.end_audio_stream()?;
            ended = true;
            eprintln!("AUDIO_END ms={elapsed}");
        }
        match session.poll()? {
            LivePoll::Frame(frame) => {
                let ms = started.elapsed().as_millis();
                eprintln!(
                    "PROBE_FRAME {}",
                    serde_json::json!({
                        "ms": ms,
                        "interim": frame.interim_input_transcript,
                        "final": frame.input_transcript,
                        "audio_active": !ended,
                    })
                );
                if let Some(text) = frame.interim_input_transcript {
                    eprintln!("INTERIM ms={ms} text={text:?}");
                }
                if let Some(text) = frame.input_transcript {
                    final_count += 1;
                    eprintln!("FINAL ms={ms} audio_active={} text={text:?}", !ended);
                }
            }
            LivePoll::ServerError(e) => anyhow::bail!("provider error: {e}"),
            LivePoll::PeerClosed(_) => anyhow::bail!("provider closed before probe completed"),
            _ => {}
        }
    }
    session.close()?;
    anyhow::ensure!(final_count > 0, "no final transcripts received");
    Ok(())
}

#[test]
#[ignore = "requires credentials and SGT_PUBLIC_SPEECH_WAV containing generated public speech"]
fn continuous_preset_finalization_provider_acceptance() -> anyhow::Result<()> {
    let path = std::env::var_os("SGT_PUBLIC_SPEECH_WAV")
        .ok_or_else(|| anyhow::anyhow!("SGT_PUBLIC_SPEECH_WAV is required"))?;
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    anyhow::ensure!(
        spec.channels == 1 && spec.sample_rate == 16_000 && spec.bits_per_sample == 16,
        "expected 16 kHz mono PCM16"
    );
    let mut speech = reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?;
    if let Ok(value) = std::env::var("SGT_TEST_SPEECH_PEAK") {
        let target: f64 = value.parse()?;
        anyhow::ensure!(target > 0.0 && target <= 1.0, "invalid test speech peak");
        let peak = speech.iter().map(|s| (*s as f64).abs()).fold(0.0, f64::max);
        anyhow::ensure!(peak > 0.0, "synthetic speech fixture is silent");
        for sample in &mut speech {
            *sample = (*sample as f64 * target * 32767.0 / peak).round() as i16;
        }
        eprintln!("[SpeechAcceptance] normalized_fixture_peak={target}");
    }
    let model = std::env::var("SGT_TRANSCRIPTION_TEST_MODEL")?;
    anyhow::ensure!(
        crate::api::gemini_transcribe::is_live_transcribe(&model),
        "dedicated live transcription endpoint required"
    );
    let api_key = APP.lock().unwrap().config.gemini_api_key.clone();
    anyhow::ensure!(!api_key.trim().is_empty(), "Gemini credentials required");
    let mut session = open_ready_session(&api_key, &model, &[], || false)?;
    let audio_buffer = Arc::new(Mutex::new(Vec::new()));
    let committed = Arc::new(Mutex::new(String::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let producer_buffer = audio_buffer.clone();
    let producer_committed = committed.clone();
    let producer_stop = stop.clone();
    let producer = std::thread::spawn(move || {
        let started = Instant::now();
        let mut first_turn_chars = 0;
        for frame_index in 0..360 {
            let offset = frame_index % 180 * 1_600;
            let frame = (0..1_600).map(|index| speech.get(offset + index).copied().unwrap_or(0));
            producer_buffer.lock().unwrap().extend(frame);
            if frame_index == 179 {
                first_turn_chars = producer_committed.lock().unwrap().chars().count();
            }
            std::thread::sleep(
                (started + Duration::from_millis((frame_index + 1) as u64 * 100))
                    .saturating_duration_since(Instant::now()),
            );
        }
        let second_turn_chars = producer_committed.lock().unwrap().chars().count();
        producer_stop.store(true, Ordering::SeqCst);
        (first_turn_chars, second_turn_chars)
    });
    let preset = Preset {
        hide_recording_ui: true,
        auto_paste: false,
        auto_stop_recording: false,
        ..Default::default()
    };
    let mut transcript = crate::api::gemini_transcribe::TranscriptState::default();
    let pause = Arc::new(AtomicBool::new(false));
    let abort = Arc::new(AtomicBool::new(false));
    let auto_paste = crate::overlay::utils::StreamingAutoPaste::new(false, abort.clone());
    stream_loop::run_streaming_loop(stream_loop::StreamingLoopContext {
        preset: &preset,
        session: &mut session,
        api_key: &api_key,
        model: &model,
        vocabulary: &[],
        audio_buffer: &audio_buffer,
        accumulated_text: &committed,
        transcribe_text: &mut transcript,
        auto_paste: &auto_paste,
        stop_signal: &stop,
        pause_signal: &pause,
        abort_signal: &abort,
        overlay_hwnd: HWND::default(),
        update_stream_text: &|_: &str| {},
    });
    auto_paste.begin_drain();
    let _ = session.close();
    auto_paste.finish();
    let (first, second) = producer.join().unwrap();
    eprintln!(
        "[PresetFinalizationAcceptance] first_turn_final_chars={first} second_turn_final_chars={second}"
    );
    anyhow::ensure!(
        first > 0,
        "first authoritative final must arrive while capture is still open"
    );
    anyhow::ensure!(
        second > first,
        "next speech turn must finalize on the same recording session"
    );
    Ok(())
}
