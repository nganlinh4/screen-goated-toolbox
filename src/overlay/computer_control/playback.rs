//! Minimal output sink for the model's spoken replies (24 kHz mono PCM16).
//! Resamples to the default output device's rate and plays it back. Kept
//! self-contained (not coupled to `TTS_MANAGER`) so barge-in can simply clear
//! the queue.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, StreamTrait};

/// Gemini Live output audio is PCM16 mono at 24 kHz.
const MODEL_RATE: u32 = 24_000;

pub(super) struct AudioSink {
    queue: Arc<Mutex<VecDeque<f32>>>,
    target_rate: u32,
    channels: usize,
    failed: Arc<AtomicBool>,
    played: Arc<AtomicU64>,
    _stream: cpal::Stream,
}

impl AudioSink {
    pub(super) fn new() -> Option<Self> {
        let device = crate::api::realtime_audio::concrete_default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let target_rate = config.sample_rate;
        let channels = config.channels as usize;

        let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let failed = Arc::new(AtomicBool::new(false));
        let played = Arc::new(AtomicU64::new(0));

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let q = queue.clone();
                let stream_failed = failed.clone();
                let played_samples = played.clone();
                device
                    .build_output_stream(
                        config,
                        move |data: &mut [f32], _: &_| {
                            let mut q = q.lock().unwrap();
                            for x in data.iter_mut() {
                                if let Some(sample) = q.pop_front() {
                                    *x = sample;
                                    played_samples.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    *x = 0.0;
                                }
                            }
                        },
                        move |e| {
                            stream_failed.store(true, Ordering::SeqCst);
                            eprintln!("[cc] audio output error: {e}");
                            super::telemetry::typed_error(
                                "ERR_AUDIO_OUTPUT_STREAM",
                                "speech",
                                "audio output stream reported an error",
                                serde_json::json!({"error": e.to_string()}),
                            );
                        },
                        None,
                    )
                    .ok()?
            }
            cpal::SampleFormat::I16 => {
                let q = queue.clone();
                let stream_failed = failed.clone();
                let played_samples = played.clone();
                device
                    .build_output_stream(
                        config,
                        move |data: &mut [i16], _: &_| {
                            let mut q = q.lock().unwrap();
                            for x in data.iter_mut() {
                                let f = if let Some(sample) = q.pop_front() {
                                    played_samples.fetch_add(1, Ordering::Relaxed);
                                    sample
                                } else {
                                    0.0
                                };
                                *x = (f.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                            }
                        },
                        move |e| {
                            stream_failed.store(true, Ordering::SeqCst);
                            eprintln!("[cc] audio output error: {e}");
                            super::telemetry::typed_error(
                                "ERR_AUDIO_OUTPUT_STREAM",
                                "speech",
                                "audio output stream reported an error",
                                serde_json::json!({"error": e.to_string()}),
                            );
                        },
                        None,
                    )
                    .ok()?
            }
            _ => return None,
        };
        stream.play().ok()?;

        super::telemetry::event(
            "audio_output_ready",
            "speech",
            super::telemetry::Privacy::Safe,
            serde_json::json!({
                "target_rate": target_rate,
                "channels": channels,
                "sample_format": format!("{sample_format:?}"),
            }),
        );

        Some(Self {
            queue,
            target_rate,
            channels,
            failed,
            played,
            _stream: stream,
        })
    }

    /// Queue a chunk of 24 kHz mono PCM16 for playback.
    pub(super) fn push(&self, pcm_24k: &[i16]) -> usize {
        if pcm_24k.is_empty() {
            return 0;
        }
        let resampled = if self.target_rate == MODEL_RATE {
            pcm_24k.to_vec()
        } else {
            crate::api::audio::resample_linear_i16(
                pcm_24k,
                self.target_rate as f64 / MODEL_RATE as f64,
            )
        };
        let mut queued = 0;
        if let Ok(mut q) = self.queue.lock() {
            for &s in &resampled {
                let f = s as f32 / i16::MAX as f32;
                for _ in 0..self.channels {
                    q.push_back(f);
                    queued += 1;
                }
            }
        }
        queued
    }

    /// Drop all queued audio (barge-in / interruption).
    pub(super) fn clear(&self) -> usize {
        if let Ok(mut q) = self.queue.lock() {
            let dropped = q.len();
            q.clear();
            dropped
        } else {
            0
        }
    }

    /// True while there is still queued audio playing — used to gate the mic so
    /// the agent's own voice doesn't trip barge-in on itself.
    pub(super) fn is_playing(&self) -> bool {
        self.queue.lock().map(|q| !q.is_empty()).unwrap_or(false)
    }

    pub(super) fn queued_samples(&self) -> usize {
        self.queue.lock().map(|queue| queue.len()).unwrap_or(0)
    }

    pub(super) fn played_samples(&self) -> u64 {
        self.played.load(Ordering::Relaxed)
    }

    pub(super) fn needs_rebuild(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }
}
