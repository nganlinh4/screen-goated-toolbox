// Media Foundation audio decode and AAC encode.
// Decodes source audio to PCM float; encodes PCM float to AAC via the SinkWriter.

mod decoder;
mod encoder;
#[path = "mf_audio_symphonia.rs"]
mod mf_audio_symphonia;
mod pcm;

pub use decoder::MfAudioDecoder;
pub use encoder::AudioStream;

/// Configuration for audio processing.
#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate_kbps: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            bitrate_kbps: 192,
        }
    }
}
