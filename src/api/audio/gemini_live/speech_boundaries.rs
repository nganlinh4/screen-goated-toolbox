use std::time::Instant;

use crate::api::gemini_transcribe::{HybridVad, compute_i16_rms};

/// Flushes a completed speech turn without stopping microphone capture or the
/// session. The same VAD owner is used by continuous transcription overlays.
pub(super) fn flush_completed_speech(
    vad: &mut HybridVad,
    sent_audio: Option<&[i16]>,
    now: Instant,
    mut end_audio_stream: impl FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let should_flush = match sent_audio {
        Some(samples) => vad.observe(compute_i16_rms(samples), now),
        None => vad.poll_end(now),
    };
    if should_flush {
        end_audio_stream().inspect_err(|error| {
            crate::log_info!("[GeminiLiveStream] speech_boundary_failed error={error:#}");
        })?;
        crate::log_trace!(
            "[GeminiLiveStream] speech_boundary audio_stream_end=true recording_active=true"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn speech_pause_flushes_once_and_next_turn_rearms_without_closing_capture() {
        let start = Instant::now();
        let mut vad = HybridVad::default();
        let voice = [4_000i16; 1_600];
        let silence = [0i16; 1_600];
        let end_count = std::cell::Cell::new(0);
        let observe = |vad: &mut HybridVad, audio, elapsed| {
            flush_completed_speech(vad, audio, start + Duration::from_millis(elapsed), || {
                end_count.set(end_count.get() + 1);
                Ok(())
            })
            .unwrap();
        };
        observe(&mut vad, Some(silence.as_slice()), 0);
        observe(&mut vad, Some(voice.as_slice()), 100);
        observe(&mut vad, Some(silence.as_slice()), 279);
        observe(&mut vad, None, 519);
        assert_eq!(
            end_count.get(),
            0,
            "do not flush before the silence boundary"
        );
        observe(&mut vad, None, 520);
        assert_eq!(end_count.get(), 1);
        observe(&mut vad, None, 900);
        assert_eq!(
            end_count.get(),
            1,
            "continued silence must not resend the boundary"
        );
        observe(&mut vad, Some(voice.as_slice()), 1_000);
        observe(&mut vad, Some(silence.as_slice()), 1_420);
        observe(&mut vad, None, 1_800);
        assert_eq!(
            end_count.get(),
            2,
            "each speech turn flushes once while the session continues"
        );
    }

    #[test]
    fn reconnect_discards_old_pending_boundary() {
        let start = Instant::now();
        let mut vad = HybridVad::default();
        vad.observe(0.1, start);
        vad.reset_connection();
        flush_completed_speech(&mut vad, None, start + Duration::from_secs(2), || {
            anyhow::bail!("a fresh session must not flush an old turn")
        })
        .unwrap();
    }

    #[test]
    fn flush_failure_is_not_reported_as_success() {
        let start = Instant::now();
        let mut vad = HybridVad::default();
        vad.observe(0.1, start);
        assert!(
            flush_completed_speech(&mut vad, None, start + Duration::from_secs(1), || {
                anyhow::bail!("socket closed")
            })
            .is_err()
        );
    }
}
