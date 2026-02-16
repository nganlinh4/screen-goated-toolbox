use std::f32;

/// Simple OLA (Overlap-Add) time stretcher for pitch-preserving tempo change.
/// Uses Hann window for perfect reconstruction at 50% overlap.
pub struct WsolaStretcher {
    /// Frame size in samples (20ms at 24kHz = 480 samples)
    frame_size: usize,
    /// Hop size (frame_size / 2 for 50% overlap)
    hop_size: usize,
    /// Hann window
    window: Vec<f32>,
    /// Input buffer for accumulating samples
    pub input_buffer: Vec<f32>,
    /// Output overlap buffer - carries the "tail" that needs to overlap with next chunk
    output_overlap: Vec<f32>,
    /// Search range for alignment (SOLA)
    search_range: usize,
    /// Previous speed ratio (to detect changes)
    last_speed: f64,
}

impl WsolaStretcher {
    pub fn new(sample_rate: u32) -> Self {
        // 20ms frame size for better streaming with small chunks
        // At 24kHz: 20ms = 480 samples
        let frame_size = (sample_rate as usize * 20) / 1000;
        let hop_size = frame_size / 2; // 50% overlap

        // Create Hann window - with 50% overlap, Hann windows sum to exactly 1.0
        // This is crucial for artifact-free overlap-add!
        let window: Vec<f32> = (0..frame_size)
            .map(|i| {
                let t = i as f32 / frame_size as f32;
                // Hann window: 0.5 * (1 - cos(2*pi*t))
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * t).cos())
            })
            .collect();

        Self {
            frame_size,
            hop_size,
            window,
            input_buffer: Vec::new(),
            output_overlap: Vec::new(),
            search_range: hop_size / 2, // Search +/- 50% of hop size
            last_speed: 1.0,
        }
    }

    /// Find best offset using cross-correlation
    fn find_best_offset(&self, input_pos: usize, target_hop: usize) -> usize {
        // Strategy: We want to overlap the END of the previous frame (which is in output buffer)
        // with the BEGINNING of the new frame.
        // We look for self-similarity in the input signal around the analysis hop.

        // Search range: [target_hop - search_range, target_hop + search_range]
        let start = target_hop.saturating_sub(self.search_range);
        let end = (target_hop + self.search_range).min(
            self.input_buffer
                .len()
                .saturating_sub(self.frame_size + input_pos)
                .saturating_sub(1),
        );

        if start >= end {
            return target_hop;
        }

        let mut best_offset = target_hop;
        let mut max_corr = -1.0;

        // Use a subset of samples for correlation to save CPU
        let compare_len = self.search_range;

        let ref_pos = input_pos + self.hop_size;
        if ref_pos + compare_len > self.input_buffer.len() {
            return target_hop;
        }

        let ref_segment = &self.input_buffer[ref_pos..ref_pos + compare_len];

        for k in start..end {
            let candidate_pos = input_pos + k;
            if candidate_pos + compare_len > self.input_buffer.len() {
                continue;
            }

            let candidate = &self.input_buffer[candidate_pos..candidate_pos + compare_len];

            // Cross-correlation
            let mut corr = 0.0;
            for i in 0..compare_len {
                corr += ref_segment[i] * candidate[i];
            }

            if corr > max_corr {
                max_corr = corr;
                best_offset = k;
            }
        }

        best_offset
    }

    /// Time-stretch the input samples.
    /// speed_ratio > 1.0 = faster (compress time), < 1.0 = slower (expand time)
    pub fn stretch(&mut self, input: &[i16], speed_ratio: f64) -> Vec<i16> {
        // Bypass for normal speed
        if (speed_ratio - 1.0).abs() < 0.05 || input.is_empty() {
            // Flush any remaining overlap buffer
            if !self.output_overlap.is_empty() {
                let result: Vec<i16> = self
                    .output_overlap
                    .drain(..)
                    .map(|s| s.clamp(-32768.0, 32767.0) as i16)
                    .collect();
                // Also return the input
                let mut combined = result;
                combined.extend(input.iter().cloned());
                return combined;
            }
            return input.to_vec();
        }

        // Clear buffers if speed changed significantly (avoid artifacts)
        if (speed_ratio - self.last_speed).abs() > 0.15 {
            self.input_buffer.clear();
            self.output_overlap.clear();
        }
        self.last_speed = speed_ratio;

        // Add input samples to buffer (convert to f32)
        self.input_buffer.extend(input.iter().map(|&s| s as f32));

        // Need at least one frame + search range to process
        if self.input_buffer.len() < self.frame_size + self.search_range {
            return Vec::new();
        }

        // Ideal analysis hop
        let target_analysis_hop = (self.hop_size as f64 * speed_ratio).round() as usize;

        // Synthesis hop stays constant at 50% of frame size
        let synthesis_hop = self.hop_size;

        // Output buffer
        // We guess size based on target ratio, but it will vary slightly due to SOLA
        let estimated_frames = self.input_buffer.len() / target_analysis_hop.max(1);
        let mut output = vec![0.0f32; estimated_frames * synthesis_hop + self.frame_size];

        // Initialize output with overlap from previous call
        for (i, &v) in self.output_overlap.iter().enumerate() {
            if i < output.len() {
                output[i] = v;
            }
        }

        let mut input_pos = 0usize;
        let mut output_pos = 0usize;

        loop {
            // Ensure we have enough input for:
            // 1. Comparison (at current pos + hop_size)
            // 2. Next frame (at current pos + target_hop + search_range)
            if input_pos + self.frame_size + self.search_range + target_analysis_hop
                > self.input_buffer.len()
            {
                break;
            }
            if output_pos + self.frame_size > output.len() {
                output.resize(output_pos + self.frame_size * 2, 0.0);
            }

            // Find best alignment offset (SOLA)
            let actual_analysis_hop = self.find_best_offset(input_pos, target_analysis_hop);

            // Advance input by the OPTIMIZED hop
            input_pos += actual_analysis_hop;

            // Apply window and overlap-add
            for i in 0..self.frame_size {
                let in_sample = self.input_buffer[input_pos + i];
                let w = self.window[i];
                output[output_pos + i] += in_sample * w;
            }

            output_pos += synthesis_hop;
        }

        // The "complete" output is everything up to the start of the last frame's tail
        let complete_len = output_pos.min(output.len());

        // Save the tail for next call's overlap
        self.output_overlap.clear();
        if complete_len < output.len() {
            self.output_overlap
                .extend_from_slice(&output[complete_len..]);
        }

        // Remove consumed input
        let consumed = input_pos.min(self.input_buffer.len());

        if consumed > 0 {
            self.input_buffer.drain(0..consumed);
        }

        // Return the complete portion as i16
        output[..complete_len]
            .iter()
            .map(|&s| s.clamp(-32768.0, 32767.0) as i16)
            .collect()
    }
}
