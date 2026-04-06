pub mod ffi;

use super::capture::{start_device_loopback_capture, start_mic_capture, start_per_app_capture};
use super::state::SharedRealtimeState;
use super::utils::update_overlay_text;
use super::{REALTIME_RMS, WM_VOLUME_UPDATE};
use crate::config::Preset;
use anyhow::{Result, anyhow};
use std::ffi::CString;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW};

const TRANSCRIBE_INTERVAL_MS: u64 = 200;
const SILENCE_COMMIT_MS: u64 = 1_200;
const VOICE_ACTIVITY_RMS: f32 = 0.015;

#[derive(Clone, Copy, Debug)]
pub enum MoonshineModelVariant {
    TinyStreaming,
    SmallStreaming,
    MediumStreaming,
}

impl MoonshineModelVariant {
    pub fn arch(&self) -> u32 {
        match self {
            Self::TinyStreaming => ffi::MOONSHINE_MODEL_ARCH_TINY_STREAMING,
            Self::SmallStreaming => ffi::MOONSHINE_MODEL_ARCH_SMALL_STREAMING,
            Self::MediumStreaming => ffi::MOONSHINE_MODEL_ARCH_MEDIUM_STREAMING,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::TinyStreaming => "Moonshine Tiny",
            Self::SmallStreaming => "Moonshine Small",
            Self::MediumStreaming => "Moonshine Medium",
        }
    }

    pub fn model_dir_name(&self) -> &'static str {
        match self {
            Self::TinyStreaming => "moonshine_tiny_streaming_en",
            Self::SmallStreaming => "moonshine_small_streaming_en",
            Self::MediumStreaming => "moonshine_medium_streaming_en",
        }
    }

    pub fn download_url(&self) -> &'static str {
        match self {
            Self::TinyStreaming => "https://download.moonshine.ai/model/tiny-streaming-en/quantized",
            Self::SmallStreaming => "https://download.moonshine.ai/model/small-streaming-en/quantized",
            Self::MediumStreaming => "https://download.moonshine.ai/model/medium-streaming-en/quantized",
        }
    }

    pub fn model_files(&self) -> &'static [&'static str] {
        &[
            "adapter.ort",
            "cross_kv.ort",
            "decoder_kv.ort",
            "encoder.ort",
            "frontend.ort",
            "streaming_config.json",
            "tokenizer.bin",
        ]
    }
}

fn get_model_dir(variant: MoonshineModelVariant) -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("models")
        .join(variant.model_dir_name())
}

pub fn is_model_downloaded(variant: MoonshineModelVariant) -> bool {
    let dir = get_model_dir(variant);
    variant.model_files().iter().all(|f| dir.join(f).exists())
}

pub fn download_model(
    variant: MoonshineModelVariant,
    stop_signal: Arc<AtomicBool>,
) -> Result<()> {
    let dir = get_model_dir(variant);
    std::fs::create_dir_all(&dir)?;

    let base_url = variant.download_url();
    for filename in variant.model_files() {
        if stop_signal.load(Ordering::Relaxed) {
            return Err(anyhow!("Download cancelled"));
        }
        let target = dir.join(filename);
        if target.exists() {
            continue;
        }
        let url = format!("{base_url}/{filename}");
        crate::log_info!("[Moonshine] Downloading {filename}...");
        let response = ureq::get(&url)
            .header("User-Agent", "ScreenGoatedToolbox")
            .call()
            .map_err(|e| anyhow!("Failed to download {filename}: {e}"))?;
        let mut reader = response.into_body().into_reader();
        let mut out = std::fs::File::create(&target)?;
        std::io::copy(&mut reader, &mut out)?;
    }
    Ok(())
}

