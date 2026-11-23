//! Shared request plumbing for TTS workers: PCM streaming onto the player
//! channel and fatal-request reporting.

use std::sync::Arc;

use super::super::manager::TtsManager;
use super::super::types::AudioEvent;

/// Chunk a mono PCM16 LE buffer at `source_rate` Hz onto the AudioEvent
/// channel as 24 kHz frames. Resampling is delegated to [`resample_audio`]
/// in the parent module. Bails early if the request is interrupted.
pub(super) fn stream_pcm_samples(
    manager: &Arc<TtsManager>,
    request: &super::super::types::QueuedRequest,
    tx: &std::sync::mpsc::Sender<AudioEvent>,
    samples: Vec<i16>,
    source_rate: u32,
) {
    send_pcm_samples(manager, request, tx, samples, source_rate);
    let _ = tx.send(AudioEvent::End);
}

/// Sends one PCM buffer as 24 kHz frames without ending the stream, so a
/// provider that synthesizes several pieces can append them in order.
/// Returns false once the request is cancelled or the receiver is gone.
pub(super) fn send_pcm_samples(
    manager: &Arc<TtsManager>,
    request: &super::super::types::QueuedRequest,
    tx: &std::sync::mpsc::Sender<AudioEvent>,
    samples: Vec<i16>,
    source_rate: u32,
) -> bool {
    if samples.is_empty() {
        return true;
    }

    let resampled = if source_rate != 24000 {
        super::resample_audio(&samples, source_rate, 24000)
    } else {
        samples
    };

    let mut bytes = Vec::with_capacity(resampled.len() * 2);
    for sample in resampled {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    let chunk_size = 24000usize;
    for chunk in bytes.chunks(chunk_size) {
        if request.is_cancelled(manager) || tx.send(AudioEvent::Data(chunk.to_vec())).is_err() {
            return false;
        }
    }
    true
}

/// Short-circuit a request that hit a fatal error (model missing, runtime
/// unavailable, etc.). Emits the reason before `AudioEvent::End` so artifact
/// callers can show the actual failure instead of treating it as empty audio.
pub(super) fn fail_request(
    provider: &str,
    _hwnd: isize,
    tx: &std::sync::mpsc::Sender<AudioEvent>,
    reason: impl AsRef<str>,
) {
    let reason = reason.as_ref().to_string();
    eprintln!("[TTS {provider}] {reason}");
    let _ = tx.send(AudioEvent::Error(reason));
    let _ = tx.send(AudioEvent::End);
}
