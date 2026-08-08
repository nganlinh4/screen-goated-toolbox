//! Time-varying pitch-preserving time stretch (WSOLA).
//!
//! Overlap-add resampling changes duration by changing playback rate, which shifts
//! pitch. WSOLA instead keeps the sample rate and slides a fixed-size analysis
//! window along the source at a variable rate, overlap-adding the grains at a fixed
//! synthesis rate. Pitch is preserved because the grains themselves are never
//! resampled; only how often they are drawn from the source changes.
//!
//! The "waveform similarity" part is what keeps it from warbling: each grain is
//! nudged within a small search window so it lines up in phase with what the
//! previous grain would naturally have continued into.
//!
//! The caller drives the analysis position, one absolute frame index per hop, so the
//! stretch factor can change every hop and stays exactly consistent with the export
//! time map. FFmpeg's `atempo` cannot do this: it applies one tempo, and its runtime
//! `tempo` command does not track a dense command stream.

/// Grain length. ~43ms at 48kHz: long enough to carry low-frequency content, short
/// enough that the tempo can change inside a speed ramp without smearing.
const FRAME_LEN: usize = 2048;

/// 50% overlap. A periodic Hann window at this hop sums to exactly 1.0, so
/// overlap-add reconstructs unity gain without a correction factor.
const SYNTHESIS_HOP: usize = FRAME_LEN / 2;

/// How far a grain may slide to find phase alignment. ~13ms covers one period down
/// to ~75Hz, below any speaking voice.
const SEARCH_RADIUS: i64 = 640;

/// Samples compared when scoring an alignment.
const CORRELATION_LEN: usize = 640;

/// The search runs coarse-then-fine: every 4th offset, then every offset within 3
/// of the winner. Full-resolution search over the whole radius costs ~8x more for
/// an alignment that is audibly identical.
const COARSE_STEP: i64 = 4;
const FINE_RADIUS: i64 = 3;

/// A reported position further than this from the running sample count means the
/// caller seeked rather than the decoder rounding a timestamp. Half a second is far
/// beyond any codec's timestamp jitter and far below any real seek.
const SEEK_JUMP_FRAMES: i64 = 24_000;

pub(super) struct Wsola {
    channels: usize,
    window: Vec<f32>,
    /// Interleaved input; `input[0]` is absolute source frame `input_start`.
    input: Vec<f32>,
    /// Mono downmix of `input`, kept in step. The alignment search runs on this so
    /// a stereo pair always slides together and the image cannot wander.
    mono: Vec<f32>,
    input_start: i64,
    /// Where the previous grain would have continued, used to score alignments.
    reference: Vec<f32>,
    /// Overlap-add accumulator, `FRAME_LEN` frames wide.
    accum: Vec<f32>,
    /// Where the previous grain was actually taken from, so an exactly-unity hop can
    /// skip the search and reconstruct the input bit-for-bit.
    last_grain_start: Option<i64>,
    ended: bool,
}

impl Wsola {
    pub(super) fn new(channels: usize) -> Self {
        let window = (0..FRAME_LEN)
            .map(|index| {
                let phase = std::f64::consts::TAU * index as f64 / FRAME_LEN as f64;
                (0.5 - 0.5 * phase.cos()) as f32
            })
            .collect();
        Self {
            channels,
            window,
            input: Vec::new(),
            mono: Vec::new(),
            input_start: 0,
            reference: Vec::new(),
            accum: vec![0.0; FRAME_LEN * channels],
            last_grain_start: None,
            ended: false,
        }
    }

    /// Frames the caller must have pushed before a hop at `analysis_frame` can run.
    pub(super) fn required_input_end(analysis_frame: i64) -> i64 {
        analysis_frame + SEARCH_RADIUS + FRAME_LEN as i64
    }

    pub(super) fn output_hop_frames() -> usize {
        SYNTHESIS_HOP
    }

    /// Absolute frame index one past the last buffered input frame.
    pub(super) fn buffered_end(&self) -> i64 {
        self.input_end()
    }

    fn input_end(&self) -> i64 {
        self.input_start + (self.input.len() / self.channels) as i64
    }

