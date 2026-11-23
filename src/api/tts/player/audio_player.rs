use std::collections::VecDeque;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

use super::device_failure::DeviceFailure;
use super::render_state::{RenderState, playback_format, with_output_deck};
use crate::api::tts::types::SOURCE_SAMPLE_RATE;
use crate::api::tts::wsola::WsolaStretcher;
use auto_speed::compute_realtime_auto_speed;
use output_device::{StreamRetry, default_render_endpoint_id, resolve_render_device};
mod auto_speed;
mod output_device;
mod pcm_frames;
mod stream_tail;

struct WasapiApartment;

impl Drop for WasapiApartment {
    fn drop(&mut self) {
        wasapi::deinitialize();
    }
}

/// Simple audio player using Windows WASAPI with loopback exclusion.
pub(crate) struct AudioPlayer {
    _sample_rate: u32,
    shared_buffer: Arc<Mutex<VecDeque<i16>>>,
    device_padding_frames: Arc<AtomicU32>,
    submitted_frames: Arc<AtomicU64>,
    reset_generation: Arc<AtomicU64>,
    device_failure: Arc<DeviceFailure>,
    shutdown: Arc<AtomicBool>,
    /// Paired with `shared_buffer`; wakes the idle render thread when audio is
    /// queued, a reset is requested or the player shuts down.
    deck_ready: Arc<Condvar>,
    _thread: Option<std::thread::JoinHandle<()>>,
    wsola: Mutex<WsolaStretcher>,
}

/// Safety-net wake for a parked idle render thread.
const IDLE_PARK: Duration = Duration::from_secs(5);
const DEFAULT_DEVICE_CHECK: Duration = Duration::from_millis(750);

impl AudioPlayer {
    pub(crate) fn new(sample_rate: u32) -> Self {
        Self::new_with_progress(sample_rate, Arc::new(AtomicU64::new(0)))
    }

