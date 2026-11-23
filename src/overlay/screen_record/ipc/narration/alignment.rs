use super::*;

#[derive(Clone)]
pub(super) struct CleanNarrationItem {
    pub(super) id: String,
    pub(super) text: String,
    pub(super) tts_text: String,
    pub(super) start_time: f64,
    pub(super) end_time: f64,
    pub(super) text_units: usize,
}

#[derive(Clone)]
pub(super) struct NarrationRequestGroup {
    pub(super) id: String,
    pub(super) items: Vec<CleanNarrationItem>,
    pub(super) text: String,
}

#[derive(Clone)]
pub(super) struct NarrationAlignedRange {
    pub(super) start_sec: f64,
    pub(super) end_sec: f64,
    pub(super) confidence: f64,
}

pub(super) struct NarrationSplitResult {
    pub(super) ranges: Vec<NarrationAlignedRange>,
    pub(super) mode: &'static str,
}

pub(super) fn estimate_narration_speech_units(text: &str) -> usize {
    let mut word_count = 0usize;
    let mut in_word = false;
    let mut alnum_chars = 0usize;
    let mut has_whitespace = false;
    for ch in text.nfc() {
        if ch.is_whitespace() {
            has_whitespace = true;
            if in_word {
                word_count += 1;
                in_word = false;
            }
            continue;
        }
        if ch.is_alphanumeric() {
            alnum_chars += 1;
            in_word = true;
        } else if in_word {
            word_count += 1;
            in_word = false;
        }
    }
    if in_word {
        word_count += 1;
    }
    if has_whitespace && word_count > 1 {
        word_count
    } else {
        alnum_chars.div_ceil(4).max(1)
    }
}

