use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

use crate::api::tts::manager::TtsManager;
use crate::api::tts::types::SOURCE_SAMPLE_RATE;
use crate::api::tts::wsola::WsolaStretcher;

struct AutoSpeedState {
    speed: u32,
}

/// Simple audio player using Windows WASAPI with loopback exclusion.
pub(crate) struct AudioPlayer {
    _sample_rate: u32,
    shared_buffer: Arc<Mutex<VecDeque<i16>>>,
    device_padding_frames: Arc<AtomicU32>,
    shutdown: Arc<AtomicBool>,
    _thread: Option<std::thread::JoinHandle<()>>,
    wsola: Mutex<WsolaStretcher>,
}

impl AudioPlayer {
    fn current_default_render_endpoint_id() -> Option<String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            device.GetId().ok()?.to_string().ok()
        }
    }

    pub(crate) fn new(sample_rate: u32, manager: Arc<TtsManager>) -> Self {
        let shared_buffer: Arc<Mutex<VecDeque<i16>>> = Arc::new(Mutex::new(VecDeque::new()));
        let buffer_clone = shared_buffer.clone();
        let device_padding_frames = Arc::new(AtomicU32::new(0));
        let device_padding_clone = device_padding_frames.clone();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        let thread = std::thread::spawn(move || {
            eprintln!("[TTS Player] WASAPI thread starting...");
            if wasapi::initialize_mta().is_err() {
                eprintln!("[TTS Player] ERROR: Failed to initialize COM for WASAPI thread");
                return;
            }

            eprintln!("[TTS Player] COM initialized, entering audio stream loop...");

            while !shutdown_clone.load(Ordering::Relaxed) {
                let target_device_id = Self::current_target_device_id();

                let stream_result = unsafe {
                    Self::run_wasapi_excluded(
                        sample_rate,
                        buffer_clone.clone(),
                        device_padding_clone.clone(),
                        shutdown_clone.clone(),
                        target_device_id,
                        manager.clone(),
                    )
                };
                device_padding_clone.store(0, Ordering::Release);
                match stream_result {
                    Ok(()) => break,
                    Err(e) => {
                        if shutdown_clone.load(Ordering::Relaxed) {
                            break;
                        }
                        eprintln!(
                            "[TTS Player] WARNING: WASAPI stream failed: {}. Reinitializing...",
                            e
                        );
                        std::thread::sleep(Duration::from_millis(750));
                    }
                }
            }
        });

        Self {
            _sample_rate: sample_rate,
            shared_buffer,
            device_padding_frames,
            shutdown,
            _thread: Some(thread),
            wsola: Mutex::new(WsolaStretcher::new(SOURCE_SAMPLE_RATE)),
        }
    }

    fn current_target_device_id() -> Option<String> {
        if let Ok(app) = crate::APP.lock() {
            let id = app.config.tts_output_device.clone();
            if id.is_empty() { None } else { Some(id) }
        } else {
            None
        }
    }

    unsafe fn run_wasapi_excluded(
        _sample_rate: u32,
        shared_buffer: Arc<Mutex<VecDeque<i16>>>,
        device_padding_frames: Arc<AtomicU32>,
        shutdown: Arc<AtomicBool>,
        target_device_id: Option<String>,
        manager: Arc<TtsManager>,
    ) -> anyhow::Result<()> {
        unsafe {
            eprintln!("[TTS WASAPI] Initializing audio output...");

            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok();

            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

            let device = if let Some(ref id_str) = target_device_id {
                eprintln!("[TTS WASAPI] Using specified device: {}", id_str);
                let id_hstring = windows::core::HSTRING::from(id_str.clone());
                match enumerator.GetDevice(&id_hstring) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("[TTS WASAPI] ERROR: Specified device not found: {:?}", e);
                        return Err(anyhow::anyhow!(
                            "Configured TTS output device is unavailable; waiting for it to return"
                        ));
                    }
                }
            } else {
                eprintln!("[TTS WASAPI] Using default audio device");
                enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?
            };

            let active_device_id = device.GetId()?.to_string().ok();
            let follows_default_device = target_device_id.is_none();

            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            let mix_format_ptr = client.GetMixFormat()?;
            let mix_format = *mix_format_ptr;

            let channels = mix_format.nChannels;
            let sample_rate = mix_format.nSamplesPerSec;
            let bits = mix_format.wBitsPerSample;
            eprintln!(
                "[TTS WASAPI] Device format: {} channels, {} Hz, {} bits",
                channels, sample_rate, bits
            );

            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                0,
                1000000,
                0,
                mix_format_ptr,
                None,
            )?;

            let buffer_size = client.GetBufferSize()?;
            let render_client: IAudioRenderClient = client.GetService()?;

            let mut client_started = false;

            eprintln!(
                "[TTS WASAPI] Audio client initialized successfully (buffer size: {})",
                buffer_size
            );

            let channels = mix_format.nChannels as usize;
            let is_float = mix_format.wFormatTag == 3
                || (mix_format.wFormatTag == 65534 && (mix_format.cbSize >= 22));

            let mut last_clear_gen = manager.buffer_clear_generation.load(Ordering::SeqCst);
            let mut last_default_device_check = Instant::now();

            while !shutdown.load(Ordering::Relaxed) {
                let current_clear_gen = manager.buffer_clear_generation.load(Ordering::SeqCst);
                if current_clear_gen > last_clear_gen {
                    if let Ok(mut deck) = shared_buffer.lock() {
                        deck.clear();
                    }
                    last_clear_gen = current_clear_gen;
                }
                if follows_default_device
                    && last_default_device_check.elapsed() >= Duration::from_millis(750)
                {
                    let current_default_id = Self::current_default_render_endpoint_id();
                    if current_default_id.is_some() && current_default_id != active_device_id {
                        return Err(anyhow::anyhow!(
                            "Windows default audio output changed; rebuilding TTS output"
                        ));
                    }
                    last_default_device_check = Instant::now();
                }
                let padding = client.GetCurrentPadding()?;
                device_padding_frames.store(padding, Ordering::Release);
                let available = buffer_size.saturating_sub(padding);

                if available > 0 {
                    let has_audio = shared_buffer
                        .lock()
                        .map(|deck| !deck.is_empty())
                        .unwrap_or(false);
                    if !has_audio {
                        if client_started && padding == 0 {
                            client.Stop()?;
                            client_started = false;
                        }
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    if !client_started {
                        client.Start()?;
                        client_started = true;
                    }
                    // Mark the device reservation before removing samples from
                    // the shared queue. This closes the handoff window where a
                    // safe-gap observer could otherwise see both stores empty.
                    device_padding_frames
                        .store(padding.saturating_add(available), Ordering::Release);
                    let buffer_ptr = render_client.GetBuffer(available)?;
                    let mut deck = shared_buffer.lock().unwrap();

                    if is_float {
                        let out_slice = std::slice::from_raw_parts_mut(
                            buffer_ptr as *mut f32,
                            (available as usize) * channels,
                        );

                        for i in 0..available as usize {
                            if let Some(sample) = deck.pop_front() {
                                let s = (sample as f32) / 32768.0;
                                for c in 0..channels {
                                    out_slice[i * channels + c] = s;
                                }
                            } else {
                                for c in 0..channels {
                                    out_slice[i * channels + c] = 0.0;
                                }
                            }
                        }
                    } else {
                        let out_slice = std::slice::from_raw_parts_mut(
                            buffer_ptr as *mut i16,
                            (available as usize) * channels,
                        );
                        for i in 0..available as usize {
                            if let Some(sample) = deck.pop_front() {
                                for c in 0..channels {
                                    out_slice[i * channels + c] = sample;
                                }
                            } else {
                                for c in 0..channels {
                                    out_slice[i * channels + c] = 0;
                                }
                            }
                        }
                    }

                    render_client.ReleaseBuffer(available, 0)?;
                }

                std::thread::sleep(Duration::from_millis(10));
            }

            if client_started {
                client.Stop()?;
            }
            Ok(())
        }
    }

    pub(super) fn play(&self, audio_data: &[u8], is_realtime: bool) {
        let effective_speed = if is_realtime {
            use crate::api::realtime_audio::WM_UPDATE_TTS_SPEED;
            use crate::overlay::realtime_webview::state::{CURRENT_TTS_SPEED, REALTIME_HWND};

            let auto_speed = compute_realtime_auto_speed();
            let speed = auto_speed.speed;

            let old_speed = CURRENT_TTS_SPEED.swap(speed, Ordering::Relaxed);
            if old_speed != speed {
                unsafe {
                    use crate::overlay::realtime_webview::state::TRANSLATION_HWND;
                    use windows::Win32::Foundation::{LPARAM, WPARAM};
                    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
                    if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
                        let _ = PostMessageW(
                            Some(REALTIME_HWND),
                            WM_UPDATE_TTS_SPEED,
                            WPARAM(speed as usize),
                            LPARAM(0),
                        );
                    }
                    if !std::ptr::addr_of!(TRANSLATION_HWND).read().is_invalid() {
                        let _ = PostMessageW(
                            Some(TRANSLATION_HWND),
                            WM_UPDATE_TTS_SPEED,
                            WPARAM(speed as usize),
                            LPARAM(0),
                        );
                    }
                }
            }
            speed
        } else {
            100
        };

        let speed_ratio = effective_speed as f64 / 100.0;

        let input_samples: Vec<i16> = audio_data
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        if input_samples.is_empty() {
            return;
        }

        let stretched_samples = if (speed_ratio - 1.0).abs() < 0.05 {
            input_samples
        } else if let Ok(mut wsola) = self.wsola.lock() {
            let result = wsola.stretch(&input_samples, speed_ratio);
            if result.is_empty() {
                return;
            }
            result
        } else {
            input_samples
        };

        let vol = if is_realtime {
            crate::overlay::realtime_webview::state::CURRENT_TTS_VOLUME.load(Ordering::Relaxed)
                as f32
                / 100.0
        } else {
            1.0
        };

        let output_samples: Vec<i16> = stretched_samples
            .iter()
            .flat_map(|&s| {
                let scaled = (s as f32 * vol).clamp(-32768.0, 32767.0) as i16;
                [scaled, scaled]
            })
            .collect();

        if let Ok(mut buf) = self.shared_buffer.lock() {
            buf.extend(output_samples);
        }
    }

    pub(crate) fn play_native_stream(&self, audio_data: &[u8]) {
        let vol = crate::overlay::realtime_webview::state::CURRENT_TTS_VOLUME
            .load(Ordering::Relaxed) as f32
            / 100.0;
        let output_samples = audio_data
            .chunks_exact(2)
            .flat_map(|chunk| {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                let scaled = (sample as f32 * vol).clamp(-32768.0, 32767.0) as i16;
                [scaled, scaled]
            })
            .collect::<Vec<_>>();

        if output_samples.is_empty() {
            return;
        }
        if let Ok(mut buf) = self.shared_buffer.lock() {
            buf.extend(output_samples);
        }
    }

    pub(super) fn drain(&self) {
        let start = Instant::now();
        loop {
            let len = self.shared_buffer.lock().map(|b| b.len()).unwrap_or(0);
            if len == 0 {
                break;
            }
            if start.elapsed() > Duration::from_secs(30) {
                eprintln!(
                    "[TTS Player] WARNING: drain() timed out after 30s, {} samples remaining",
                    len
                );
                if let Ok(mut buf) = self.shared_buffer.lock() {
                    buf.clear();
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    pub(crate) fn has_pending_playback(&self) -> bool {
        let queued = self
            .shared_buffer
            .lock()
            .map(|buf| !buf.is_empty())
            .unwrap_or(false);
        playback_work_present(queued, self.device_padding_frames.load(Ordering::Acquire))
    }

    pub(crate) fn stop(&self) {
        if let Ok(mut buf) = self.shared_buffer.lock() {
            buf.clear();
        }
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

fn compute_realtime_auto_speed() -> AutoSpeedState {
    use crate::overlay::realtime_webview::state::{
        COMMITTED_TRANSLATION_QUEUE, CURRENT_TTS_SPEED, REALTIME_S2S_AUDIO_BACKLOG,
        REALTIME_S2S_AUDIO_BACKLOG_MS, REALTIME_S2S_AUDIO_DELAY_MS, REALTIME_S2S_READY_BACKLOG_MS,
        REALTIME_TTS_AUTO_SPEED, REALTIME_TTS_SPEED,
    };

    let base_speed = REALTIME_TTS_SPEED.load(Ordering::Relaxed);
    let auto_enabled = REALTIME_TTS_AUTO_SPEED.load(Ordering::Relaxed);
    let text_queue_len = COMMITTED_TRANSLATION_QUEUE
        .lock()
        .map(|q| q.len())
        .unwrap_or(0);
    let s2s_segment_backlog = REALTIME_S2S_AUDIO_BACKLOG.load(Ordering::Relaxed);
    let s2s_delay_ms = REALTIME_S2S_AUDIO_DELAY_MS.load(Ordering::Relaxed);
    let s2s_backlog_ms = REALTIME_S2S_AUDIO_BACKLOG_MS.load(Ordering::Relaxed);
    let s2s_ready_backlog_ms = REALTIME_S2S_READY_BACKLOG_MS.load(Ordering::Relaxed);
    let text_queue_delay_ms = (text_queue_len as u32).saturating_mul(1_200);
    let s2s_ready_delay_ms = s2s_ready_backlog_ms.saturating_add(s2s_delay_ms);
    let s2s_waiting_delay_ms = s2s_backlog_ms.saturating_sub(5_000);
    let effective_delay_ms = text_queue_delay_ms
        .max(s2s_ready_delay_ms)
        .max(s2s_waiting_delay_ms);

    let target_speed = if auto_enabled {
        let boost = match effective_delay_ms {
            0..=1_800 => 0,
            1_801..=2_600 => 10,
            2_601..=3_600 => 25,
            3_601..=5_000 => 40,
            5_001..=7_000 => 55,
            _ => 75,
        };
        let cap = if text_queue_len > 0 || s2s_ready_backlog_ms > 0 {
            200
        } else if s2s_segment_backlog > 1 {
            base_speed.saturating_add(35).min(170)
        } else {
            base_speed.saturating_add(15).min(150)
        };
        (base_speed + boost).min(cap)
    } else {
        base_speed
    };

    let current_speed = CURRENT_TTS_SPEED.load(Ordering::Relaxed);
    let current_speed = if current_speed < base_speed && target_speed >= base_speed {
        base_speed
    } else {
        current_speed
    };
    let speed = if target_speed > current_speed {
        (current_speed + 15).min(target_speed)
    } else if target_speed < current_speed {
        let step_down = if s2s_segment_backlog > 0 || text_queue_len > 0 {
            2
        } else {
            5
        };
        current_speed.saturating_sub(step_down).max(target_speed)
    } else {
        current_speed
    }
    .clamp(50, 200);

    AutoSpeedState { speed }
}

fn playback_work_present(queued: bool, device_padding_frames: u32) -> bool {
    queued || device_padding_frames > 0
}

#[cfg(test)]
mod tests {
    use super::playback_work_present;

    #[test]
    fn device_padding_remains_pending_work_after_shared_queue_handoff() {
        assert!(playback_work_present(true, 0));
        assert!(playback_work_present(false, 480));
        assert!(!playback_work_present(false, 0));
    }
}
