use std::time::Instant;

use super::output_vad::{OutputRegion, OutputVad};

const OUTPUT_SAMPLE_RATE: f64 = 24_000.0;

pub(super) struct TimelineAudioAppend {
    pub(super) audio_samples: usize,
    pub(super) silence_samples: usize,
    pub(super) regions: Vec<OutputRegion>,
}

pub(super) fn append_received_audio_on_clock(
    full_output: &mut Vec<i16>,
    vad: &mut OutputVad,
    output_clock: Instant,
    samples: &[i16],
) -> TimelineAudioAppend {
    let desired_len = (output_clock.elapsed().as_secs_f64() * OUTPUT_SAMPLE_RATE).round() as usize;
    let silence_samples = desired_len.saturating_sub(full_output.len());
    let mut timeline_samples = Vec::with_capacity(silence_samples + samples.len());
    if silence_samples > 0 {
        full_output.resize(desired_len, 0);
        timeline_samples.resize(silence_samples, 0);
    }
    full_output.extend_from_slice(samples);
    timeline_samples.extend_from_slice(samples);
    let regions = vad.push(&timeline_samples);
    TimelineAudioAppend {
        audio_samples: samples.len(),
        silence_samples,
        regions,
    }
}
