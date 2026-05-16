//! Shared helpers for the offline open-weights TTS workers.
//!
//! Only the Kokoro worker is currently functional; the other four leaderboard
//! providers route through here for `fail_request` (so the request channel and
//! UI loading state stay clean when a method has no offline pipeline yet).
//!
//! Once additional offline providers land, [`stream_pcm_samples`] is the
//! canonical place to chunk raw 24 kHz PCM16 samples onto the AudioEvent
//! channel that drives the shared TTS player.

use std::sync::{Arc, atomic::Ordering};

use super::super::manager::TtsManager;
use super::super::types::AudioEvent;
use super::super::utils::{clear_tts_loading_state, clear_tts_state};

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
    let hwnd = request.req.hwnd;
    let generation = request.generation;

    if samples.is_empty() {
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(hwnd);
        return;
    }

    clear_tts_loading_state(hwnd);

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
        if generation < manager.interrupt_generation.load(Ordering::SeqCst) {
            break;
        }
        let _ = tx.send(AudioEvent::Data(chunk.to_vec()));
    }
    let _ = tx.send(AudioEvent::End);
    clear_tts_state(hwnd);
}

/// Short-circuit a request that hit a fatal error (model missing, runtime
/// unavailable, etc.). Emits the reason before `AudioEvent::End` so artifact
/// callers can show the actual failure instead of treating it as empty audio.
pub(super) fn fail_request(
    provider: &str,
    hwnd: isize,
    tx: &std::sync::mpsc::Sender<AudioEvent>,
    reason: impl AsRef<str>,
) {
    let reason = reason.as_ref().to_string();
    eprintln!("[TTS {provider}] {reason}");
    let _ = tx.send(AudioEvent::Error(reason));
    let _ = tx.send(AudioEvent::End);
    clear_tts_loading_state(hwnd);
    clear_tts_state(hwnd);
}
