use super::*;

#[derive(Clone)]
pub(super) struct CleanNarrationItem {
    pub(super) id: String,
    pub(super) text: String,
    pub(super) tts_text: String,
    pub(super) aligner_text: String,
    pub(super) start_time: f64,
    pub(super) end_time: f64,
    pub(super) text_units: usize,
}

#[derive(Clone)]
pub(super) struct NarrationRequestGroup {
    pub(super) id: String,
    pub(super) items: Vec<CleanNarrationItem>,
    pub(super) text: String,
    pub(super) spans: Vec<NarrationGroupTextSpan>,
}

#[derive(Clone)]
pub(super) struct NarrationGroupTextSpan {
    subtitle_id: String,
    text: String,
    start_char: usize,
    end_char: usize,
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NarrationAlignerRequest<'a> {
    audio_path: &'a str,
    prompt_text: &'a str,
    language_code: Option<&'a str>,
    items: Vec<NarrationAlignerItem<'a>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NarrationAlignerItem<'a> {
    subtitle_id: &'a str,
    text: &'a str,
    start_char: usize,
    end_char: usize,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NarrationAlignerResponse {
    ranges: Vec<NarrationAlignerRange>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NarrationAlignerRange {
    subtitle_id: String,
    source_in_point: f64,
    source_out_point: f64,
    #[serde(default = "default_alignment_confidence")]
    confidence: f64,
}

pub(super) fn default_alignment_confidence() -> f64 {
    1.0
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

pub(super) fn normalize_alignment_text(text: &str) -> String {
    text.nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn append_group_sentence(
    text_parts: &mut Vec<String>,
    spans: &mut Vec<NarrationGroupTextSpan>,
    item: &CleanNarrationItem,
    cursor: &mut usize,
) {
    let sentence = normalize_group_sentence(&item.tts_text);
    if sentence.is_empty() {
        return;
    }
    if !text_parts.is_empty() {
        *cursor += 1;
    }
    let start_char = *cursor;
    *cursor += sentence.chars().count();
    spans.push(NarrationGroupTextSpan {
        subtitle_id: item.id.clone(),
        text: item.aligner_text.clone(),
        start_char,
        end_char: *cursor,
    });
    text_parts.push(sentence);
}

pub(super) fn build_group_text_and_spans(
    items: &[CleanNarrationItem],
) -> (String, Vec<NarrationGroupTextSpan>) {
    let mut text_parts = Vec::new();
    let mut spans = Vec::new();
    let mut cursor = 0usize;
    for item in items {
        append_group_sentence(&mut text_parts, &mut spans, item, &mut cursor);
    }
    (text_parts.join(" "), spans)
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
        let (text, spans) = build_group_text_and_spans(current);
        groups.push(NarrationRequestGroup {
            id: format!("group-{group_index}"),
            items: std::mem::take(current),
            text,
            spans,
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

pub(super) fn align_group_audio_ranges(
    group: &NarrationRequestGroup,
    audio_path: &str,
    audio: &TtsCollectedAudio,
    language_code: Option<&str>,
    vad_search_radius_sec: f64,
) -> NarrationSplitResult {
    if let Some(aligned) = try_external_narration_aligner(group, audio_path, audio, language_code) {
        return aligned;
    }
    split_group_audio_ranges(group, audio, vad_search_radius_sec)
}

pub(super) fn try_external_narration_aligner(
    group: &NarrationRequestGroup,
    audio_path: &str,
    audio: &TtsCollectedAudio,
    language_code: Option<&str>,
) -> Option<NarrationSplitResult> {
    let command = std::env::var(NARRATION_ALIGNER_ENV).ok()?;
    let command = command.trim();
    if command.is_empty() {
        return None;
    }

    let items = group
        .spans
        .iter()
        .map(|span| NarrationAlignerItem {
            subtitle_id: &span.subtitle_id,
            text: &span.text,
            start_char: span.start_char,
            end_char: span.end_char,
        })
        .collect();
    let request = NarrationAlignerRequest {
        audio_path,
        prompt_text: &group.text,
        language_code,
        items,
    };
    let payload = serde_json::to_vec(&request).ok()?;
    let mut child = std::process::Command::new(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    if let Some(stdin) = child.stdin.as_mut()
        && stdin.write_all(&payload).is_err()
    {
        let _ = child.kill();
        return None;
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        eprintln!(
            "[Narration][Aligner] command failed status={:?} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(600)
                .collect::<String>()
        );
        return None;
    }
    let response: NarrationAlignerResponse = serde_json::from_slice(&output.stdout).ok()?;
    if response.ranges.len() != group.items.len() {
        return None;
    }
    let duration_sec = (audio.duration_ms as f64 / 1000.0).max(0.05);
    let mut by_id = response
        .ranges
        .into_iter()
        .map(|range| (range.subtitle_id.clone(), range))
        .collect::<HashMap<_, _>>();
    let mut previous_end = 0.0;
    let mut ranges = Vec::with_capacity(group.items.len());
    for item in &group.items {
        let range = by_id.remove(&item.id)?;
        let start = range
            .source_in_point
            .max(previous_end)
            .clamp(0.0, duration_sec);
        let end = range.source_out_point.max(start + 0.05).min(duration_sec);
        previous_end = end;
        ranges.push(NarrationAlignedRange {
            start_sec: start,
            end_sec: end,
            confidence: range.confidence.clamp(0.0, 1.0),
        });
    }
    Some(NarrationSplitResult {
        ranges,
        mode: "aligned",
    })
}
