use super::types::{
    AudioEvent, QueuedRequest, SOURCE_SAMPLE_RATE, TtsCollectedAudio, TtsRequest, TtsRequestProfile,
};
#[cfg(not(feature = "recorder-worker"))]
use super::utils;
use std::collections::VecDeque;
use std::sync::{Arc, mpsc};
use std::sync::{
    Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[cfg(not(feature = "recorder-worker"))]
mod playback;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use playback::RealtimeAudioSink;
#[cfg(all(test, not(feature = "recorder-worker")))]
mod direct_realtime_tests;
#[cfg(not(feature = "recorder-worker"))]
mod progress;
#[cfg(not(feature = "recorder-worker"))]
pub(crate) use progress::PlaybackProgress;
#[cfg(all(test, not(feature = "recorder-worker")))]
mod queue_fixture_tests;
#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests;

static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(not(feature = "recorder-worker"))]
type PlaybackRequest = (
    mpsc::Receiver<AudioEvent>,
    isize,
    u64,
    u64,
    bool,
    Arc<AtomicBool>,
    Option<Arc<crate::overlay::result::ChainCancelToken>>,
);

#[cfg(not(feature = "recorder-worker"))]
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
    #[cfg(not(feature = "recorder-worker"))]
    pub playback_queue: Mutex<VecDeque<PlaybackRequest>>,
    /// Signal for Player
    #[cfg(not(feature = "recorder-worker"))]
    pub playback_signal: Condvar,

    #[cfg(not(feature = "recorder-worker"))]
    playback_operations: Mutex<()>,
    #[cfg(not(feature = "recorder-worker"))]
    pub(super) playback_device: Mutex<std::sync::Weak<super::player::audio_player::AudioPlayer>>,
    #[cfg(not(feature = "recorder-worker"))]
    pub(super) active_playback: Mutex<Option<(u64, bool, Arc<AtomicBool>)>>,
    #[cfg(not(feature = "recorder-worker"))]
    realtime_epoch: AtomicU64,
    /// Admitted live translation text requests, including the active request.
    #[cfg(not(feature = "recorder-worker"))]
    realtime_text_requests: Mutex<std::collections::HashSet<u64>>,
    #[cfg(not(feature = "recorder-worker"))]
    realtime_direct: Mutex<Option<std::sync::Weak<dyn playback::RealtimeAudioSink>>>,
    #[cfg(not(feature = "recorder-worker"))]
    pub(super) playback_played_frames: Arc<AtomicU64>,
    #[cfg(not(feature = "recorder-worker"))]
    pub(super) pending_progress: Mutex<std::collections::HashMap<u64, Arc<PlaybackProgress>>>,

    /// Generation counter for interrupts (incrementing this invalidates old jobs)
    pub interrupt_generation: AtomicU64,

    /// Flag to indicate if audio is currently playing (set by player thread)
    #[cfg(not(feature = "recorder-worker"))]
    pub is_playing: AtomicBool,

    /// Flag to shutdown the manager
    pub shutdown: AtomicBool,
}

impl Default for TtsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TtsManager {
    pub fn new() -> Self {
        Self {
            _is_ready: AtomicBool::new(false),
            work_queue: Mutex::new(VecDeque::new()),
            work_signal: Condvar::new(),
            #[cfg(not(feature = "recorder-worker"))]
            playback_queue: Mutex::new(VecDeque::new()),
            #[cfg(not(feature = "recorder-worker"))]
            playback_signal: Condvar::new(),
            #[cfg(not(feature = "recorder-worker"))]
            playback_operations: Mutex::new(()),
            #[cfg(not(feature = "recorder-worker"))]
            playback_device: Mutex::new(std::sync::Weak::new()),
            #[cfg(not(feature = "recorder-worker"))]
            active_playback: Mutex::new(None),
            #[cfg(not(feature = "recorder-worker"))]
            realtime_epoch: AtomicU64::new(0),
            #[cfg(not(feature = "recorder-worker"))]
            realtime_text_requests: Mutex::new(std::collections::HashSet::new()),
            #[cfg(not(feature = "recorder-worker"))]
            realtime_direct: Mutex::new(None),
            #[cfg(not(feature = "recorder-worker"))]
            playback_played_frames: Arc::new(AtomicU64::new(0)),
            #[cfg(not(feature = "recorder-worker"))]
            pending_progress: Mutex::new(std::collections::HashMap::new()),
            interrupt_generation: AtomicU64::new(0),
            #[cfg(not(feature = "recorder-worker"))]
            is_playing: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
        }
    }

