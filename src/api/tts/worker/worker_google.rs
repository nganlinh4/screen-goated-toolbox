// Google Translate TTS handler.

use minimp3::{Decoder, Frame};
use std::io::{Cursor, Read};
use std::sync::{Arc, atomic::Ordering};

use super::super::manager::TtsManager;
use super::super::types::AudioEvent;
use super::super::utils::{clear_tts_loading_state, clear_tts_state};
use super::resample_audio;
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

    // Detect language for Google TTS TL parameter
    let lang_code = whatlang::detect_lang(&text).unwrap_or(whatlang::Lang::Eng);
    let tl = Language::from_639_3(lang_code.code())
        .and_then(|l| l.to_639_1())
        .unwrap_or("en");

    eprintln!("[TTS Google] Detected language: {}", tl);

    let url = format!(
        "https://translate.google.com/translate_tts?ie=UTF-8&q={}&tl={}&client=tw-ob",
        urlencoding::encode(&text),
        tl
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

    // Decode MP3 to PCM
    let mut decoder = Decoder::new(Cursor::new(mp3_data));
    let mut source_sample_rate = 24000u32;
    let mut all_samples: Vec<i16> = Vec::new();

    loop {
        if request.generation < manager.interrupt_generation.load(Ordering::SeqCst) {
            let _ = tx.send(AudioEvent::End);
            clear_tts_state(request.req.hwnd);
            return;
        }

        match decoder.next_frame() {
            Ok(Frame {
                data,
                sample_rate,
                channels,
                ..
            }) => {
                source_sample_rate = sample_rate as u32;
                if channels == 2 {
                    for chunk in data.chunks(2) {
                        let sample = ((chunk[0] as i32 + chunk[1] as i32) / 2) as i16;
                        all_samples.push(sample);
                    }
                } else {
                    all_samples.extend_from_slice(&data);
                }
            }
            Err(minimp3::Error::Eof) => break,
            Err(_) => break,
        }
    }

    if all_samples.is_empty() {
        let _ = tx.send(AudioEvent::End);
        clear_tts_state(request.req.hwnd);
        return;
    }

    clear_tts_loading_state(request.req.hwnd);

    // Resample if needed to 24kHz
    let audio_bytes = if source_sample_rate != 24000 {
        let resampled = resample_audio(&all_samples, source_sample_rate, 24000);
        let mut bytes = Vec::with_capacity(resampled.len() * 2);
        for sample in resampled {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    } else {
        let mut bytes = Vec::with_capacity(all_samples.len() * 2);
        for sample in all_samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    };

    let chunk_size = 24000;
    for chunk in audio_bytes.chunks(chunk_size) {
        if request.generation < manager.interrupt_generation.load(Ordering::SeqCst) {
            break;
        }
        let _ = tx.send(AudioEvent::Data(chunk.to_vec()));
    }

    let _ = tx.send(AudioEvent::End);
    clear_tts_state(request.req.hwnd);
}
