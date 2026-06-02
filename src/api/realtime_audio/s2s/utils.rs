use super::*;

pub(super) fn merge_segment_text(existing: &mut String, incoming: &str) {
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return;
    }
    if existing.trim().is_empty() || incoming.starts_with(existing.trim()) {
        existing.clear();
        existing.push_str(incoming);
        return;
    }
    if existing.trim_end().ends_with(incoming) {
        return;
    }

    let overlap = largest_suffix_prefix_overlap(existing.trim_end(), incoming);
    if overlap > 0 {
        existing.push_str(&incoming[overlap..]);
        return;
    }

    let needs_space = existing
        .chars()
        .last()
        .is_some_and(|ch| !ch.is_whitespace())
        && incoming.chars().next().is_some_and(|ch| {
            !ch.is_whitespace() && !matches!(ch, '.' | ',' | '?' | '!' | ';' | ':')
        });
    if needs_space {
        existing.push(' ');
    }
    existing.push_str(incoming);
}

pub(super) fn largest_suffix_prefix_overlap(existing: &str, incoming: &str) -> usize {
    let max = existing.len().min(incoming.len());
    incoming
        .char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(incoming.len()))
        .filter(|&len| {
            len > 0
                && len <= max
                && existing.ends_with(&incoming[..len])
                && is_meaningful_text_overlap(&incoming[..len])
        })
        .max()
        .unwrap_or(0)
}

pub(super) fn is_meaningful_text_overlap(overlap: &str) -> bool {
    overlap.chars().any(char::is_whitespace) || overlap.chars().count() >= MIN_TEXT_OVERLAP_CHARS
}

pub(super) fn calculate_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum = samples
        .iter()
        .map(|sample| {
            let normalized = *sample as f32 / i16::MAX as f32;
            normalized * normalized
        })
        .sum::<f32>();
    (sum / samples.len() as f32).sqrt()
}

pub(super) fn speech_threshold_for_noise(noise_floor: f32) -> f32 {
    (noise_floor * SPEECH_THRESHOLD_MULTIPLIER).clamp(MIN_SPEECH_THRESHOLD, MAX_SPEECH_THRESHOLD)
}

pub(super) fn update_noise_floor(
    noise_floor: f32,
    rms: f32,
    speech_threshold: f32,
    is_speech: bool,
) -> f32 {
    if is_speech {
        return noise_floor;
    }

    let learn_limit = NOISE_LEARN_MAX_RMS.min(speech_threshold * NOISE_LEARN_THRESHOLD_RATIO);
    if rms <= learn_limit {
        return (noise_floor * 0.98) + (rms * 0.02);
    }

    noise_floor.min(MAX_SPEECH_THRESHOLD / SPEECH_THRESHOLD_MULTIPLIER) * 0.995
}

pub(super) fn analyze_segment_samples(samples: &[i16]) -> SegmentSampleMetrics {
    if samples.is_empty() {
        return SegmentSampleMetrics::default();
    }

    let mut rms_sum = 0.0f32;
    let mut frame_count = 0usize;
    let mut energetic_frames = 0usize;
    let mut speech_like_frames = 0usize;
    let mut peak_rms = 0.0f32;
    for frame in samples.chunks(FRAME_SAMPLES) {
        frame_count += 1;
        let rms = calculate_rms(frame);
        peak_rms = peak_rms.max(rms);
        rms_sum += rms;
        if rms >= MIN_SPEECH_THRESHOLD {
            energetic_frames += 1;
        }
        if is_speech_like_frame(frame, rms) {
            speech_like_frames += 1;
        }
    }

    SegmentSampleMetrics {
        mean_rms: rms_sum / frame_count.max(1) as f32,
        peak_rms,
        energetic_frames,
        speech_like_frames,
    }
}

pub(super) fn is_speech_like_frame(frame: &[i16], rms: f32) -> bool {
    if frame.len() < 2 || rms < MIN_SPEECH_THRESHOLD {
        return false;
    }

    let peak = frame
        .iter()
        .map(|sample| (*sample as f32).abs() / i16::MAX as f32)
        .fold(0.0, f32::max);
    let crest = peak / rms.max(0.000_1);
    let zero_crossings = frame
        .windows(2)
        .filter(|pair| (pair[0] < 0 && pair[1] >= 0) || (pair[0] >= 0 && pair[1] < 0))
        .count();
    let zcr = zero_crossings as f32 / (frame.len() - 1) as f32;

    (0.015..=0.24).contains(&zcr) && (1.2..=18.0).contains(&crest)
}

pub(super) fn adaptive_vad_snapshot(
    adaptive_vad: &Arc<Mutex<AdaptiveS2sVadState>>,
) -> AdaptiveS2sVadSnapshot {
    adaptive_vad
        .lock()
        .map(|state| state.snapshot())
        .unwrap_or_default()
}

pub(super) fn observe_adaptive_vad(
    adaptive_vad: &Arc<Mutex<AdaptiveS2sVadState>>,
    outcome: SegmentOutcome,
    segment: &Segment,
) {
    if let Ok(mut state) = adaptive_vad.lock() {
        state.observe(outcome, segment);
    }
}