    /// Check if TTS is ready to accept requests
    #[cfg(not(feature = "recorder-worker"))]
    pub fn _is_ready(&self) -> bool {
        self._is_ready.load(Ordering::SeqCst)
    }

    /// Request TTS for the given text. Appends to queue (sequential playback).
    /// Returns the request ID.
    #[cfg(not(feature = "recorder-worker"))]
    pub fn speak(&self, text: &str, hwnd: isize) -> u64 {
        self.speak_internal(text, hwnd, false, None, None, None)
            .unwrap()
    }

    /// Request TTS for realtime translation. Uses REALTIME_TTS_SPEED and auto-catchup.
    /// Returns the request ID.
    #[cfg(not(feature = "recorder-worker"))]
    pub fn speak_realtime(&self, text: &str, hwnd: isize, epoch: u64) -> Option<u64> {
        self.speak_internal(text, hwnd, true, None, Some(epoch), None)
    }

    /// Request TTS with a sandboxed per-request profile.
    #[cfg(not(feature = "recorder-worker"))]
    pub fn speak_with_profile(&self, text: &str, hwnd: isize, profile: TtsRequestProfile) -> u64 {
        self.speak_internal(text, hwnd, false, Some(profile), None, None)
            .unwrap()
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn speak_cancellable(
        &self,
        text: &str,
        hwnd: isize,
        token: Arc<crate::overlay::result::ChainCancelToken>,
    ) -> Option<u64> {
        self.speak_internal(text, hwnd, false, None, None, Some(token))
    }

    /// Generate a full TTS artifact without enqueueing it for immediate playback.
    #[cfg(all(test, not(feature = "recorder-worker")))]
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
        let timeout = std::time::Duration::from_secs(90);
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
                    cancel: cancel.clone(),
                    artifact: true,
                },
                tx,
            ));
        }
        self.work_signal.notify_one();

        let mut audio_bytes = Vec::new();
        let started = std::time::Instant::now();
        loop {
            if cancel.load(Ordering::SeqCst) {
                self.remove_queued_request(id);
                return Err(anyhow::anyhow!("Generation cancelled"));
            }
            if self.shutdown.load(Ordering::SeqCst) {
                return Err(anyhow::anyhow!("Generation cancelled"));
            }
            if started.elapsed() >= timeout {
                cancel.store(true, Ordering::SeqCst);
                self.remove_queued_request(id);
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

        if cancel.load(Ordering::SeqCst) || self.shutdown.load(Ordering::SeqCst) {
            return Err(anyhow::anyhow!("Generation cancelled"));
        }

        if audio_bytes.is_empty() {
            return Err(anyhow::anyhow!("TTS generated no audio"));
        }

        let pcm_samples: Vec<i16> = audio_bytes
            .as_chunks::<2>()
            .0
            .iter()
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

    fn remove_queued_request(&self, request_id: u64) {
        if let Ok(mut queue) = self.work_queue.lock() {
            queue.retain(|(request, _)| request.req._id != request_id);
        }
    }

    /// Replay collected 24kHz mono PCM from a sample offset.
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn play_pcm(
        &self,
        pcm_samples: Vec<i16>,
        start_sample: usize,
    ) -> (u64, Arc<PlaybackProgress>) {
        let _operation = self.playback_operations.lock().unwrap();
        let new_gen = self.interrupt_generation.load(Ordering::SeqCst);
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

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(PlaybackProgress::new(
            self.playback_played_frames.clone(),
            cancel.clone(),
        ));
        self.pending_progress
            .lock()
            .unwrap()
            .insert(id, progress.clone());
        {
            let mut pq = self.playback_queue.lock().unwrap();
            pq.push_back((rx, 0, id, new_gen, false, cancel, None));
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

        (id, progress)
    }

    /// Internal speak implementation
    #[cfg(not(feature = "recorder-worker"))]
    fn speak_internal(
        &self,
        text: &str,
        hwnd: isize,
        is_realtime: bool,
        profile: Option<TtsRequestProfile>,
        expected_epoch: Option<u64>,
        chain_token: Option<Arc<crate::overlay::result::ChainCancelToken>>,
    ) -> Option<u64> {
        let _operation = self.playback_operations.lock().unwrap();
        if expected_epoch.is_some_and(|epoch| epoch != self.realtime_epoch.load(Ordering::SeqCst)) {
            return None;
        }
        let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let current_gen = self.interrupt_generation.load(Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        if let Some(token) = &chain_token {
            token.register_cancel_flag(&cancel);
            if token.is_cancelled() {
                return None;
            }
        }
        utils::set_tts_request(hwnd, id);

        eprintln!(
            "[TTS Manager] speak_internal: id={}, gen={}, hwnd={}, realtime={}, text_len={}",
            id,
            current_gen,
            hwnd,
            is_realtime,
            text.len()
        );

        let (tx, rx) = mpsc::channel();

        if is_realtime {
            self.realtime_text_requests.lock().unwrap().insert(id);
        }

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
                    cancel: cancel.clone(),
                    artifact: false,
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
            pq.push_back((rx, hwnd, id, current_gen, is_realtime, cancel, chain_token));
        }
        self.playback_signal.notify_one();

        Some(id)
    }

    /// Stop the current speech or cancel pending request
    #[cfg(not(feature = "recorder-worker"))]
    pub fn stop(&self) {
        let _operation = self.playback_operations.lock().unwrap();
        self.interrupt_generation.fetch_add(1, Ordering::SeqCst);
        self.realtime_epoch.fetch_add(1, Ordering::SeqCst);
        self.realtime_text_requests.lock().unwrap().clear();
        self.stop_direct_realtime();
        self.halt_playback_device();

        // Clear queues
        {
            let mut wq = self.work_queue.lock().unwrap();
            wq.retain(|(request, _)| request.artifact);
        }
        #[cfg(not(feature = "recorder-worker"))]
        self.clear_queued_playback();

        // Wake up player to realize it should stop
        #[cfg(not(feature = "recorder-worker"))]
        self.playback_signal.notify_all();
    }

    /// Check if there's any pending TTS audio (in work queue, playback queue, or currently playing)
    #[cfg(not(feature = "recorder-worker"))]
    pub fn has_pending_audio(&self) -> bool {
        // Check if actively playing first (most common case when user wants to stop)
        if self.is_playing.load(Ordering::SeqCst) {
            return true;
        }
        let pq_has = self
            .playback_queue
            .lock()
            .map(|q| !q.is_empty())
            .unwrap_or(false);
        pq_has
            || self
                .active_playback
                .lock()
                .map(|active| active.is_some())
                .unwrap_or(false)
    }

    /// Shutdown the TTS manager
    #[cfg(not(feature = "recorder-worker"))]
    pub fn _shutdown(&self) {
        let _operation = self.playback_operations.lock().unwrap();
        self.shutdown.store(true, Ordering::SeqCst);
        self.interrupt_generation.fetch_add(1, Ordering::SeqCst);
        self.realtime_text_requests.lock().unwrap().clear();
        self.stop_direct_realtime();
        self.halt_playback_device();
        self.work_signal.notify_all();
        self.playback_signal.notify_all();
    }
}
