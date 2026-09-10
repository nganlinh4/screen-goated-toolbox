//! Opt-in synthetic-speech acceptance through the real provider and insertion worker.

use super::*;
use crate::api::gemini_live::ready_session::{ConnectedLiveSocket, LivePoll, OpenOptions};
use crate::api::gemini_transcribe::{TranscriptState, build_live_setup, is_live_transcribe};
use crate::overlay::utils::StreamingAutoPaste;
use sha2::{Digest, Sha256};
use std::sync::{Arc, atomic::AtomicBool};

#[test]
#[ignore = "requires provider credentials, generated public speech, and a disposable owned editor"]
fn owned_edit_provider_acceptance_types_before_first_final() -> anyhow::Result<()> {
    let _exclusive = ACCEPTANCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = std::env::var("SGT_PUBLIC_SPEECH_WAV")?;
    let model = std::env::var("SGT_TRANSCRIPTION_TEST_MODEL")?;
    anyhow::ensure!(
        is_live_transcribe(&model),
        "a revisable live transcription endpoint is required"
    );
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    anyhow::ensure!(spec.channels == 1 && spec.sample_rate == 16_000 && spec.bits_per_sample == 16);
    let speech = reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?;
    anyhow::ensure!(speech.len() >= 16_000, "generated speech is too short");
    let key = std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::APP.lock().unwrap().config.gemini_api_key.clone());
    anyhow::ensure!(!key.is_empty(), "provider credentials required");
    let mut session = ConnectedLiveSocket::connect(&key)
        .map_err(|_| anyhow::anyhow!("provider connection failed"))?
        .activate_with(
            build_live_setup(&model, &[], None),
            OpenOptions {
                active_read_timeout: Duration::from_millis(10),
                ..Default::default()
            },
            || false,
        )?;
    let window = OwnedWindow::new();
    window.prepare("prefix suffix", 7);
    let writer = StreamingAutoPaste::new(true, Arc::new(AtomicBool::new(false)));
    let mut transcript = TranscriptState::default();
    let started = Instant::now();
    let audio_duration = Duration::from_secs_f64(speech.len() as f64 / 16_000.0);
    let mut offset = 0;
    let mut ended = false;
    let mut first_visible = None;
    let mut first_final = None;
    let mut interim_count = 0;
    let mut final_count = 0;
    while started.elapsed() < audio_duration + Duration::from_secs(8) {
        if offset < speech.len() && started.elapsed().as_millis() >= (offset / 16) as u128 {
            let end = (offset + 1_600).min(speech.len());
            session.send_audio_pcm(&speech[offset..end], 16_000)?;
            offset = end;
        } else if !ended && offset == speech.len() && started.elapsed() >= audio_duration {
            session.end_audio_stream()?;
            writer.begin_drain();
            ended = true;
        }
        match session.poll()? {
            LivePoll::Frame(frame) => {
                for (kind, text) in [
                    ("interim", frame.interim_input_transcript.as_deref()),
                    ("final", frame.input_transcript.as_deref()),
                ] {
                    if let Some(text) = text {
                        eprintln!(
                            "[StreamingProviderFrame] kind={kind} ms={} bytes={} sha256={:x}",
                            started.elapsed().as_millis(),
                            text.len(),
                            Sha256::digest(text.as_bytes())
                        );
                    }
                }
                let delta = transcript.apply_update(
                    frame.interim_input_transcript.as_deref(),
                    frame.input_transcript.as_deref(),
                );
                if let Some(delta) = delta {
                    first_final.get_or_insert(started.elapsed());
                    final_count += 1;
                    writer.final_text(&delta);
                } else if frame.interim_input_transcript.is_some() {
                    interim_count += 1;
                    writer.interim(&transcript.display()[transcript.committed().len()..]);
                }
            }
            LivePoll::ServerError(error) => anyhow::bail!("provider rejected acceptance: {error}"),
            LivePoll::PeerClosed(_) => anyhow::bail!("provider closed before acceptance finished"),
            _ => {}
        }
        if first_visible.is_none() && window.read() != "prefix suffix" {
            first_visible = Some(started.elapsed());
        }
    }
    session.close()?;
    writer.finish();
    let expected = format!(
        "prefix {}suffix",
        crate::overlay::utils::streaming_paste::policy::sanitize(transcript.committed())
    );
    window.wait_text(&expected);
    anyhow::ensure!(interim_count > 0 && final_count > 0);
    anyhow::ensure!(
        first_visible
            .zip(first_final)
            .is_some_and(|(visible, final_at)| visible < final_at),
        "target must receive provisional text before the first authoritative final"
    );
    eprintln!(
        "[StreamingProviderAcceptance] first_visible_ms={} first_final_ms={} interim_updates={interim_count} final_updates={final_count} final_target_matches=true",
        first_visible.unwrap().as_millis(),
        first_final.unwrap().as_millis()
    );
    Ok(())
}
