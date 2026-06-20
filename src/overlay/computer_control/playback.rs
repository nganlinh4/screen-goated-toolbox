//! Minimal output sink for the model's spoken replies (24 kHz mono PCM16).
//! Resamples to the default output device's rate and plays it back. Kept
//! self-contained (not coupled to `TTS_MANAGER`) so barge-in can simply clear
//! the queue.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Gemini Live output audio is PCM16 mono at 24 kHz.
const MODEL_RATE: u32 = 24_000;

pub(super) struct AudioSink {
    queue: Arc<Mutex<VecDeque<f32>>>,
    target_rate: u32,
    channels: usize,
    _stream: cpal::Stream,
}

impl AudioSink {
    pub(super) fn new() -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let target_rate = config.sample_rate;
        let channels = config.channels as usize;

        let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let q = queue.clone();
                device
                    .build_output_stream(
                        config,
                        move |data: &mut [f32], _: &_| {
                            let mut q = q.lock().unwrap();
                            for x in data.iter_mut() {
                                *x = q.pop_front().unwrap_or(0.0);
                            }
                        },
                        |e| eprintln!("[cc] audio output error: {e}"),
                        None,
                    )
                    .ok()?
            }
            cpal::SampleFormat::I16 => {
                let q = queue.clone();
                device
                    .build_output_stream(
                        config,
                        move |data: &mut [i16], _: &_| {
                            let mut q = q.lock().unwrap();
                            for x in data.iter_mut() {
                                let f = q.pop_front().unwrap_or(0.0);
                                *x = (f.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                            }
                        },
                        |e| eprintln!("[cc] audio output error: {e}"),
                        None,
                    )
                    .ok()?
            }
            _ => return None,
        };
        stream.play().ok()?;

        Some(Self {
            queue,
            target_rate,
            channels,
            _stream: stream,
        })
    }

    /// Queue a chunk of 24 kHz mono PCM16 for playback.
    pub(super) fn push(&self, pcm_24k: &[i16]) {
        if pcm_24k.is_empty() {
            return;
        }
        let resampled = if self.target_rate == MODEL_RATE {
            pcm_24k.to_vec()
        } else {
            crate::api::audio::resample_linear_i16(
                pcm_24k,
                self.target_rate as f64 / MODEL_RATE as f64,
            )
        };
        if let Ok(mut q) = self.queue.lock() {
            for &s in &resampled {
                let f = s as f32 / i16::MAX as f32;
                for _ in 0..self.channels {
                    q.push_back(f);
                }
            }
        }
    }

    /// Drop all queued audio (barge-in / interruption).
    pub(super) fn clear(&self) {
        if let Ok(mut q) = self.queue.lock() {
            q.clear();
        }
    }

    /// True while there is still queued audio playing — used to gate the mic so
    /// the agent's own voice doesn't trip barge-in on itself.
    pub(super) fn is_playing(&self) -> bool {
        self.queue.lock().map(|q| !q.is_empty()).unwrap_or(false)
    }
}
