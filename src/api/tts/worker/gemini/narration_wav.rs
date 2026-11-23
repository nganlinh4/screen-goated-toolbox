//! Whole-clip Gemini Live synthesis for recorder subtitle narration. Only the
//! recorder worker narrates, so the host never compiles this.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::super::super::types::{SOURCE_SAMPLE_RATE, TtsCollectedAudio, TtsRequestProfile};
use super::super::super::websocket::{build_tts_setup, build_tts_text};
use super::{
    classify_setup_error, playground_language_instruction, premature_close_message,
    tts_open_options,
};
use crate::APP;
use crate::api::gemini_live::ready_session::{ConnectedLiveSocket, LivePoll};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(90);

pub fn synthesize_gemini_live_to_wav_cancel(
    text: &str,
    profile: TtsRequestProfile,
    cancel: Arc<AtomicBool>,
) -> anyhow::Result<TtsCollectedAudio> {
    let api_key = {
        let app = APP
            .lock()
            .map_err(|_| anyhow::anyhow!("APP config lock poisoned"))?;
        app.config.gemini_api_key.trim().to_string()
    };
    if api_key.is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY:google"));
    }

    let model = crate::model_config::normalize_tts_gemini_model(&profile.gemini_model).to_string();
    let instruction = playground_language_instruction(text, &profile);
    let setup = build_tts_setup(
        &model,
        &profile.gemini_voice,
        &profile.gemini_speed,
        instruction.as_deref(),
    );
    let cancelled = || cancel.load(Ordering::SeqCst);
    let session = ConnectedLiveSocket::connect(&api_key)?.activate_with(
        setup,
        tts_open_options(Duration::from_secs(15)),
        cancelled,
    );
    anyhow::ensure!(!cancelled(), "Generation cancelled");
    let mut session = session.map_err(classify_setup_error)?;

    if let Err(error) = session.send_json(&build_tts_text(text)) {
        let _ = session.close();
        return Err(error);
    }
    let read_started = Instant::now();
    let mut audio_bytes = Vec::new();
    let outcome = loop {
        if cancelled() {
            break Err(anyhow::anyhow!("Generation cancelled"));
        }
        if read_started.elapsed() > RESPONSE_TIMEOUT {
            break Err(anyhow::anyhow!("TTS generation timed out"));
        }
        match session.poll() {
            Ok(LivePoll::Frame(frame)) => {
                let response_complete = frame.response_complete();
                for audio_data in frame.audio_chunks {
                    audio_bytes.extend_from_slice(&audio_data);
                }
                if response_complete {
                    break Ok(());
                }
            }
            // A close before turn completion would narrate a truncated clip.
            Ok(LivePoll::PeerClosed(close)) => {
                break Err(anyhow::anyhow!(premature_close_message(close.as_ref())));
            }
            Ok(LivePoll::ServerError(error)) => {
                break Err(anyhow::anyhow!("Gemini TTS error: {error}"));
            }
            Ok(LivePoll::Unparsed { .. }) => {}
            Ok(LivePoll::Idle) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => break Err(error),
        }
    };
    let _ = session.close();
    outcome?;
    collected_audio(&audio_bytes)
}

/// Little-endian 16-bit mono PCM at the Live output rate, plus its WAV form.
fn collected_audio(audio_bytes: &[u8]) -> anyhow::Result<TtsCollectedAudio> {
    anyhow::ensure!(!audio_bytes.is_empty(), "TTS generated no audio");
    let pcm_samples: Vec<i16> = audio_bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| i16::from_le_bytes(*chunk))
        .collect();
    let duration_ms = (pcm_samples.len() as u64 * 1000) / SOURCE_SAMPLE_RATE as u64;
    let wav_data = crate::api::audio::encode_wav(&pcm_samples, SOURCE_SAMPLE_RATE, 1);
    Ok(TtsCollectedAudio {
        pcm_samples,
        wav_data,
        sample_rate: SOURCE_SAMPLE_RATE,
        duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::{SOURCE_SAMPLE_RATE, collected_audio};

    #[test]
    fn pcm_bytes_become_timed_mono_samples_and_a_wav() {
        let samples: Vec<i16> = (0..SOURCE_SAMPLE_RATE as i16 / 2).collect();
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let audio = collected_audio(&bytes).unwrap();
        assert_eq!(audio.pcm_samples, samples);
        assert_eq!(audio.duration_ms, 500);
        assert_eq!(audio.sample_rate, SOURCE_SAMPLE_RATE);
        assert_eq!(&audio.wav_data[..4], b"RIFF");
    }

    #[test]
    fn an_empty_response_is_an_error() {
        let Err(error) = collected_audio(&[]) else {
            panic!("empty audio was accepted");
        };
        assert!(error.to_string().contains("no audio"));
    }
}