    /// Appends interleaved frames.
    ///
    /// `start_frame` is the decoder's idea of where this data sits, which drifts a
    /// frame or two from the running sample count because timestamps get rounded.
    /// Sample counts are the truth for a continuous stream, so small disagreements
    /// are ignored and the data is simply appended. Only a jump big enough to mean a
    /// real seek restarts the buffer — treating jitter as a seek would throw away
    /// data the read position still needs and strand the rest of the source.
    pub(super) fn push(&mut self, pcm: &[f32], start_frame: i64) {
        if self.input.is_empty() {
            self.input_start = start_frame;
        } else if (start_frame - self.input_end()).abs() > SEEK_JUMP_FRAMES {
            self.input.clear();
            self.mono.clear();
            self.input_start = start_frame;
            self.reference.clear();
            self.last_grain_start = None;
        }
        self.input.extend_from_slice(pcm);
        for frame in pcm.chunks_exact(self.channels) {
            self.mono
                .push(frame.iter().sum::<f32>() / self.channels as f32);
        }
    }

    /// Marks the input complete. Pads with silence so the final grains can still be
    /// read whole instead of being dropped.
    pub(super) fn end_input(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        let pad = FRAME_LEN + SEARCH_RADIUS as usize;
        self.input
            .extend(std::iter::repeat_n(0.0, pad * self.channels));
        self.mono.extend(std::iter::repeat_n(0.0, pad));
    }

    fn local(&self, frame: i64) -> usize {
        (frame - self.input_start).max(0) as usize
    }

    /// Best alignment offset for a grain starting near `analysis_frame`, scored by
    /// normalized cross-correlation against the previous grain's continuation.
    fn best_offset(&self, analysis_frame: i64) -> i64 {
        if self.reference.len() < CORRELATION_LEN {
            return 0;
        }
        // An exactly-unity hop already lines up with the previous grain: taking it
        // as-is makes overlap-add reconstruct the input exactly, where a search
        // would slide it and colour audio that was not meant to be stretched.
        if self.last_grain_start == Some(analysis_frame - SYNTHESIS_HOP as i64) {
            return 0;
        }
        let min_offset = (self.input_start - analysis_frame).max(-SEARCH_RADIUS);
        let max_offset =
            (self.input_end() - CORRELATION_LEN as i64 - analysis_frame).min(SEARCH_RADIUS);
        if min_offset > max_offset {
            return 0;
        }

        let score_at = |offset: i64| -> f32 {
            let base = self.local(analysis_frame + offset);
            let candidate = &self.mono[base..base + CORRELATION_LEN];
            let mut dot = 0.0f32;
            let mut energy = 0.0f32;
            for (reference, sample) in self.reference.iter().zip(candidate) {
                dot += reference * sample;
                energy += sample * sample;
            }
            dot / (energy.sqrt() + 1e-9)
        };

        let mut best = min_offset;
        let mut best_score = f32::NEG_INFINITY;
        let mut offset = min_offset;
        while offset <= max_offset {
            let score = score_at(offset);
            if score > best_score {
                best_score = score;
                best = offset;
            }
            offset += COARSE_STEP;
        }
        for offset in (best - FINE_RADIUS).max(min_offset)..=(best + FINE_RADIUS).min(max_offset) {
            let score = score_at(offset);
            if score > best_score {
                best_score = score;
                best = offset;
            }
        }
        best
    }

    /// Emits one synthesis hop, drawing its grain from near `analysis_frame`.
    /// Returns `false` when more input is needed first.
    pub(super) fn hop(&mut self, analysis_frame: i64, out: &mut Vec<f32>) -> bool {
        if Self::required_input_end(analysis_frame) > self.input_end() {
            return false;
        }
        // Fewer than a grain's worth buffered means there is nothing to emit yet.
        if self.input_end() - self.input_start < FRAME_LEN as i64 {
            return false;
        }
        let offset = self.best_offset(analysis_frame);
        // Clamping rather than bailing: if the read position ever falls outside the
        // buffered window the nearest available grain is wrong by milliseconds,
        // where refusing the hop would silence every remaining hop in the span.
        let grain_start = (analysis_frame + offset)
            .max(self.input_start)
            .min(self.input_end() - FRAME_LEN as i64);

        let base = self.local(grain_start);
        for index in 0..FRAME_LEN {
            let gain = self.window[index];
            let source = (base + index) * self.channels;
            let target = index * self.channels;
            for channel in 0..self.channels {
                self.accum[target + channel] += self.input[source + channel] * gain;
            }
        }

        self.last_grain_start = Some(grain_start);
        let reference_start = self.local(grain_start + SYNTHESIS_HOP as i64);
        self.reference.clear();
        self.reference
            .extend_from_slice(&self.mono[reference_start..reference_start + CORRELATION_LEN]);

        let drained = SYNTHESIS_HOP * self.channels;
        out.extend_from_slice(&self.accum[..drained]);
        self.accum.copy_within(drained.., 0);
        let tail = self.accum.len() - drained;
        self.accum[tail..].fill(0.0);

        self.discard_before(analysis_frame - SEARCH_RADIUS);
        true
    }

