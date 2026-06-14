// Google Translate TTS handler.

use std::io::Read;
use std::sync::{Arc, atomic::Ordering};

use super::super::manager::TtsManager;
use super::super::types::AudioEvent;
use super::super::utils::clear_tts_state;
use super::open_weights::stream_pcm_samples;
use crate::api::client::UREQ_AGENT;
use isolang::Language;

/// Google Translate TTS integrated with the existing audio pipeline.
/// Downloads MP3, decodes to PCM, sends via AudioEvent channel for WSOLA speed control.
pub(super) fn handle_google_tts(
    manager: Arc<TtsManager>,
    request: super::super::types::QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    let text = request.req.text.clone();
    eprintln!("[TTS Google] Starting Google TTS for {} chars", text.len());
    let speed = request
        .req
        .profile
        .as_ref()
        .map(|profile| profile.google_speed.as_str())
        .unwrap_or("Normal");

    // Detect language for Google TTS TL parameter, unless a batched caller
    // already supplied a stable language hint.
    let detected_code = request
        .req
        .profile
        .as_ref()
        .and_then(|profile| profile.language_code_override.clone())
        .unwrap_or_else(|| {
            crate::lang_detect::detect_language(&text).unwrap_or_else(|| "eng".to_string())
        });
    let tl = Language::from_639_3(&detected_code)
        .and_then(|l| l.to_639_1())
        .unwrap_or("en");

    eprintln!("[TTS Google] Detected language: {}", tl);

    let tts_speed = if speed == "Slow" { "0.75" } else { "1" };
    let url = format!(
        "https://translate.google.com/translate_tts?ie=UTF-8&q={}&tl={}&ttsspeed={}&client=tw-ob",
        urlencoding::encode(&text),
        tl,
        tts_speed
    );

    eprintln!("[TTS Google] Fetching audio from Google...");

    let resp = match UREQ_AGENT.get(&url).call() {
        Ok(r) => {
            eprintln!(
                "[TTS Google] HTTP response received (status: {})",
                r.status()
            );
            r
        }
        Err(e) => {
            eprintln!("[TTS Google] ERROR: HTTP request failed: {:?}", e);
            let _ = tx.send(AudioEvent::End);
            clear_tts_state(request.req.hwnd);
            return;
        }
    };

    let mut mp3_data = Vec::new();
    if resp
        .into_body()
        .into_reader()
        .read_to_end(&mut mp3_data)
        .is_err()
    {
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(request.req.hwnd);
        return;
    }

    // Decode MP3 to PCM (in-memory via symphonia)
    let mut source_sample_rate = 24000u32;
    let mut all_samples: Vec<i16> = Vec::new();

    if !super::audio_utils::decode_mp3_to_pcm(
        mp3_data,
        &mut all_samples,
        &mut source_sample_rate,
        || request.generation < manager.interrupt_generation.load(Ordering::SeqCst),
    ) {
        // Interrupted mid-decode.
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(request.req.hwnd);
        return;
    }

    stream_pcm_samples(&manager, &request, &tx, all_samples, source_sample_rate);
}
