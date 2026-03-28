//! Manager for Gemini Live LLM connection pool

use super::types::{LiveEvent, LiveInputContent, LiveRequest, QueuedLiveRequest};
use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::{
    Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Manager for the Gemini Live LLM connection pool
/// Similar architecture to TtsManager for consistency
pub struct GeminiLiveManager {
    /// Queue for workers: requests waiting to be processed
    pub work_queue: Mutex<VecDeque<QueuedLiveRequest>>,
    /// Signal for workers to wake up
    pub work_signal: Condvar,

    /// Generation counter for interrupts
    pub interrupt_generation: AtomicU64,

    /// Shutdown flag
    pub shutdown: AtomicBool,
}

impl GeminiLiveManager {
    pub fn new() -> Self {
        Self {
            work_queue: Mutex::new(VecDeque::new()),
            work_signal: Condvar::new(),
            interrupt_generation: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
        }
    }

    /// Send a request to the Gemini Live LLM and get a receiver for events
    /// Returns (request_id, event_receiver)
    pub fn request(
        &self,
        model: String,
        content: LiveInputContent,
        instruction: String,
        show_thinking: bool,
    ) -> (u64, mpsc::Receiver<LiveEvent>) {
        let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let current_gen = self.interrupt_generation.load(Ordering::SeqCst);

        let (tx, rx) = mpsc::channel();

        let req = LiveRequest {
            model,
            content,
            instruction,
            show_thinking,
        };

        {
            let mut queue = self.work_queue.lock().unwrap();
            queue.push_back(QueuedLiveRequest {
                req,
                generation: current_gen,
                response_tx: tx,
            });
        }
        self.work_signal.notify_one();

        (id, rx)
    }

    /// Check if a request's generation is still valid
    pub fn is_generation_valid(&self, generation: u64) -> bool {
        generation >= self.interrupt_generation.load(Ordering::SeqCst)
    }
}

impl Default for GeminiLiveManager {
    fn default() -> Self {
        Self::new()
    }
}