    /// Drops input the search can no longer reach, so a long source does not pin the
    /// whole decode in memory.
    fn discard_before(&mut self, frame: i64) {
        let drop_frames = self.local(frame).min(self.mono.len());
        if drop_frames < FRAME_LEN {
            return;
        }
        self.input.drain(..drop_frames * self.channels);
        self.mono.drain(..drop_frames);
        self.input_start += drop_frames as i64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    fn sine(frames: usize, frequency: f64, channels: usize) -> Vec<f32> {
        let mut pcm = Vec::with_capacity(frames * channels);
        for index in 0..frames {
            let value =
                (std::f64::consts::TAU * frequency * index as f64 / RATE).sin() as f32 * 0.5;
            for _ in 0..channels {
                pcm.push(value);
            }
        }
        pcm
    }

    /// Runs a whole buffer through at a fixed tempo and returns interleaved output.
    fn stretch(pcm: &[f32], channels: usize, tempo: f64) -> Vec<f32> {
        let mut wsola = Wsola::new(channels);
        wsola.push(pcm, 0);
        wsola.end_input();
        let total_frames = (pcm.len() / channels) as f64;
        let mut out = Vec::new();
        let mut hop_index = 0usize;
        loop {
            let analysis = (hop_index as f64 * Wsola::output_hop_frames() as f64 * tempo).round();
            if analysis + FRAME_LEN as f64 > total_frames {
                break;
            }
            if !wsola.hop(analysis as i64, &mut out) {
                break;
            }
            hop_index += 1;
        }
        out
    }

    /// Zero crossings per second — a direct read of pitch that does not care about
    /// duration, which is exactly the property under test.
    fn zero_crossings_per_second(pcm: &[f32], channels: usize) -> f64 {
        let frames: Vec<f32> = pcm.chunks_exact(channels).map(|frame| frame[0]).collect();
        let crossings = frames
            .windows(2)
            .filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0))
            .count();
        crossings as f64 / (frames.len() as f64 / RATE)
    }

    #[test]
    fn unity_tempo_reconstructs_the_input() {
        let pcm = sine(48_000, 440.0, 2);
        let out = stretch(&pcm, 2, 1.0);

        assert!(!out.is_empty());
        // Overlap-add starts one grain behind, so compare the settled interior.
        let skip = FRAME_LEN * 2;
        let compared = out.len().min(pcm.len()) - skip;
        let error: f64 = out[skip..skip + compared]
            .iter()
            .zip(&pcm[skip..skip + compared])
            .map(|(got, want)| ((got - want) as f64).powi(2))
            .sum::<f64>()
            / compared as f64;
        assert!(
            error.sqrt() < 1e-5,
            "unity stretch must round-trip exactly, rms error {}",
            error.sqrt()
        );
    }

    #[test]
    fn speeding_up_shortens_without_raising_pitch() {
        let pcm = sine(96_000, 440.0, 2);
        let out = stretch(&pcm, 2, 2.0);

        let in_frames = 96_000.0;
        let out_frames = (out.len() / 2) as f64;
        assert!(
            (out_frames / in_frames - 0.5).abs() < 0.05,
            "2x should halve the length, got ratio {}",
            out_frames / in_frames
        );
        let pitch = zero_crossings_per_second(&out, 2);
        assert!(
            (pitch - 880.0).abs() < 40.0,
            "440Hz must stay 440Hz (880 crossings/s), got {pitch}"
        );
    }

    #[test]
    fn slowing_down_lengthens_without_lowering_pitch() {
        let pcm = sine(48_000, 440.0, 2);
        let out = stretch(&pcm, 2, 0.5);

        let out_frames = (out.len() / 2) as f64;
        assert!(
            (out_frames / 48_000.0 - 2.0).abs() < 0.1,
            "0.5x should double the length, got ratio {}",
            out_frames / 48_000.0
        );
        let pitch = zero_crossings_per_second(&out, 2);
        assert!(
            (pitch - 880.0).abs() < 40.0,
            "440Hz must stay 440Hz (880 crossings/s), got {pitch}"
        );
    }

    /// The regression the whole change is about: a resampler would sweep the pitch
    /// up with the tempo. Pitch must hold across a 1x -> 4x ramp.
    #[test]
    fn pitch_holds_across_a_tempo_ramp() {
        let pcm = sine(192_000, 440.0, 2);
        let mut wsola = Wsola::new(2);
        wsola.push(&pcm, 0);
        wsola.end_input();

        let mut out = Vec::new();
        let mut analysis = 0.0f64;
        let total = 192_000.0;
        while analysis + FRAME_LEN as f64 <= total {
            let tempo = 1.0 + 3.0 * (analysis / total);
            if !wsola.hop(analysis.round() as i64, &mut out) {
                break;
            }
            analysis += Wsola::output_hop_frames() as f64 * tempo;
        }

        let pitch = zero_crossings_per_second(&out, 2);
        assert!(
            (pitch - 880.0).abs() < 40.0,
            "pitch must not follow the ramp, got {pitch} crossings/s"
        );
    }

    #[test]
    fn mono_sources_are_supported() {
        let pcm = sine(48_000, 440.0, 1);
        let out = stretch(&pcm, 1, 2.0);
        let pitch = zero_crossings_per_second(&out, 1);
        assert!((pitch - 880.0).abs() < 40.0, "mono pitch {pitch}");
    }

    /// Media Foundation reports each chunk's position from a rounded timestamp, so
    /// it drifts a frame or two either side of the running sample count. Treating
    /// that as a seek used to discard the buffer mid-read, which stranded every
    /// remaining hop of the source and left the export silent from that point on.
    #[test]
    fn decoder_timestamp_jitter_does_not_strand_the_source() {
        let pcm = sine(96_000, 440.0, 2);
        let mut wsola = Wsola::new(2);

        // Feed in decoder-sized chunks whose reported position wobbles by +/-2
        // frames, the way a real AAC decoder's timestamps do.
        let chunk_frames = 1115usize;
        let mut frame = 0usize;
        let mut wobble = 0i64;
        while frame < 96_000 {
            let end = (frame + chunk_frames).min(96_000);
            wobble = match wobble {
                0 => 2,
                2 => -1,
                -1 => 1,
                _ => 0,
            };
            wsola.push(&pcm[frame * 2..end * 2], frame as i64 + wobble);
            frame = end;
        }
        wsola.end_input();

        let mut out = Vec::new();
        let mut hops = 0usize;
        while wsola.hop((hops * Wsola::output_hop_frames()) as i64, &mut out) {
            hops += 1;
        }

        // Unity tempo over 96k frames is ~93 hops; anything far short means the
        // buffer was thrown away partway through.
        assert!(
            hops > 85,
            "jittered timestamps stranded the source after {hops} hops"
        );
        let pitch = zero_crossings_per_second(&out, 2);
        assert!((pitch - 880.0).abs() < 40.0, "pitch {pitch}");
    }

    #[test]
    fn long_input_is_not_pinned_in_memory() {
        let pcm = sine(480_000, 440.0, 2);
        let mut wsola = Wsola::new(2);
        wsola.push(&pcm, 0);
        wsola.end_input();
        let mut out = Vec::new();
        let mut hop_index = 0i64;
        while wsola.hop(hop_index * Wsola::output_hop_frames() as i64, &mut out) {
            hop_index += 1;
        }
        assert!(
            wsola.input.len() < 480_000,
            "consumed input should be released, {} samples held",
            wsola.input.len()
        );
    }
}