pub fn run_moonshine_transcription(
    _preset: Preset,
    stop_signal: Arc<AtomicBool>,
    overlay_hwnd: HWND,
    state: SharedRealtimeState,
    variant: MoonshineModelVariant,
) -> Result<()> {
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(super::state::TranscriptionMethod::MoonshineLocal);
    }

    if !is_model_downloaded(variant) {
        update_overlay_text(overlay_hwnd, &format!("Downloading {}...", variant.label()));
        download_model(variant, stop_signal.clone())?;
    }

    if stop_signal.load(Ordering::Relaxed) {
        return Ok(());
    }

    update_overlay_text(overlay_hwnd, &format!("Loading {}...", variant.label()));
    let model_dir = get_model_dir(variant);
    let path_cstr = CString::new(model_dir.to_string_lossy().as_bytes())?;

    let lib = ffi::load()?;

    let transcriber = unsafe {
        (lib.load_transcriber)(
            path_cstr.as_ptr(),
            variant.arch(),
            std::ptr::null(),
            0,
            ffi::MOONSHINE_HEADER_VERSION,
        )
    };
    if transcriber < 0 {
        return Err(anyhow!("Failed to load Moonshine model: error {transcriber}"));
    }

    let stream = unsafe { (lib.create_stream)(transcriber, 0) };
    if stream < 0 {
        unsafe { (lib.free_transcriber)(transcriber) };
        return Err(anyhow!("Failed to create Moonshine stream: error {stream}"));
    }

    let err = unsafe { (lib.start_stream)(transcriber, stream) };
    if err != 0 {
        unsafe {
            (lib.free_stream)(transcriber, stream);
            (lib.free_transcriber)(transcriber);
        }
        return Err(anyhow!("Failed to start Moonshine stream: error {err}"));
    }

    update_overlay_text(overlay_hwnd, "");
    crate::log_info!("[Moonshine] {} loaded, streaming started", variant.label());

    let audio_buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let pause_signal = Arc::new(AtomicBool::new(false));
    let _audio_stream = start_audio_capture(audio_buffer.clone(), stop_signal.clone(), pause_signal)?;

    let mut committed_history = String::new();
    let mut last_transcribe_at = Instant::now() - Duration::from_millis(TRANSCRIBE_INTERVAL_MS);
    let mut last_voice_activity = Instant::now();

    while !stop_signal.load(Ordering::Relaxed) {
        if !overlay_hwnd.is_invalid() && !unsafe { IsWindow(Some(overlay_hwnd)).as_bool() } {
            break;
        }
        if super::DEVICE_RECONNECT_REQUESTED.load(Ordering::SeqCst) {
            break;
        }
        if crate::overlay::realtime_webview::AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
            || crate::overlay::realtime_webview::TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
        {
            break;
        }

        // Drain audio samples
        let new_samples: Vec<i16> = {
            let mut buf = audio_buffer.lock().unwrap();
            if buf.is_empty() {
                Vec::new()
            } else {
                buf.drain(..).collect()
            }
        };

        if !new_samples.is_empty() {
            let rms = compute_rms(&new_samples);
            REALTIME_RMS.store(rms.to_bits(), Ordering::Relaxed);
            crate::overlay::recording::update_audio_viz(rms);
            if rms > 0.001 {
                crate::overlay::recording::AUDIO_WARMUP_COMPLETE.store(true, Ordering::SeqCst);
            }
            if rms > VOICE_ACTIVITY_RMS {
                last_voice_activity = Instant::now();
            }
            if !overlay_hwnd.is_invalid() {
                unsafe {
                    let _ = PostMessageW(Some(overlay_hwnd), WM_VOLUME_UPDATE, WPARAM(0), LPARAM(0));
                }
            }

            // Feed audio to Moonshine
            let float_samples: Vec<f32> = new_samples.iter().map(|&s| s as f32 / 32768.0).collect();
            unsafe {
                (lib.add_audio)(
                    transcriber,
                    stream,
                    float_samples.as_ptr(),
                    float_samples.len() as u64,
                    16000,
                    0,
                );
            }
        }

        // Transcribe at intervals
        if last_transcribe_at.elapsed() >= Duration::from_millis(TRANSCRIBE_INTERVAL_MS) {
            last_transcribe_at = Instant::now();

            let mut transcript_ptr: *mut ffi::transcript_t = std::ptr::null_mut();
            let err = unsafe {
                (lib.transcribe_stream)(transcriber, stream, 0, &mut transcript_ptr)
            };

            if err == ffi::MOONSHINE_ERROR_NONE && !transcript_ptr.is_null() {
                let transcript = unsafe { &*transcript_ptr };
                let mut draft = String::new();

                for i in 0..transcript.line_count {
                    let line = unsafe { &*transcript.lines.add(i as usize) };
                    let text = if line.text.is_null() {
                        String::new()
                    } else {
                        unsafe { std::ffi::CStr::from_ptr(line.text) }
                            .to_string_lossy()
                            .to_string()
                    };

                    if text.is_empty() {
                        continue;
                    }

                    if line.is_complete != 0 {
                        // Committed line
                        if committed_history.is_empty() {
                            committed_history = text;
                        } else {
                            committed_history = format!("{} {}", committed_history, text);
                        }
                    } else {
                        // Draft line (last incomplete line)
                        draft = text;
                    }
                }

                publish_transcript(&state, overlay_hwnd, &committed_history, &draft);
            }
        }

        // Silence commit: if no voice for SILENCE_COMMIT_MS, stop+restart stream
        if last_voice_activity.elapsed() >= Duration::from_millis(SILENCE_COMMIT_MS)
            && !committed_history.is_empty()
        {
            // The committed history is already up to date from the loop above
        }

        std::thread::sleep(Duration::from_millis(50));
    }

    unsafe {
        (lib.stop_stream)(transcriber, stream);
        (lib.free_stream)(transcriber, stream);
        (lib.free_transcriber)(transcriber);
    }
    crate::log_info!("[Moonshine] Session ended");

    Ok(())
}

fn start_audio_capture(
    audio_buffer: Arc<Mutex<Vec<i16>>>,
    stop_signal: Arc<AtomicBool>,
    pause_signal: Arc<AtomicBool>,
) -> Result<Option<cpal::Stream>> {
    let (audio_source, check_per_app) = {
        let app = crate::APP.lock().unwrap();
        (app.config.realtime_audio_source.clone(), true)
    };

    use crate::overlay::realtime_webview::{REALTIME_TTS_ENABLED, SELECTED_APP_PID};
    let tts_enabled = REALTIME_TTS_ENABLED.load(Ordering::SeqCst);
    let selected_pid = SELECTED_APP_PID.load(Ordering::SeqCst);
    let using_per_app =
        check_per_app && audio_source == "device" && tts_enabled && selected_pid > 0;

    if using_per_app {
        start_per_app_capture(selected_pid, audio_buffer, stop_signal, pause_signal)?;
        Ok(None)
    } else if audio_source == "mic" {
        Ok(Some(start_mic_capture(audio_buffer, stop_signal, pause_signal)?))
    } else if audio_source == "device" && tts_enabled && selected_pid == 0 {
        Ok(None)
    } else {
        Ok(Some(start_device_loopback_capture(audio_buffer, stop_signal, pause_signal)?))
    }
}

fn compute_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples
        .iter()
        .map(|&s| (s as f64 / 32768.0).powi(2))
        .sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

fn publish_transcript(
    state: &SharedRealtimeState,
    overlay_hwnd: HWND,
    committed: &str,
    draft: &str,
) {
    if let Ok(mut s) = state.lock() {
        s.set_transcription_method(super::state::TranscriptionMethod::MoonshineLocal);
        s.set_transcript_segments(committed, draft);
        let display = s.display_transcript.clone();
        update_overlay_text(overlay_hwnd, &display);
    }
}
