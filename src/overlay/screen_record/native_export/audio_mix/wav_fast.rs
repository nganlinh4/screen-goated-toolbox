//! Streaming WAV reader for the stretch mixer. Source frames are decoded on
//! demand and linearly resampled to [`MIX_OUTPUT_SAMPLE_RATE`] one chunk at a
//! time, so a long microphone track never sits in memory as a whole.

use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;

use super::{MIX_OUTPUT_CHANNELS, MIX_OUTPUT_SAMPLE_RATE};

/// Speed is sampled once per chunk, so the chunk length caps how closely a speed
/// ramp can be tracked. 2048 frames is ~43ms at 48kHz: fine enough that the residual
/// error inside a chunk stays inaudible, and each chunk is still placed at an exact
/// output time so nothing accumulates.
const WAV_FAST_CHUNK_FRAMES: usize = 2048;

/// A forward jump larger than this repositions the file instead of decoding
/// and discarding the frames in between.
const SEEK_INSTEAD_OF_READ_FRAMES: usize = 4 * WAV_FAST_CHUNK_FRAMES;

pub(super) struct DecodedAudioChunk {
    /// Interleaved f32, [`MIX_OUTPUT_CHANNELS`] channels at [`MIX_OUTPUT_SAMPLE_RATE`].
    pub(super) samples: Vec<f32>,
    pub(super) decoded_time: f64,
}

#[derive(Clone, Copy)]
enum SampleFormat {
    Float32,
    Int16,
    Int32 { full_scale: f32 },
}

pub(super) struct WavChunkStream {
    reader: hound::WavReader<BufReader<File>>,
    format: SampleFormat,
    channels: u16,
    source_rate: u32,
    source_frames: usize,
    output_frames: usize,
    next_output_frame: usize,
    /// Decoded source frames starting at absolute index `window_start`.
    window: VecDeque<[f32; 2]>,
    window_start: usize,
}

/// Opens `path` for streaming when it is a WAV layout the fast path handles.
pub(super) fn read_wav_fast_chunks(path: &str) -> Result<Option<WavChunkStream>, String> {
    if !path.to_ascii_lowercase().ends_with(".wav") {
        return Ok(None);
    }
    let reader = hound::WavReader::open(path).map_err(|e| format!("Open WAV fast path: {e}"))?;
    let spec = reader.spec();
    if spec.sample_rate == 0 || !(spec.channels == 1 || spec.channels == 2) {
        return Ok(None);
    }
    let format = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => SampleFormat::Float32,
        (hound::SampleFormat::Int, 16) => SampleFormat::Int16,
        (hound::SampleFormat::Int, 24) => SampleFormat::Int32 {
            full_scale: 8_388_608.0,
        },
        (hound::SampleFormat::Int, 32) => SampleFormat::Int32 {
            full_scale: 2_147_483_648.0,
        },
        _ => return Ok(None),
    };
    let source_frames = reader.duration() as usize;
    let output_frames = output_frame_count(source_frames, spec.sample_rate);
    Ok(Some(WavChunkStream {
        reader,
        format,
        channels: spec.channels,
        source_rate: spec.sample_rate,
        source_frames,
        output_frames,
        next_output_frame: 0,
        window: VecDeque::new(),
        window_start: 0,
    }))
}

fn output_frame_count(source_frames: usize, source_rate: u32) -> usize {
    if source_frames == 0 || source_rate == 0 {
        return 0;
    }
    ((source_frames as u64 * MIX_OUTPUT_SAMPLE_RATE as u64).saturating_add(source_rate as u64 - 1)
        / source_rate as u64) as usize
}

impl WavChunkStream {
    /// Drops every whole chunk that ends at or before `target_sec`; the stream
    /// only moves forward, like the stretcher's reads.
    pub(super) fn seek(&mut self, target_sec: f64) {
        while self.next_output_frame < self.output_frames {
            let chunk_end =
                (self.next_output_frame + WAV_FAST_CHUNK_FRAMES).min(self.output_frames);
            if chunk_end as f64 / MIX_OUTPUT_SAMPLE_RATE as f64 > target_sec {
                break;
            }
            self.next_output_frame = chunk_end;
        }
    }

    pub(super) fn next_chunk(&mut self) -> Result<Option<DecodedAudioChunk>, String> {
        let start = self.next_output_frame;
        if start >= self.output_frames {
            return Ok(None);
        }
        let end = (start + WAV_FAST_CHUNK_FRAMES).min(self.output_frames);
        let mut samples = Vec::with_capacity((end - start) * MIX_OUTPUT_CHANNELS as usize);
        let ratio = self.source_rate as f64 / MIX_OUTPUT_SAMPLE_RATE as f64;
        for output_frame in start..end {
            let source_pos = output_frame as f64 * ratio;
            let left_idx = source_pos.floor() as usize;
            let right_idx = (left_idx + 1).min(self.source_frames - 1);
            let t = (source_pos - left_idx as f64) as f32;
            let (left, right) = self.source_pair(left_idx, right_idx)?;
            samples.push(left[0] + (right[0] - left[0]) * t);
            samples.push(left[1] + (right[1] - left[1]) * t);
        }
        self.next_output_frame = end;
        Ok(Some(DecodedAudioChunk {
            samples,
            decoded_time: start as f64 / MIX_OUTPUT_SAMPLE_RATE as f64,
        }))
    }

    /// Source frames `left` and `right` (`left <= right`) as stereo. `left`
    /// never decreases between calls, so every frame before it is released.
    fn source_pair(&mut self, left: usize, right: usize) -> Result<([f32; 2], [f32; 2]), String> {
        if left < self.window_start {
            return Err("WAV fast path read moved backwards".to_string());
        }
        if left >= self.window_start + self.window.len() + SEEK_INSTEAD_OF_READ_FRAMES {
            self.reader
                .seek(left as u32)
                .map_err(|e| format!("Seek WAV fast path: {e}"))?;
            self.window.clear();
            self.window_start = left;
        }
        while self.window_start + self.window.len() <= right {
            let frame = self.read_frame()?;
            self.window.push_back(frame);
        }
        let release = left - self.window_start;
        self.window.drain(..release);
        self.window_start = left;
        Ok((self.window[0], self.window[right - left]))
    }

    fn read_frame(&mut self) -> Result<[f32; 2], String> {
        let first = self.read_sample()?;
        if self.channels == 1 {
            return Ok([first, first]);
        }
        Ok([first, self.read_sample()?])
    }

    fn read_sample(&mut self) -> Result<f32, String> {
        let sample = match self.format {
            SampleFormat::Float32 => self
                .reader
                .samples::<f32>()
                .next()
                .map(|sample| sample.map(|value| value.clamp(-1.0, 1.0))),
            SampleFormat::Int16 => self
                .reader
                .samples::<i16>()
                .next()
                .map(|sample| sample.map(|value| value as f32 / 32768.0)),
            SampleFormat::Int32 { full_scale } => self
                .reader
                .samples::<i32>()
                .next()
                .map(|sample| sample.map(|value| (value as f32 / full_scale).clamp(-1.0, 1.0))),
        };
        sample
            .ok_or_else(|| "WAV fast path ended before its declared length".to_string())?
            .map_err(|e| format!("Read WAV sample: {e}"))
    }
}

#[cfg(test)]
#[path = "wav_fast_tests.rs"]
mod tests;
