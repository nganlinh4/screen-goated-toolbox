use std::sync::{Arc, Mutex, atomic::AtomicBool, mpsc};

use super::{
    AdaptiveS2sVadState, S2sContextMemory, S2sContextSnapshot, S2sEvent, S2sRaceEvent, S2sSettings,
    Segment,
};

#[derive(Clone, Debug)]
pub struct S2sBatchSegment {
    pub id: u64,
    pub source_start_sec: f64,
    pub source_end_sec: f64,
    pub source_text: String,
    pub target_text: String,
    pub audio_pcm_24k: Vec<i16>,
}

#[derive(Clone)]
pub(super) struct TimedSegment {
    pub(super) segment: Segment,
    pub(super) start_sample: usize,
    pub(super) end_sample: usize,
}

#[derive(Clone)]
pub(super) struct S2sSessionResources {
    pub(super) event_tx: mpsc::Sender<S2sEvent>,
    pub(super) stop_signal: Arc<AtomicBool>,
    pub(super) settings: S2sSettings,
    pub(super) context_memory: Arc<Mutex<S2sContextMemory>>,
    pub(super) adaptive_vad: Arc<Mutex<AdaptiveS2sVadState>>,
}

pub(super) struct HedgedSegmentRequest {
    pub(super) session_index: usize,
    pub(super) generation: u64,
    pub(super) segment: Segment,
    pub(super) context: S2sContextSnapshot,
    pub(super) final_attempt: bool,
}

pub(super) struct HedgedAttemptRequest {
    pub(super) session_index: usize,
    pub(super) attempt: usize,
    pub(super) generation: u64,
    pub(super) segment: Segment,
    pub(super) context: S2sContextSnapshot,
    pub(super) final_attempt: bool,
}

pub(super) struct HedgedAttemptResources {
    pub(super) settings: S2sSettings,
    pub(super) stop_signal: Arc<AtomicBool>,
    pub(super) cancel_signal: Arc<AtomicBool>,
    pub(super) race_tx: mpsc::Sender<S2sRaceEvent>,
}

pub(super) struct ProcessSegmentParams<'a> {
    pub(super) mode: super::S2sMode,
    pub(super) session_index: usize,
    pub(super) generation: u64,
    pub(super) event_tx: &'a mpsc::Sender<S2sEvent>,
    pub(super) stop_signal: &'a Arc<AtomicBool>,
    pub(super) cancel_signal: Option<&'a Arc<AtomicBool>>,
    pub(super) final_attempt: bool,
}
