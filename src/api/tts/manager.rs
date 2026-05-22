use super::types::{
    AudioEvent, QueuedRequest, SOURCE_SAMPLE_RATE, TtsCollectedAudio, TtsRequest, TtsRequestProfile,
};
use super::utils;
use std::collections::VecDeque;
use std::sync::{Arc, mpsc};
use std::sync::{
    Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

type PlaybackRequest = (mpsc::Receiver<AudioEvent>, isize, u64, u64, bool);

pub(crate) fn next_request_id_for_internal_use() -> u64 {
    REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Manages the persistent TTS WebSocket connection
pub struct TtsManager {
    /// Flag to indicate if the connection is ready
    _is_ready: AtomicBool,

    /// Queue for Socket Workers: (Request + Generation, Output Channel)
    pub work_queue: Mutex<VecDeque<(QueuedRequest, mpsc::Sender<AudioEvent>)>>,
    /// Signal for Socket Workers
    pub work_signal: Condvar,

    /// Queue for Player: (Input Channel, Window Handle, Request ID, Generation ID, IsRealtime)
    pub playback_queue: Mutex<VecDeque<PlaybackRequest>>,
    /// Signal for Player
    pub playback_signal: Condvar,

    /// Generation counter for interrupts (incrementing this invalidates old jobs)
    pub interrupt_generation: AtomicU64,

    /// Separate generation for the render thread to clear already-buffered audio.
    pub buffer_clear_generation: AtomicU64,

    /// Flag to indicate if audio is currently playing (set by player thread)
    pub is_playing: AtomicBool,

    /// Flag to shutdown the manager
    pub shutdown: AtomicBool,
}

impl TtsManager {
    pub fn new() -> Self {
        Self {
            _is_ready: AtomicBool::new(false),
            work_queue: Mutex::new(VecDeque::new()),
            work_signal: Condvar::new(),
            playback_queue: Mutex::new(VecDeque::new()),
            playback_signal: Condvar::new(),
            interrupt_generation: AtomicU64::new(0),
            buffer_clear_generation: AtomicU64::new(0),
            is_playing: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
        }
    }

    /// Check if TTS is ready to accept requests
    pub fn _is_ready(&self) -> bool {
        self._is_ready.load(Ordering::SeqCst)
    }

    /// Request TTS for the given text. Appends to queue (sequential playback).
    /// Returns the request ID.
    pub fn speak(&self, text: &str, hwnd: isize) -> u64 {
        self.speak_internal(text, hwnd, false, None)
    }

    /// Request TTS for realtime translation. Uses REALTIME_TTS_SPEED and auto-catchup.
    /// Returns the request ID.
    pub fn speak_realtime(&self, text: &str, hwnd: isize) -> u64 {
        self.speak_internal(text, hwnd, true, None)
    }

    /// Request TTS with a sandboxed per-request profile.
    pub fn speak_interrupt_with_profile(
        &self,
        text: &str,
        hwnd: isize,
        profile: TtsRequestProfile,
    ) -> u64 {
        self.speak_interrupt_internal(text, hwnd, Some(profile))
    }

    /// Generate a full TTS artifact without enqueueing it for immediate playback.
    #[cfg(test)]
    pub fn synthesize_to_wav_with_profile(
        &self,
        text: &str,
        profile: TtsRequestProfile,
    ) -> anyhow::Result<TtsCollectedAudio> {
        self.synthesize_to_wav_with_profile_cancel(text, profile, Arc::new(AtomicBool::new(false)))
    }

    pub fn synthesize_to_wav_with_profile_cancel(
        &self,
        text: &str,
        profile: TtsRequestProfile,
        cancel: Arc<AtomicBool>,
    ) -> anyhow::Result<TtsCollectedAudio> {
        let timeout = match profile.method {
            crate::config::TtsMethod::VieneuTts => std::time::Duration::from_secs(900),
            crate::config::TtsMethod::StepAudioEditX => std::time::Duration::from_secs(300),
            crate::config::TtsMethod::MagpieMultilingual => std::time::Duration::from_secs(240),
            _ => std::time::Duration::from_secs(90),
        };
        let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let generation = self.interrupt_generation.load(Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();

        {
            let mut wq = self.work_queue.lock().unwrap();
            wq.push_back((
                QueuedRequest {
                    req: TtsRequest {
                        _id: id,
                        text: text.to_string(),
                        hwnd: 0,
                        is_realtime: false,
                        profile: Some(profile),
                    },
                    generation,
                },
                tx,
            ));
        }
        self.work_signal.notify_one();

        let mut audio_bytes = Vec::new();
        let started = std::time::Instant::now();
        loop {
            if cancel.load(Ordering::SeqCst) {
                self.stop();
                return Err(anyhow::anyhow!("Generation cancelled"));
            }
            if generation < self.interrupt_generation.load(Ordering::SeqCst) {
                return Err(anyhow::anyhow!("Generation cancelled"));
            }
            if started.elapsed() >= timeout {
                self.stop();
                return Err(anyhow::anyhow!("TTS generation timed out"));
            }
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(AudioEvent::Data(data)) => audio_bytes.extend_from_slice(&data),
                Ok(AudioEvent::Error(error)) => return Err(anyhow::anyhow!(error)),
                Ok(AudioEvent::End) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        if cancel.load(Ordering::SeqCst)
            || generation < self.interrupt_generation.load(Ordering::SeqCst)
        {
            return Err(anyhow::anyhow!("Generation cancelled"));
        }

        if audio_bytes.is_empty() {
            return Err(anyhow::anyhow!("TTS generated no audio"));
        }

        let pcm_samples: Vec<i16> = audio_bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        let duration_ms = ((pcm_samples.len() as u64) * 1000) / SOURCE_SAMPLE_RATE as u64;
        let wav_data = crate::api::audio::encode_wav(&pcm_samples, SOURCE_SAMPLE_RATE, 1);

        Ok(TtsCollectedAudio {
            pcm_samples,
            wav_data,
            sample_rate: SOURCE_SAMPLE_RATE,
            duration_ms,
        })
    }

    /// Replay collected 24kHz mono PCM from a sample offset.
    pub fn play_pcm_interrupt(&self, pcm_samples: Vec<i16>, start_sample: usize) -> u64 {
        let new_gen = self.interrupt_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let total_samples = pcm_samples.len();
        let remaining_samples = total_samples.saturating_sub(start_sample.min(total_samples));

        eprintln!(
            "[TTS Playground] play-pcm req_id={} gen={} start_sample={} remaining_samples={} duration_ms={}",
            id,
            new_gen,
            start_sample,
            remaining_samples,
            (remaining_samples as u64 * 1000) / SOURCE_SAMPLE_RATE as u64
        );

        {
            let mut wq = self.work_queue.lock().unwrap();
            wq.clear();
        }
        let (tx, rx) = mpsc::channel();
        {
            let mut pq = self.playback_queue.lock().unwrap();
            pq.clear();
            pq.push_back((rx, 0, id, new_gen, false));
        }
        self.playback_signal.notify_one();

        std::thread::spawn(move || {
            let samples = pcm_samples.get(start_sample..).unwrap_or(&[]);
            let mut chunks_sent = 0usize;
            for chunk in samples.chunks(SOURCE_SAMPLE_RATE as usize) {
                let mut bytes = Vec::with_capacity(chunk.len() * 2);
                for sample in chunk {
                    bytes.extend_from_slice(&sample.to_le_bytes());
                }
                if tx.send(AudioEvent::Data(bytes)).is_err() {
                    eprintln!(
                        "[TTS Playground] play-pcm sender disconnected req_id={} chunks_sent={}",
                        id, chunks_sent
                    );
                    return;
                }
                chunks_sent += 1;
            }
            let _ = tx.send(AudioEvent::End);
            eprintln!(
                "[TTS Playground] play-pcm queued req_id={} chunks_sent={}",
                id, chunks_sent
            );
        });

        id
    }

    /// Internal speak implementation
    fn speak_internal(
        &self,
        text: &str,
        hwnd: isize,
        is_realtime: bool,
        profile: Option<TtsRequestProfile>,
    ) -> u64 {
        let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let current_gen = self.interrupt_generation.load(Ordering::SeqCst);

        eprintln!(
            "[TTS Manager] speak_internal: id={}, gen={}, hwnd={}, realtime={}, text_len={}",
            id,
            current_gen,
            hwnd,
            is_realtime,
            text.len()
        );

        let (tx, rx) = mpsc::channel();

        // Add to queues
        {
            let mut wq = self.work_queue.lock().unwrap();
            let queue_len = wq.len();
            wq.push_back((
                QueuedRequest {
                    req: TtsRequest {
                        _id: id,
                        text: text.to_string(),
                        hwnd,
                        is_realtime,
                        profile,
                    },
                    generation: current_gen,
                },
                tx,
            ));
            eprintln!(
                "[TTS Manager] Added to work_queue (queue size now: {})",
                queue_len + 1
            );
        }
        self.work_signal.notify_one();

        {
            let mut pq = self.playback_queue.lock().unwrap();
            pq.push_back((rx, hwnd, id, current_gen, is_realtime));
        }
        self.playback_signal.notify_one();

        id
    }

    /// Request TTS for the given text, interrupting any current speech.
    /// Clears the queue and stops current playback immediately.
    pub fn speak_interrupt(&self, text: &str, hwnd: isize) -> u64 {
        self.speak_interrupt_internal(text, hwnd, None)
    }

    fn speak_interrupt_internal(
        &self,
        text: &str,
        hwnd: isize,
        profile: Option<TtsRequestProfile>,
    ) -> u64 {
        // Increment generation to invalidate all currently running/queued work
        let new_gen = self.interrupt_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.buffer_clear_generation.fetch_add(1, Ordering::SeqCst);
        let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

        // Clear all queues
        {
            let mut wq = self.work_queue.lock().unwrap();
            wq.clear();
        }
        {
            let mut pq = self.playback_queue.lock().unwrap();
            pq.clear(); // Drops receivers, causing senders to error and workers to reset
        }

        // Push new request
        let (tx, rx) = mpsc::channel();

        {
            let mut wq = self.work_queue.lock().unwrap();
            wq.push_back((
                QueuedRequest {
                    req: TtsRequest {
                        _id: id,
                        text: text.to_string(),
                        hwnd,
                        is_realtime: false,
                        profile,
                    },
                    generation: new_gen,
                },
                tx,
            ));
        }
        self.work_signal.notify_one();

        {
            let mut pq = self.playback_queue.lock().unwrap();
            pq.push_back((rx, hwnd, id, new_gen, false));
        }
        // Force notify player to wake up and check generation/queue
        self.playback_signal.notify_one();

        id
    }

    /// Stop the current speech or cancel pending request
    pub fn stop(&self) {
        self.interrupt_generation.fetch_add(1, Ordering::SeqCst);
        self.buffer_clear_generation.fetch_add(1, Ordering::SeqCst);

        // Clear queues
        {
            let mut wq = self.work_queue.lock().unwrap();
            wq.clear();
        }
        {
            let mut pq = self.playback_queue.lock().unwrap();
            pq.clear();
        }

        // Wake up player to realize it should stop
        self.playback_signal.notify_all();
    }

    /// Stop speech for a specific request ID (only if it's the current one)
    pub fn stop_if_active(&self, _request_id: u64) {
        // Simplified to just stop
        self.stop();
    }

    /// Check if this request ID is currently active
    pub fn is_speaking(&self, _request_id: u64) -> bool {
        self.has_pending_audio()
    }

    /// Check if there's any pending TTS audio (in work queue, playback queue, or currently playing)
    pub fn has_pending_audio(&self) -> bool {
        // Check if actively playing first (most common case when user wants to stop)
        if self.is_playing.load(Ordering::SeqCst) {
            return true;
        }
        let wq_has = self
            .work_queue
            .lock()
            .map(|q| !q.is_empty())
            .unwrap_or(false);
        let pq_has = self
            .playback_queue
            .lock()
            .map(|q| !q.is_empty())
            .unwrap_or(false);
        wq_has || pq_has
    }

    /// Shutdown the TTS manager
    pub fn _shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.interrupt_generation.fetch_add(1, Ordering::SeqCst);
        self.work_signal.notify_all();
        self.playback_signal.notify_all();
    }

    /// List available audio output devices (ID, Name)
    pub fn get_output_devices() -> Vec<(String, String)> {
        utils::get_output_devices()
    }
}

#[cfg(test)]
mod tests {
    use super::TtsManager;
    use crate::api::tts::types::AudioEvent;
    use crate::api::tts::types::TtsRequestProfile;
    use crate::config::{StepAudioSettings, TtsMethod};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    fn step_audio_e2e_profile() -> TtsRequestProfile {
        TtsRequestProfile {
            method: TtsMethod::StepAudioEditX,
            gemini_model: String::new(),
            gemini_voice: String::new(),
            gemini_speed: String::new(),
            gemini_instruction: String::new(),
            gemini_language_conditions: Vec::new(),
            gemini_parallel_requests: 2,
            google_speed: String::new(),
            edge_voice: String::new(),
            edge_settings: Default::default(),
            step_audio_settings: StepAudioSettings::default(),
            magpie_settings: Default::default(),
            kokoro_settings: Default::default(),
            supertonic_settings: Default::default(),
            vieneu_settings: Default::default(),
            language_code_override: Some("eng".to_string()),
        }
    }

    #[test]
    fn synthesize_to_wav_with_step_audio_profile_e2e_when_enabled() {
        if std::env::var("SGT_STEP_AUDIO_MANAGER_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping Step Audio manager e2e test; set SGT_STEP_AUDIO_MANAGER_E2E=1 to run it"
            );
            return;
        }

        let manager = Arc::new(TtsManager::new());
        let worker_manager = manager.clone();
        let worker = std::thread::spawn(move || {
            crate::api::tts::worker::run_socket_worker(worker_manager);
        });

        let audio = manager
            .synthesize_to_wav_with_profile(
                "Step Audio manager artifact path test.",
                step_audio_e2e_profile(),
            )
            .expect("synthesize Step Audio artifact");

        assert_eq!(audio.sample_rate, 24_000);
        assert!(
            audio.duration_ms >= 500,
            "Step Audio artifact is too short: {}ms",
            audio.duration_ms
        );
        assert!(
            audio.wav_data.len() > 44,
            "Step Audio artifact WAV data should include samples"
        );

        let start_sample = (audio.pcm_samples.len() / 4).min(24_000);
        let playback_id = manager.play_pcm_interrupt(audio.pcm_samples.clone(), start_sample);
        let playback_job = manager
            .playback_queue
            .lock()
            .expect("lock playback queue")
            .pop_front()
            .expect("playback job queued");
        let (rx, hwnd, req_id, _generation, is_realtime) = playback_job;
        assert_eq!(hwnd, 0);
        assert_eq!(req_id, playback_id);
        assert!(!is_realtime);

        let mut playback_bytes = 0usize;
        loop {
            match rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("playback PCM event")
            {
                AudioEvent::Data(data) => playback_bytes += data.len(),
                AudioEvent::Error(error) => panic!("playback error: {error}"),
                AudioEvent::End => break,
            }
        }
        assert!(
            playback_bytes > 0,
            "Step Audio generated PCM should enqueue playable audio bytes"
        );

        manager._shutdown();
        worker.join().expect("join Step Audio worker");
    }

    #[test]
    fn step_audio_playback_loopback_e2e_when_enabled() {
        if std::env::var("SGT_STEP_AUDIO_LOOPBACK_E2E").as_deref() != Ok("1") {
            eprintln!(
                "skipping Step Audio loopback e2e test; set SGT_STEP_AUDIO_LOOPBACK_E2E=1 to run it"
            );
            return;
        }

        let manager = Arc::new(TtsManager::new());
        let worker_manager = manager.clone();
        let worker = std::thread::spawn(move || {
            crate::api::tts::worker::run_socket_worker(worker_manager);
        });

        let player_manager = manager.clone();
        let player = std::thread::spawn(move || {
            crate::api::tts::player::run_player_thread(player_manager);
        });

        let host =
            cpal::host_from_id(cpal::HostId::Wasapi).unwrap_or_else(|_| cpal::default_host());
        let device = host
            .default_output_device()
            .expect("default output device for loopback capture");
        let config = device
            .default_output_config()
            .expect("default output config for loopback capture");
        let stream_config: cpal::StreamConfig = config.clone().into();
        let captured_samples = Arc::new(AtomicU64::new(0));
        let captured_energy = Arc::new(AtomicU64::new(0));
        let capture_active = Arc::new(AtomicBool::new(true));
        let samples_for_callback = captured_samples.clone();
        let energy_for_callback = captured_energy.clone();
        let active_for_callback = capture_active.clone();
        let err_fn = |err| eprintln!("[StepAudioLoopbackE2E] loopback stream error: {err}");

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &_| {
                    if !active_for_callback.load(Ordering::Relaxed) {
                        return;
                    }
                    samples_for_callback.fetch_add(data.len() as u64, Ordering::Relaxed);
                    let energy: u64 = data
                        .iter()
                        .map(|sample| (sample.abs() * 1_000_000.0) as u64)
                        .sum();
                    energy_for_callback.fetch_add(energy, Ordering::Relaxed);
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _: &_| {
                    if !active_for_callback.load(Ordering::Relaxed) {
                        return;
                    }
                    samples_for_callback.fetch_add(data.len() as u64, Ordering::Relaxed);
                    let energy: u64 = data.iter().map(|sample| sample.unsigned_abs() as u64).sum();
                    energy_for_callback.fetch_add(energy, Ordering::Relaxed);
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                &stream_config,
                move |data: &[u16], _: &_| {
                    if !active_for_callback.load(Ordering::Relaxed) {
                        return;
                    }
                    samples_for_callback.fetch_add(data.len() as u64, Ordering::Relaxed);
                    let energy: u64 = data
                        .iter()
                        .map(|sample| sample.abs_diff(32768) as u64)
                        .sum();
                    energy_for_callback.fetch_add(energy, Ordering::Relaxed);
                },
                err_fn,
                None,
            ),
            other => panic!("unsupported loopback sample format: {other:?}"),
        }
        .expect("build default output loopback stream");
        stream.play().expect("start default output loopback stream");

        let audio = manager
            .synthesize_to_wav_with_profile(
                "Step Audio loopback playback test.",
                step_audio_e2e_profile(),
            )
            .expect("synthesize Step Audio for loopback playback");
        assert!(
            audio.duration_ms >= 500,
            "Step Audio loopback source is too short: {}ms",
            audio.duration_ms
        );

        manager.play_pcm_interrupt(audio.pcm_samples, 0);
        let started = Instant::now();
        while manager.has_pending_audio() && started.elapsed() < Duration::from_secs(45) {
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_millis(500));
        capture_active.store(false, Ordering::Relaxed);
        drop(stream);

        let samples = captured_samples.load(Ordering::Relaxed);
        let energy = captured_energy.load(Ordering::Relaxed);
        manager._shutdown();
        worker.join().expect("join Step Audio worker");
        player.join().expect("join TTS player");

        assert!(
            samples > 0,
            "loopback capture should receive render-device samples"
        );
        assert!(
            energy > 0,
            "Step Audio playback should produce non-silent loopback output"
        );
    }
}
