//! Pitch-preserving PCM float time stretch for export audio.
//!
//! Uses a small WSOLA-style overlap-add pass:
//! - analyze the source in short overlapping windows
//! - search locally for the best alignment to the previous window tail
//! - overlap-add with a Hann window
//!
//! This changes duration without the chipmunk pitch shift that plain resampling
//! produces.

fn pcm_le_bytes_to_f32(input: &[u8]) -> Option<Vec<f32>> {
    if input.is_empty() || !input.len().is_multiple_of(4) {
        return None;
    }

    let mut output = vec![0.0f32; input.len() / 4];
    unsafe {
        std::ptr::copy_nonoverlapping(
            input.as_ptr(),
            output.as_mut_ptr() as *mut u8,
            input.len(),
        );
    }
    Some(output)
}

fn f32_to_pcm_le_bytes(input: &[f32]) -> Vec<u8> {
    let mut output = vec![0u8; input.len() * 4];
    unsafe {
        std::ptr::copy_nonoverlapping(
            input.as_ptr() as *const u8,
            output.as_mut_ptr(),
            output.len(),
        );
    }
    output
}

fn hann_window(size: usize) -> Vec<f32> {
    if size <= 1 {
        return vec![1.0; size.max(1)];
    }
    let denom = (size - 1) as f32;
    (0..size)
        .map(|i| {
            let phase = (i as f32) / denom;
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * phase).cos()
        })
        .collect()
}

fn sample_windowed_frame(
    pcm: &[f32],
    channels: usize,
    start_frame: usize,
    frame_size: usize,
    window: &[f32],
) -> Vec<f32> {
    let mut buf = vec![0.0f32; frame_size * channels];
    for (frame, &w) in window.iter().enumerate().take(frame_size) {
        let src = (start_frame + frame) * channels;
        let dst = frame * channels;
        for ch in 0..channels {
            buf[dst + ch] = pcm[src + ch] * w;
        }
    }
    buf
}

struct StretchSearch<'a> {
    pcm: &'a [f32],
    channels: usize,
    in_frames: usize,
    frame_size: usize,
    search_radius: usize,
    overlap: usize,
    window: &'a [f32],
}

fn choose_alignment(
    params: &StretchSearch<'_>,
    expected_start: usize,
    prev_overlap: Option<&[f32]>,
) -> usize {
    let max_start = params.in_frames.saturating_sub(params.frame_size);
    let clamped_expected = expected_start.min(max_start);
    let Some(reference) = prev_overlap else {
        return clamped_expected;
    };

    let search_start = clamped_expected.saturating_sub(params.search_radius);
    let search_end = clamped_expected
        .saturating_add(params.search_radius)
        .min(max_start);
    let mut best_start = clamped_expected;
    let mut best_score = f64::INFINITY;

    for candidate in search_start..=search_end {
        let mut score = 0.0f64;
        for (frame, &w) in params.window.iter().enumerate().take(params.overlap) {
            let w = w as f64;
            let ref_idx = frame * params.channels;
            let cand_idx = (candidate + frame) * params.channels;
            for ch in 0..params.channels {
                let diff = reference[ref_idx + ch] as f64
                    - (params.pcm[cand_idx + ch] as f64 * w);
                score += diff * diff;
            }
        }
        if score < best_score {
            best_score = score;
            best_start = candidate;
        }
    }

    best_start
}

fn normalize_output(output: &mut [f32], weights: &[f32]) {
    for (sample, weight) in output.iter_mut().zip(weights.iter()) {
        if *weight > 1e-6 {
            *sample /= *weight;
        } else {
            *sample = 0.0;
        }
    }
}

fn stretch_pcm_f32(
    pcm: &[f32],
    speed: f64,
    sample_rate: u32,
    channels: usize,
) -> Option<Vec<f32>> {
    if pcm.is_empty() || channels == 0 || speed <= 0.0 || !speed.is_finite() {
        return None;
    }

    let in_frames = pcm.len() / channels;
    if in_frames < 2 {
        return None;
    }
    if (speed - 1.0).abs() < 0.001 {
        return Some(pcm.to_vec());
    }

    let mut frame_size = ((sample_rate as f64 * 0.025).round() as usize).clamp(256, 2048);
    frame_size = frame_size.min(in_frames.max(2));
    if frame_size < 8 {
        return None;
    }
    if frame_size.is_multiple_of(2) {
        frame_size = frame_size.max(2);
    } else {
        frame_size = frame_size.saturating_sub(1).max(2);
    }
    let overlap = (frame_size / 2).max(1);
    let hop_out = frame_size - overlap;
    if hop_out == 0 {
        return None;
    }

    let hop_in = ((hop_out as f64) * speed).round().max(1.0) as usize;
    let search_radius = (overlap / 2).max(1);
    let window = hann_window(frame_size);
    let search = StretchSearch {
        pcm,
        channels,
        in_frames,
        frame_size,
        search_radius,
        overlap,
        window: &window,
    };

    let expected_out_frames = ((in_frames as f64) / speed).round().max(1.0) as usize;
    let mut output = vec![0.0f32; (expected_out_frames + frame_size + hop_out) * channels];
    let mut weights = vec![0.0f32; output.len()];

    let mut prev_overlap: Option<Vec<f32>> = None;
    let mut segment_idx = 0usize;
    let mut expected_start = 0usize;

    loop {
        if expected_start > in_frames.saturating_sub(frame_size) {
            expected_start = in_frames.saturating_sub(frame_size);
        }

        let start = choose_alignment(&search, expected_start, prev_overlap.as_deref());

        let out_start = segment_idx * hop_out;
        let end_frame = out_start + frame_size;
        if end_frame * channels >= output.len() {
            let new_frames = end_frame + hop_out;
            output.resize(new_frames * channels, 0.0);
            weights.resize(new_frames * channels, 0.0);
        }

        let windowed = sample_windowed_frame(pcm, channels, start, frame_size, &window);
        for (frame, &w) in window.iter().enumerate().take(frame_size) {
            let dst_frame = out_start + frame;
            let dst = dst_frame * channels;
            let src = frame * channels;
            for ch in 0..channels {
                output[dst_frame * channels + ch] += windowed[src + ch];
                weights[dst + ch] += w;
            }
        }

        prev_overlap = Some(
            windowed[(frame_size - overlap) * channels..frame_size * channels].to_vec(),
        );

        segment_idx += 1;
        if start >= in_frames.saturating_sub(frame_size) {
            break;
        }
        expected_start = expected_start.saturating_add(hop_in);
        if expected_start >= in_frames.saturating_sub(1) {
            break;
        }
    }

    normalize_output(&mut output, &weights);
    let target_frames = expected_out_frames.min(output.len() / channels);
    output.truncate(target_frames * channels);
    Some(output)
}

pub(super) fn time_stretch_pcm_bytes(
    input: &[u8],
    speed: f64,
    sample_rate: u32,
    channels: usize,
) -> Vec<u8> {
    let Some(pcm_f32) = pcm_le_bytes_to_f32(input) else {
        return input.to_vec();
    };
    let Some(output_f32) = stretch_pcm_f32(&pcm_f32, speed, sample_rate, channels) else {
        return input.to_vec();
    };
    f32_to_pcm_le_bytes(&output_f32)
}
