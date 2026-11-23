use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, AtomicU64},
};
use windows::Win32::Media::Audio::WAVEFORMATEX;

use super::device_failure::DeviceFailure;

pub(super) struct RenderState {
    pub shared_buffer: Arc<Mutex<VecDeque<i16>>>,
    pub device_padding_frames: Arc<AtomicU32>,
    pub shutdown: Arc<AtomicBool>,
    pub reset_generation: Arc<AtomicU64>,
    pub submitted_frames: Arc<AtomicU64>,
    pub played_frames: Arc<AtomicU64>,
    pub device_failure: Arc<DeviceFailure>,
}

pub(super) fn with_output_deck<T>(deck: &Mutex<VecDeque<i16>>, f: impl FnOnce() -> T) -> T {
    let _guard = deck.lock().unwrap();
    f()
}

pub(super) fn playback_format(sample_rate: u32) -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: 1,
        nChannels: 1,
        nSamplesPerSec: sample_rate,
        nAvgBytesPerSec: sample_rate * 2,
        nBlockAlign: 2,
        wBitsPerSample: 16,
        cbSize: 0,
    }
}