pub(super) fn normalize_group_sentence(text: &str) -> String {
    let mut trimmed = text.trim().trim_matches(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '"' | '\'' | '`' | '*' | '_' | '~' | '|' | '♪' | '♫' | '♩' | '♬'
            )
    });
    while trimmed
        .chars()
        .last()
        .is_some_and(|ch| matches!(ch, ',' | ';' | ':' | '-' | '—' | '–' | '.' | '…'))
    {
        trimmed = trimmed
            .trim_end_matches([',', ';', ':', '-', '—', '–', '.', '…'])
            .trim_end();
    }
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed
        .chars()
        .last()
        .is_some_and(|ch| matches!(ch, '!' | '?' | '！' | '？' | '。'))
    {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

pub(super) fn build_group_text(items: &[CleanNarrationItem]) -> String {
    items
        .iter()
        .map(|item| normalize_group_sentence(&item.tts_text))
        .filter(|sentence| !sentence.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn build_narration_groups(
    items: Vec<CleanNarrationItem>,
    grouping: &SubtitleNarrationGroupingRequest,
) -> Vec<NarrationRequestGroup> {
    let text_budget = grouping.text_budget_units.clamp(
        NARRATION_GROUP_MIN_TEXT_BUDGET,
        NARRATION_GROUP_MAX_TEXT_BUDGET,
    );
    let mut groups = Vec::new();
    let mut current = Vec::<CleanNarrationItem>::new();
    let mut current_units = 0usize;
    let mut current_chars = 0usize;
    let mut previous_end: Option<f64> = None;

    let flush_current = |groups: &mut Vec<NarrationRequestGroup>,
                         current: &mut Vec<CleanNarrationItem>,
                         current_units: &mut usize,
                         current_chars: &mut usize| {
        if current.is_empty() {
            return;
        }
        let group_index = groups.len();
        let text = build_group_text(current);
        groups.push(NarrationRequestGroup {
            id: format!("group-{group_index}"),
            items: std::mem::take(current),
            text,
        });
        *current_units = 0;
        *current_chars = 0;
    };

    for item in items {
        let item_chars = item.tts_text.chars().count();
        let gap = previous_end.map_or(0.0, |end| item.start_time - end);
        let should_start_new = !current.is_empty()
            && (current.len() >= NARRATION_GROUP_MAX_ITEMS
                || current_chars + item_chars > NARRATION_GROUP_MAX_CHARS
                || gap > NARRATION_GROUP_GAP_BREAK_SEC
                || current_units + item.text_units > text_budget);
        if should_start_new {
            flush_current(
                &mut groups,
                &mut current,
                &mut current_units,
                &mut current_chars,
            );
        }
        previous_end = Some(item.end_time);
        current_units += item.text_units;
        current_chars += item_chars;
        current.push(item);
    }
    flush_current(
        &mut groups,
        &mut current,
        &mut current_units,
        &mut current_chars,
    );
    groups
}

pub(super) fn split_group_audio_ranges(
    group: &NarrationRequestGroup,
    audio: &TtsCollectedAudio,
    vad_search_radius_sec: f64,
) -> NarrationSplitResult {
    let total_items = group.items.len();
    let duration_sec = (audio.duration_ms as f64 / 1000.0).max(0.05);
    if total_items <= 1 {
        return NarrationSplitResult {
            ranges: vec![NarrationAlignedRange {
                start_sec: 0.0,
                end_sec: duration_sec,
                confidence: 1.0,
            }],
            mode: "single",
        };
    }
    if audio.pcm_samples.len() < total_items {
        return NarrationSplitResult {
            ranges: (0..total_items)
                .map(|index| {
                    let start = duration_sec * index as f64 / total_items as f64;
                    let end = duration_sec * (index + 1) as f64 / total_items as f64;
                    NarrationAlignedRange {
                        start_sec: start,
                        end_sec: end.max(start + 0.05).min(duration_sec),
                        confidence: 0.25,
                    }
                })
                .collect(),
            mode: "estimated",
        };
    }
    NarrationSplitResult {
        ranges: split_group_audio_ranges_estimated(group, audio, vad_search_radius_sec),
        mode: "estimated",
    }
}

pub(super) fn split_group_audio_ranges_estimated(
    group: &NarrationRequestGroup,
    audio: &TtsCollectedAudio,
    vad_search_radius_sec: f64,
) -> Vec<NarrationAlignedRange> {
    let total_items = group.items.len();
    let duration_sec = (audio.duration_ms as f64 / 1000.0).max(0.05);
    let punctuation_total: f64 = group
        .items
        .iter()
        .map(|item| narration_pause_weight(&item.tts_text))
        .sum::<f64>()
        .max(0.0);
    let text_total: f64 = group
        .items
        .iter()
        .map(|item| item.text_units.max(1) as f64 + narration_pause_weight(&item.tts_text))
        .sum::<f64>()
        .max(1.0);
    let time_total: f64 = group
        .items
        .iter()
        .map(|item| (item.end_time - item.start_time).max(0.05))
        .sum::<f64>()
        .max(0.05);
    let weights = group
        .items
        .iter()
        .map(|item| {
            let text_ratio = (item.text_units.max(1) as f64
                + narration_pause_weight(&item.tts_text))
                / text_total;
            let time_ratio = (item.end_time - item.start_time).max(0.05) / time_total;
            (text_ratio * 0.82 + time_ratio * 0.18).max(0.001)
        })
        .collect::<Vec<_>>();

    let sample_rate = audio.sample_rate.max(1);
    let total_frames = audio.pcm_samples.len();
    let mut cumulative = 0.0;
    let ideal_frames = weights
        .iter()
        .take(total_items - 1)
        .map(|weight| {
            cumulative += *weight;
            ((cumulative * total_frames as f64).round() as usize)
                .min(total_frames.saturating_sub(1))
        })
        .collect::<Vec<_>>();
    let snapped_frames = if total_frames > 2 {
        snap_split_frames_to_silence(
            &audio.pcm_samples,
            1,
            sample_rate,
            &ideal_frames,
            vad_search_radius_sec.clamp(0.05, 1.0),
        )
    } else {
        ideal_frames
    };

    let min_gap_frames = ((sample_rate as f64 * 0.05).round() as usize).max(1);
    let candidate_frames = if snapped_frames.len() == total_items - 1 {
        snapped_frames
    } else {
        (1..total_items)
            .map(|index| total_frames * index / total_items)
            .collect::<Vec<_>>()
    };
    let mut boundaries = Vec::with_capacity(total_items + 1);
    boundaries.push(0usize);
    let mut previous = 0usize;
    for frame in candidate_frames {
        let min_next = previous.saturating_add(min_gap_frames);
        let max_next = total_frames.saturating_sub(min_gap_frames);
        let boundary = frame.max(min_next).min(max_next);
        if boundary > previous && boundary < total_frames {
            boundaries.push(boundary);
            previous = boundary;
        }
    }
    if boundaries.len() != total_items {
        boundaries = (0..=total_items)
            .map(|index| total_frames * index / total_items)
            .collect();
    } else {
        boundaries.push(total_frames);
    }
    let base_confidence = if punctuation_total > 0.0 { 0.5 } else { 0.42 };
    boundaries
        .windows(2)
        .take(total_items)
        .map(|pair| {
            let start = pair[0] as f64 / sample_rate as f64;
            let end = pair[1] as f64 / sample_rate as f64;
            NarrationAlignedRange {
                start_sec: start.min(duration_sec),
                end_sec: end.max(start + 0.05).min(duration_sec),
                confidence: base_confidence,
            }
        })
        .collect()
}

pub(super) fn narration_pause_weight(text: &str) -> f64 {
    let trimmed = text.trim();
    let Some(last) = trimmed.chars().rev().find(|ch| !ch.is_whitespace()) else {
        return 0.0;
    };
    match last {
        '.' | '。' => 0.9,
        '!' | '?' | '！' | '？' => 1.1,
        '…' => 1.2,
        ',' | ';' | ':' => 0.45,
        _ => 0.0,
    }
}
