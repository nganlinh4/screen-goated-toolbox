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
const STARTUP_BUFFER_MS: usize = 120;

#[derive(Default)]
struct StartupBuffer {
    samples: VecDeque<f32>,
    started: bool,
}

impl StartupBuffer {
    fn accept(&mut self, output: VecDeque<f32>, threshold: usize) -> Option<VecDeque<f32>> {
        if self.started {
            return Some(output);
        }
        self.samples.extend(output);
        if self.samples.len() < threshold {
            return None;
        }
        self.started = true;
        Some(self.samples.drain(..).collect())
    }

    fn finish(&mut self) -> VecDeque<f32> {
        self.started = false;
        self.samples.drain(..).collect()
    }
}

pub(super) struct AudioSink {
    queue: Arc<Mutex<VecDeque<f32>>>,
    startup: Mutex<StartupBuffer>,
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
            startup: Mutex::new(StartupBuffer::default()),
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
        let output = expand_channels(&resampled, self.channels);
        let threshold = self.target_rate as usize * self.channels * STARTUP_BUFFER_MS / 1000;
        let Ok(mut startup) = self.startup.lock() else {
            return 0;
        };
        startup
            .accept(output, threshold)
            .map(|ready| self.queue_output(ready))
            .unwrap_or(0)
    }

    /// Queue a complete utterance that was retained while no output sink was
    /// available, bypassing the streamed-audio startup buffer.
    pub(super) fn push_complete_utterance(&self, pcm_24k: &[i16]) -> usize {
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
        self.queue_output(expand_channels(&resampled, self.channels))
    }

    /// Flush a genuinely short streamed utterance and arm a fresh startup
    /// buffer for the next generation.
    pub(super) fn finish_utterance(&self) -> usize {
        let Ok(mut startup) = self.startup.lock() else {
            return 0;
        };
        let pending = startup.finish();
        self.queue_output(pending)
    }

    fn queue_output(&self, output: VecDeque<f32>) -> usize {
        let queued = output.len();
        if let Ok(mut queue) = self.queue.lock() {
            queue.extend(output);
            queued
        } else {
            0
        }
    }

    /// Drop all queued audio (barge-in / interruption).
    pub(super) fn clear(&self) -> usize {
        let staged = self
            .startup
            .lock()
            .map(|mut startup| {
                startup.started = false;
                let count = startup.samples.len();
                startup.samples.clear();
                count
            })
            .unwrap_or(0);
        let queued = self
            .queue
            .lock()
            .map(|mut queue| {
                let count = queue.len();
                queue.clear();
                count
            })
            .unwrap_or(0);
        staged + queued
    }

    /// True while there is still queued audio playing — used to gate the mic so
    /// the agent's own voice doesn't trip barge-in on itself.
    pub(super) fn is_playing(&self) -> bool {
        self.queue.lock().map(|q| !q.is_empty()).unwrap_or(false)
    }

    pub(super) fn queued_samples(&self) -> usize {
        let staged = self
            .startup
            .lock()
            .map(|startup| startup.samples.len())
            .unwrap_or(0);
        let queued = self.queue.lock().map(|queue| queue.len()).unwrap_or(0);
        queued + staged
    }

    pub(super) fn played_samples(&self) -> u64 {
        self.played.load(Ordering::Relaxed)
    }

    pub(super) fn needs_rebuild(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }
}

fn expand_channels(samples: &[i16], channels: usize) -> VecDeque<f32> {
    let mut output = VecDeque::with_capacity(samples.len() * channels);
    for &sample in samples {
        let value = sample as f32 / i16::MAX as f32;
        for _ in 0..channels {
            output.push_back(value);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::StartupBuffer;
    use std::collections::VecDeque;

    #[test]
    fn startup_buffer_does_not_release_a_bootstrap_fragment_alone() {
        let mut buffer = StartupBuffer::default();
        assert!(buffer.accept(VecDeque::from(vec![0.1]), 4).is_none());
        let ready = buffer
            .accept(VecDeque::from(vec![0.2, 0.3, 0.4]), 4)
            .expect("threshold reached");
        assert_eq!(ready.len(), 4);
    }

    #[test]
    fn completion_releases_a_genuinely_short_utterance_once() {
        let mut buffer = StartupBuffer::default();
        assert!(buffer.accept(VecDeque::from(vec![0.1, 0.2]), 4).is_none());
        assert_eq!(buffer.finish().len(), 2);
        assert!(buffer.finish().is_empty());
    }
}