    pub(super) fn new_with_progress(sample_rate: u32, played_frames: Arc<AtomicU64>) -> Self {
        let shared_buffer: Arc<Mutex<VecDeque<i16>>> = Arc::new(Mutex::new(VecDeque::new()));
        let buffer_clone = shared_buffer.clone();
        let device_padding_frames = Arc::new(AtomicU32::new(0));
        let device_padding_clone = device_padding_frames.clone();
        let submitted_frames = Arc::new(AtomicU64::new(0));
        let submitted_clone = submitted_frames.clone();
        let reset_generation = Arc::new(AtomicU64::new(0));
        let reset_clone = reset_generation.clone();
        let device_failure = Arc::new(DeviceFailure::default());
        let failure_clone = device_failure.clone();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();
        let deck_ready = Arc::new(Condvar::new());
        let deck_ready_clone = deck_ready.clone();

        let thread = std::thread::spawn(move || {
            crate::log_info!("[TTS Player] WASAPI thread starting...");
            if wasapi::initialize_mta().is_err() {
                crate::log_info!("[TTS Player] ERROR: Failed to initialize COM for WASAPI thread");
                return;
            }
            let _apartment = WasapiApartment;

            eprintln!("[TTS Player] COM initialized, entering audio stream loop...");

            let mut retry = StreamRetry::default();
            while !shutdown_clone.load(Ordering::Relaxed) {
                let target_device_id = Self::current_target_device_id();

                let stream_started = Instant::now();
                let stream_result = unsafe {
                    Self::run_wasapi_excluded(
                        sample_rate,
                        target_device_id,
                        &deck_ready_clone,
                        RenderState {
                            shared_buffer: buffer_clone.clone(),
                            device_padding_frames: device_padding_clone.clone(),
                            shutdown: shutdown_clone.clone(),
                            reset_generation: reset_clone.clone(),
                            submitted_frames: submitted_clone.clone(),
                            played_frames: played_frames.clone(),
                            device_failure: failure_clone.clone(),
                        },
                    )
                };
                {
                    let _deck = buffer_clone.lock().unwrap();
                    failure_clone
                        .finish_stream(stream_result.is_err(), reset_clone.load(Ordering::SeqCst));
                    device_padding_clone.store(0, Ordering::Release);
                }
                match stream_result {
                    Ok(()) => break,
                    Err(e) => {
                        if shutdown_clone.load(Ordering::Relaxed) {
                            break;
                        }
                        let (delay, log) =
                            retry.record_failure(&e.to_string(), stream_started.elapsed());
                        if log {
                            crate::log_info!(
                                "[TTS Player] WARNING: WASAPI stream failed: {}. Reinitializing...",
                                e
                            );
                        }
                        std::thread::sleep(delay);
                    }
                }
            }
        });

        Self {
            _sample_rate: sample_rate,
            shared_buffer,
            device_padding_frames,
            submitted_frames,
            reset_generation,
            device_failure,
            shutdown,
            deck_ready,
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
        sample_rate: u32,
        target_device_id: Option<String>,
        deck_ready: &Condvar,
        state: RenderState,
    ) -> anyhow::Result<()> {
        let RenderState {
            shared_buffer,
            device_padding_frames,
            shutdown,
            reset_generation,
            submitted_frames,
            played_frames,
            device_failure,
        } = state;
        unsafe {
            eprintln!("[TTS WASAPI] Initializing audio output...");

            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

            let (device, follows_default_device) =
                resolve_render_device(&enumerator, target_device_id.as_deref())?;

            let active_device_id = crate::api::audio::endpoint_id::read(&device)?;
            crate::log_info!(
                "[TTS WASAPI] selected_endpoint={:?} follows_default={}",
                active_device_id,
                follows_default_device
            );

            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            // The queue contains mono PCM at `sample_rate`; let the shared-mode
            // audio engine convert it to the endpoint's actual mix format.
            let stream_format = playback_format(sample_rate);
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                1000000,
                0,
                &stream_format,
                None,
            )?;

            let buffer_size = client.GetBufferSize()?;
            let render_client: IAudioRenderClient = client.GetService()?;

            let mut client_started = false;

            crate::log_info!(
                "[TTS WASAPI] Audio client initialized successfully (buffer size: {})",
                buffer_size
            );

            let mut last_clear_gen = reset_generation.load(Ordering::SeqCst);
            device_failure.reset_applied(last_clear_gen);
            let mut next_default_device_check = Instant::now() + DEFAULT_DEVICE_CHECK;
            let mut last_padding = 0u32;

            while !shutdown.load(Ordering::Relaxed) {
                let padding = with_output_deck(&shared_buffer, || -> anyhow::Result<u32> {
                    // Reset and account for consumed frames under the same lock
                    // that establishes the next request's baseline.
                    let current_clear_gen = reset_generation.load(Ordering::SeqCst);
                    synchronize_reset(&mut last_clear_gen, current_clear_gen, || {
                        if client_started {
                            client.Stop()?;
                            client_started = false;
                        }
                        client.Reset()?;
                        device_padding_frames.store(0, Ordering::Release);
                        last_padding = 0;
                        device_failure.reset_applied(current_clear_gen);
                        Ok(())
                    })?;
                    if follows_default_device && Instant::now() >= next_default_device_check {
                        let current_default_id = default_render_endpoint_id(&enumerator);
                        if current_default_id.is_some() && current_default_id != active_device_id {
                            return Err(anyhow::anyhow!(
                                "Windows default audio output changed; rebuilding TTS output"
                            ));
                        }
                        next_default_device_check = Instant::now() + DEFAULT_DEVICE_CHECK;
                    }
                    let padding = client.GetCurrentPadding()?;
                    played_frames.fetch_add(
                        last_padding.saturating_sub(padding) as u64,
                        Ordering::Release,
                    );
                    last_padding = padding;
                    device_failure.set_padding(padding);
                    device_padding_frames.store(padding, Ordering::Release);
                    Ok(padding)
                })?;
                let available = buffer_size.saturating_sub(padding);

                if available > 0 {
                    let mut deck = shared_buffer.lock().unwrap();
                    if reset_generation.load(Ordering::SeqCst) != last_clear_gen {
                        continue;
                    }
                    if deck.is_empty() {
                        if client_started && padding == 0 {
                            client.Stop()?;
                            client_started = false;
                        }
                        if client_started {
                            drop(deck);
                            std::thread::sleep(Duration::from_millis(20));
                            continue;
                        }
                        // Fully idle: park until audio, a reset or shutdown
                        // arrives, then re-check the default endpoint first.
                        drop(park_until_work(deck_ready, deck, || {
                            shutdown.load(Ordering::Relaxed)
                                || reset_generation.load(Ordering::SeqCst) != last_clear_gen
                        }));
                        next_default_device_check = Instant::now();
                        continue;
                    }
                    if !client_started {
                        client.Start()?;
                        client_started = true;
                        crate::log_info!(
                            "[TTS WASAPI] playback_started endpoint={:?}",
                            active_device_id
                        );
                    }
                    // Mark the device reservation before removing samples from
                    // the shared queue. This closes the handoff window where a
                    // safe-gap observer could otherwise see both stores empty.
                    device_padding_frames
                        .store(padding.saturating_add(available), Ordering::Release);
                    let buffer_ptr = render_client.GetBuffer(available)?;

                    let out_slice =
                        std::slice::from_raw_parts_mut(buffer_ptr as *mut i16, available as usize);
                    device_failure.begin_submission(deck.len().min(available as usize) as u32);
                    for sample in out_slice {
                        *sample = deck.pop_front().unwrap_or(0);
                    }

                    render_client.ReleaseBuffer(available, 0)?;
                    device_failure.finish_submission(padding.saturating_add(available));
                    submitted_frames.fetch_add(available as u64, Ordering::Relaxed);
                    last_padding = padding.saturating_add(available);
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
                }
            }
            speed
        } else {
            100
        };

        let speed_ratio = effective_speed as f64 / 100.0;

        let input_samples: Vec<i16> = pcm_frames::decode_pcm16_le(audio_data).collect();

        if input_samples.is_empty() {
            return;
        }

        let mut wsola = self.wsola.lock().unwrap_or_else(|error| error.into_inner());
        let stretched_samples = wsola.stretch(&input_samples, speed_ratio);
        if stretched_samples.is_empty() {
            return;
        }

        let vol = if is_realtime {
            crate::overlay::realtime_webview::state::CURRENT_TTS_VOLUME.load(Ordering::Relaxed)
                as f32
                / 100.0
        } else {
            1.0
        };

        self.enqueue(pcm_frames::scaled_stereo_frames(stretched_samples, vol));
    }