pub(super) fn log_adaptive_vad_skip(segment: &Segment, vad: AdaptiveS2sVadSnapshot) {
    eprintln!(
        "[RealtimeS2S][AdaptiveVAD] skip segment={} strictness={:.2} confidence={:.2} speech_like_ratio={:.2} speech_ratio={:.2} mean_rms={:.4} peak_rms={:.4}",
        segment.id,
        vad.strictness,
        segment_speech_confidence(segment),
        segment_speech_like_ratio(segment),
        segment_speech_ratio(segment),
        segment.mean_rms,
        segment.peak_rms
    );
}

pub(super) fn is_segment_worth_sending(segment: &Segment, vad: AdaptiveS2sVadSnapshot) -> bool {
    let speech_ratio = segment_speech_ratio(segment);
    let speech_like_ratio = segment_speech_like_ratio(segment);
    let confidence = segment_speech_confidence(segment);
    let baseline = segment.speech_frames >= MIN_SEGMENT_SPEECH_FRAMES
        || speech_ratio >= MIN_SEGMENT_SPEECH_RATIO
        || (segment.peak_rms >= MIN_SEGMENT_PEAK_RMS && speech_like_ratio >= 0.08);
    if !baseline {
        return false;
    }

    if vad.strictness <= 0.0 {
        return confidence >= 0.18 || speech_like_ratio >= 0.08;
    }

    let min_speech_like = MIN_SPEECH_LIKE_RATIO
        + (STRICT_MIN_SPEECH_LIKE_RATIO - MIN_SPEECH_LIKE_RATIO) * vad.strictness;
    let min_confidence = 0.24 + (STRICT_MIN_SPEECH_CONFIDENCE - 0.24) * vad.strictness;
    // A high blended confidence alone can come purely from the energy terms
    // (loud flat/tonal/DC noise), so require at least minimal speech-like
    // structure before accepting on confidence. The 0.08 floor matches the
    // baseline and lenient branches above.
    speech_like_ratio >= min_speech_like
        || (speech_like_ratio >= 0.08 && confidence >= min_confidence)
}

pub(super) fn segment_speech_ratio(segment: &Segment) -> f32 {
    let frame_count = segment.samples.len().div_ceil(FRAME_SAMPLES).max(1);
    segment.speech_frames as f32 / frame_count as f32
}

pub(super) fn segment_speech_like_ratio(segment: &Segment) -> f32 {
    let frame_count = segment.samples.len().div_ceil(FRAME_SAMPLES).max(1);
    segment.speech_like_frames as f32 / frame_count as f32
}

pub(super) fn segment_energetic_ratio(segment: &Segment) -> f32 {
    let frame_count = segment.samples.len().div_ceil(FRAME_SAMPLES).max(1);
    segment.energetic_frames as f32 / frame_count as f32
}

pub(super) fn segment_speech_confidence(segment: &Segment) -> f32 {
    let speech_ratio = segment_speech_ratio(segment);
    let speech_like_ratio = segment_speech_like_ratio(segment);
    let energetic_ratio = segment_energetic_ratio(segment);
    let energy_score = (segment.mean_rms / 0.055).clamp(0.0, 1.0);
    (speech_like_ratio * 0.45)
        + (speech_ratio * 0.30)
        + (energetic_ratio * 0.15)
        + (energy_score * 0.10)
}

pub(super) fn samples_to_ms(samples: usize) -> usize {
    samples.saturating_mul(1000) / 16_000
}

pub(super) fn pcm_bytes_to_i16(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect()
}

pub(super) fn speed_instruction(speed: &str) -> &'static str {
    match speed {
        "Slow" => "Speak slowly, clearly, and with deliberate pacing.",
        "Fast" => "Speak quickly, efficiently, and with a brisk pace.",
        _ => "Speak naturally and clearly.",
    }
}

pub(super) fn tts_instruction_for_target(
    target_language: &str,
    conditions: &[crate::config::TtsLanguageCondition],
) -> String {
    let target_code = language_to_639_3(target_language);
    conditions
        .iter()
        .find(|condition| {
            condition.language_code.eq_ignore_ascii_case(&target_code)
                || condition
                    .language_name
                    .eq_ignore_ascii_case(target_language.trim())
        })
        .map(|condition| condition.instruction.trim().to_string())
        .filter(|instruction| !instruction.is_empty())
        .unwrap_or_default()
}

pub(super) fn language_to_639_3(language: &str) -> String {
    let language = language.trim();
    if language.len() == 3 && isolang::Language::from_639_3(language).is_some() {
        return language.to_ascii_lowercase();
    }
    if language.len() == 2
        && let Some(lang) = isolang::Language::from_639_1(language)
    {
        return lang.to_639_3().to_string();
    }
    isolang::Language::from_name(language)
        .map(|lang| lang.to_639_3())
        .map(|code| code.to_string())
        .unwrap_or_else(|| language.to_ascii_lowercase())
}

pub(super) fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut output = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    output.push_str("...");
    output
}

pub(super) fn is_stale_session(session_id: u64) -> bool {
    crate::overlay::realtime_webview::state::REALTIME_SESSION_ID.load(Ordering::SeqCst)
        != session_id
}
