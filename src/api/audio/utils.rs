//! Audio utility functions for WAV encoding, PCM extraction, and resampling.

use std::io::Cursor;
use std::sync::mpsc;

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config::Preset;
use crate::overlay::result::{
    create_result_window, get_chain_color, RefineContext, WindowType,
};
use crate::win_types::SendHwnd;

/// Encode PCM samples to WAV format
pub fn encode_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav_cursor = Cursor::new(Vec::new());
    {
        let mut writer =
            hound::WavWriter::new(&mut wav_cursor, spec).expect("Failed to create memory writer");
        for sample in samples {
            writer
                .write_sample(*sample)
                .expect("Failed to write sample");
        }
        writer.finalize().expect("Failed to finalize WAV");
    }
    wav_cursor.into_inner()
}

/// Extract PCM i16 samples from WAV data
pub fn extract_pcm_from_wav(wav_data: &[u8]) -> anyhow::Result<Vec<i16>> {
    let cursor = Cursor::new(wav_data);
    let reader = hound::WavReader::new(cursor)?;
    let spec = reader.spec();

    // Get samples based on format
    let samples: Vec<i16> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .into_samples::<i16>()
            .filter_map(|s| s.ok())
            .collect(),
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(|s| s.ok())
            .map(|f| (f * i16::MAX as f32) as i16)
            .collect(),
    };

    // Convert to mono 16kHz if needed
    let mono_samples: Vec<i16> = if spec.channels > 1 {
        samples
            .chunks(spec.channels as usize)
            .map(|chunk| {
                let sum: i32 = chunk.iter().map(|&s| s as i32).sum();
                (sum / chunk.len() as i32) as i16
            })
            .collect()
    } else {
        samples
    };

    // Resample to 16kHz if needed
    let target_rate = 16000;
    if spec.sample_rate != target_rate {
        let ratio = target_rate as f64 / spec.sample_rate as f64;
        let new_len = (mono_samples.len() as f64 * ratio) as usize;
        let mut resampled = Vec::with_capacity(new_len);

        for i in 0..new_len {
            let src_idx = (i as f64 / ratio) as usize;
            if src_idx < mono_samples.len() {
                resampled.push(mono_samples[src_idx]);
            }
        }
        Ok(resampled)
    } else {
        Ok(mono_samples)
    }
}

/// Simple nearest-neighbor resampling to 16kHz
pub fn resample_to_16khz(samples: &[i16], source_rate: u32) -> Vec<i16> {
    if source_rate == 16000 {
        return samples.to_vec();
    }
    let ratio = 16000.0 / source_rate as f64;
    let new_len = (samples.len() as f64 * ratio) as usize;
    let mut resampled = Vec::with_capacity(new_len);
    for i in 0..new_len {
        let src_idx = (i as f64 / ratio) as usize;
        if src_idx < samples.len() {
            resampled.push(samples[src_idx]);
        }
    }
    resampled
}

/// Create a streaming overlay window for real-time transcription display.
/// Returns the HWND of the created window, or None if streaming is disabled.
pub fn create_streaming_overlay(preset: &Preset) -> Option<HWND> {
    // Find the relevant audio block for streaming settings
    let audio_block = preset
        .blocks
        .iter()
        .find(|b| b.block_type == "audio")
        .or_else(|| preset.blocks.iter().find(|b| b.block_type != "input_adapter"));

    let streaming_enabled = audio_block
        .map(|b| {
            b.show_overlay
                && (b.streaming_enabled || b.render_mode == "stream")
                && b.render_mode != "plain"
        })
        .unwrap_or(false);

    if !streaming_enabled {
        return None;
    }

    let (tx, rx) = mpsc::channel();
    let preset_for_thread = preset.clone();

    std::thread::spawn(move || {
        let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let (rect, _) = if preset_for_thread.blocks.len() > 1 {
            let w = 600;
            let h = 300;
            let gap = 20;
            let total = w * 2 + gap;
            let x = (screen_w - total) / 2;
            let y = (screen_h - h) / 2;
            (
                RECT {
                    left: x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                },
                Some(RECT {
                    left: x + w + gap,
                    top: y,
                    right: x + w + gap + w,
                    bottom: y + h,
                }),
            )
        } else {
            let w = 700;
            let h = 300;
            let x = (screen_w - w) / 2;
            let y = (screen_h - h) / 2;
            (
                RECT {
                    left: x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                },
                None,
            )
        };

        let active_block = preset_for_thread
            .blocks
            .iter()
            .find(|b| b.block_type == "audio")
            .or_else(|| preset_for_thread.blocks.iter().find(|b| b.block_type != "input_adapter"));

        let model_id = active_block.map(|b| b.model.clone()).unwrap_or_default();
        let render_mode = active_block
            .map(|b| b.render_mode.clone())
            .unwrap_or_default();

        // Get provider
        let model_conf = crate::model_config::get_model_by_id(&model_id);
        let provider = model_conf
            .map(|m| m.provider)
            .unwrap_or("gemini".to_string());

        let hwnd = create_result_window(
            rect,
            WindowType::Primary,
            RefineContext::Audio(Vec::new()),
            model_id,
            provider,
            true,          // streaming_enabled
            false,         // start_editing
            String::new(), // preset_prompt
            get_chain_color(0),
            &render_mode,
            "Listening...".to_string(),
            Some(preset_for_thread.id.clone()),
            true,
        );

        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }

        let _ = tx.send(SendHwnd(hwnd));

        // Message Loop
        unsafe {
            let mut m = MSG::default();
            while GetMessageW(&mut m, None, 0, 0).into() {
                let _ = TranslateMessage(&m);
                DispatchMessageW(&m);
                if !IsWindow(Some(hwnd)).as_bool() {
                    break;
                }
            }
        }
    });

    rx.recv().ok().map(|SendHwnd(h)| h)
}

/// RAII guard that closes a window when dropped
pub struct WindowGuard(pub HWND);

impl Drop for WindowGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(self.0), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Calculate window rectangles for result display
pub fn calculate_result_rects(preset: &Preset) -> (RECT, Option<RECT>) {
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };

    if preset.blocks.len() > 1 {
        let w = 600;
        let h = 300;
        let gap = 20;
        let total = w * 2 + gap;
        let x = (screen_w - total) / 2;
        let y = (screen_h - h) / 2;
        (
            RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            Some(RECT {
                left: x + w + gap,
                top: y,
                right: x + w + gap + w,
                bottom: y + h,
            }),
        )
    } else {
        let w = 700;
        let h = 300;
        let x = (screen_w - w) / 2;
        let y = (screen_h - h) / 2;
        (
            RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            None,
        )
    }
}