    /// Appends samples to the deck and wakes a parked render thread.
    fn enqueue(&self, samples: Vec<i16>) {
        if let Ok(mut buf) = self.shared_buffer.lock() {
            buf.extend(samples);
            self.deck_ready.notify_all();
        }
    }

    pub(crate) fn play_native_stream(&self, audio_data: &[u8]) {
        let vol = crate::overlay::realtime_webview::state::CURRENT_TTS_VOLUME
            .load(Ordering::Relaxed) as f32
            / 100.0;
        let output_samples =
            pcm_frames::scaled_stereo_frames(pcm_frames::decode_pcm16_le(audio_data), vol);

        if output_samples.is_empty() {
            return;
        }
        self.enqueue(output_samples);
    }

    pub(super) fn drain(&self, cancelled: impl Fn() -> bool) -> bool {
        let mut last_progress = Instant::now();
        let mut last_submitted = self.submitted_frames();
        let mut last_padding = self.device_padding_frames.load(Ordering::Acquire);
        loop {
            if cancelled() {
                self.stop();
                return false;
            }
            let len = self.shared_buffer.lock().map(|b| b.len()).unwrap_or(0);
            let padding = self.device_padding_frames.load(Ordering::Acquire);
            if cancelled() {
                self.stop();
                return false;
            }
            if !playback_work_present(len != 0, padding) {
                break;
            }
            let submitted = self.submitted_frames();
            if padding != last_padding {
                last_padding = padding;
                last_progress = Instant::now();
            }
            if playback_stalled(
                &mut last_submitted,
                &mut last_progress,
                submitted,
                Instant::now(),
            ) {
                eprintln!(
                    "[TTS Player] WARNING: playback stalled for 30s, {} samples remaining",
                    len
                );
                self.stop();
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    }

    pub(crate) fn has_pending_playback(&self) -> bool {
        let queued = self
            .shared_buffer
            .lock()
            .map(|buf| !buf.is_empty())
            .unwrap_or(false);
        playback_work_present(queued, self.device_padding_frames.load(Ordering::Acquire))
    }

    pub(crate) fn submitted_frames(&self) -> u64 {
        self.submitted_frames.load(Ordering::Relaxed)
    }

    pub(super) fn failure_generation(&self) -> u64 {
        self.device_failure.generation()
    }

    pub(super) fn begin_request(
        &self,
        progress: Option<&crate::api::tts::manager::PlaybackProgress>,
    ) -> u64 {
        with_output_deck(&self.shared_buffer, || {
            if let Some(progress) = progress {
                progress.start();
            }
            self.failure_generation()
        })
    }

    pub(crate) fn stop(&self) {
        let mut wsola = self.wsola.lock().unwrap_or_else(|error| error.into_inner());
        wsola.reset();
        if let Ok(mut buf) = self.shared_buffer.lock() {
            buf.clear();
            self.reset_generation.fetch_add(1, Ordering::SeqCst);
            self.deck_ready.notify_all();
        }
    }
}

/// Blocks an idle render thread on the deck until audio is queued or `wake`
/// reports a reset/shutdown, bounded by [`IDLE_PARK`].
fn park_until_work<'a>(
    deck_ready: &Condvar,
    deck: std::sync::MutexGuard<'a, VecDeque<i16>>,
    wake: impl Fn() -> bool,
) -> std::sync::MutexGuard<'a, VecDeque<i16>> {
    deck_ready
        .wait_timeout_while(deck, IDLE_PARK, |deck| deck.is_empty() && !wake())
        .map(|(deck, _)| deck)
        .unwrap_or_else(|error| error.into_inner().0)
}

fn synchronize_reset(
    applied: &mut u64,
    requested: u64,
    reset: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    if requested != *applied {
        reset()?;
        *applied = requested;
    }
    Ok(())
}

fn playback_stalled(
    last_submitted: &mut u64,
    last_progress: &mut Instant,
    submitted: u64,
    now: Instant,
) -> bool {
    if submitted != *last_submitted {
        *last_submitted = submitted;
        *last_progress = now;
    }
    now.duration_since(*last_progress) > Duration::from_secs(30)
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _deck = self.shared_buffer.lock();
        self.deck_ready.notify_all();
    }
}

fn playback_work_present(queued: bool, device_padding_frames: u32) -> bool {
    queued || device_padding_frames > 0
}

#[cfg(test)]
#[path = "audio_player_tests.rs"]
mod tests;
