//! Salvages a reply that starts restating itself.
//!
//! Qwen3-VL re-emits text it has already produced, either verbatim or broken at
//! token boundaries (`Screenshot …` returning as `Screensho` / `t 2026-08-19 21`
//! / `13759.png`). The defect is upstream and open (QwenLM/Qwen3-VL#1611), and
//! it cannot be suppressed from the request: Groq documents `presence_penalty`
//! and `frequency_penalty` as unsupported by every model it serves and exposes
//! neither `top_k`, `min_p` nor `repetition_penalty` — the whole set of controls
//! Qwen's own model card prescribes. Seeds do not avoid it, and neither
//! JSON-object nor strict-array structured output prevents it.
//!
//! In every observed failure the reply is correct up to the point where it
//! begins again, so the good prefix is kept and the restatement dropped. That
//! costs nothing: no extra tokens are sent, no second model is called, and the
//! request is unchanged. This is deliberately provider-agnostic — it judges
//! text, not endpoints, so any model that develops the same fault is covered.
//!
//! Precision matters far more than recall here, because discarding correct text
//! is worse than showing repeated text. A cut is made only where a span
//! repeating earlier content begins, the following window is almost entirely
//! repetition, and the whole remainder is mostly repetition. Against the
//! product's OCR corpus every reference survives untouched, including a receipt
//! form, a price list repeating `$21.96` and `4" Pot`, and two filenames that
//! differ only in their timestamp.

/// Shortest span treated as a repeat. Ordinary prose repeats shorter runs.
const MIN_REPEAT_SPAN: usize = 8;

/// Characters examined immediately after a candidate cut.
const ONSET_WINDOW: usize = 32;

/// Repetition share required in that window for a cut to be made.
const LOCAL_COVERAGE: f32 = 0.92;

/// Repetition share required across everything after the cut.
const TAIL_COVERAGE: f32 = 0.80;

/// Replies shorter than this are never judged.
const MIN_JUDGED_CHARS: usize = 32;

/// Bound on characters examined. Vision replies are capped at 512 output tokens,
/// so this covers them while keeping the scan's cost bounded.
const MAX_SCANNED_CHARS: usize = 2_048;

/// Byte offset where `text` starts restating itself, if it does.
///
/// Everything before the offset is the usable reply.
pub(super) fn repetition_onset(text: &str) -> Option<usize> {
    repetition_onset_with_evidence(text, MIN_REPEAT_SPAN)
}

/// As [`repetition_onset`], but requiring `min_evidence` characters of
/// restatement before a cut is made.
///
/// A partial reply cannot be judged like a complete one. Mid-stream, the second
/// of two similar lines looks exactly like the start of a restatement, and only
/// its ending distinguishes them, so a decision taken too early truncates
/// correct text. Callers holding an incomplete reply demand more evidence;
/// callers holding a finished one do not need to.
pub(super) fn repetition_onset_with_evidence(text: &str, min_evidence: usize) -> Option<usize> {
    let mut chars = Vec::with_capacity(MAX_SCANNED_CHARS);
    let mut offsets = Vec::with_capacity(MAX_SCANNED_CHARS);
    for (offset, ch) in text.char_indices() {
        if ch.is_whitespace() {
            continue;
        }
        if chars.len() == MAX_SCANNED_CHARS {
            break;
        }
        chars.push(ch.to_lowercase().next().unwrap_or(ch));
        offsets.push(offset);
    }
    if chars.len() < MIN_JUDGED_CHARS {
        return None;
    }

    let last = chars.len().saturating_sub(MIN_REPEAT_SPAN);
    for (start, &offset) in offsets.iter().enumerate().take(last).skip(MIN_REPEAT_SPAN) {
        if !occurs_earlier(&chars, start, MIN_REPEAT_SPAN) {
            continue;
        }
        let window_end = (start + ONSET_WINDOW).min(chars.len());
        if coverage(&chars, start, window_end) < LOCAL_COVERAGE {
            continue;
        }
        if chars.len() - start < min_evidence {
            continue;
        }
        if coverage(&chars, start, chars.len()) < TAIL_COVERAGE {
            continue;
        }
        return Some(snap_to_token_end(text, offset));
    }
    None
}

/// Whether the reply repeats itself enough to need salvaging.
#[cfg(test)]
pub(super) fn looks_like_repetition_defect(text: &str) -> bool {
    repetition_onset(text).is_some()
}

/// A cut may land inside a token, because that token's tail already occurred
/// earlier. Completing the token keeps a correct word whole; a cut that already
/// sits on a boundary is left alone so nothing extra is kept.
fn snap_to_token_end(text: &str, cut: usize) -> usize {
    if cut == 0 || text[..cut].ends_with(char::is_whitespace) {
        return cut;
    }
    text[cut..]
        .find(char::is_whitespace)
        .map_or(text.len(), |offset| cut + offset)
}

/// Whether the span of `length` at `start` appears anywhere before `start`.
fn occurs_earlier(chars: &[char], start: usize, length: usize) -> bool {
    if start + length > chars.len() || length > start {
        return false;
    }
    chars[..start]
        .windows(length)
        .any(|window| window == &chars[start..start + length])
}

/// Share of `chars[start..end]` covered by spans that occurred before `start`.
fn coverage(chars: &[char], start: usize, end: usize) -> f32 {
    if end <= start {
        return 0.0;
    }
    let mut covered = 0usize;
    let mut index = start;
    while index < end {
        let mut span = 0usize;
        let mut length = MIN_REPEAT_SPAN;
        while index + length <= chars.len() && occurs_earlier(chars, index, length) {
            span = length;
            length += 1;
        }
        if span >= MIN_REPEAT_SPAN {
            covered += span.min(end - index);
            index += span;
        } else {
            index += 1;
        }
    }
    covered as f32 / (end - start) as f32
}

/// Watches a streamed reply and replaces it once it starts repeating.
///
/// Streaming paints as it goes, so by the time the fault is visible the window
/// already shows part of it. The salvaged text is therefore emitted behind
/// [`crate::api::WIPE_SIGNAL`], the established way in this codebase to replace
/// what a result window has already drawn, and everything after the onset is
/// suppressed.
#[derive(Default)]
pub(super) struct RepetitionGuard {
    seen: String,
    salvaged: Option<String>,
    checked_len: usize,
}

/// What a caller should paint for one streamed chunk.
pub(super) enum GuardAction {
    /// Paint the chunk unchanged.
    Paint,
    /// Replace everything already painted with this text.
    Replace(String),
    /// Paint nothing: the reply is already restating itself.
    Suppress,
}

/// Characters added between checks, so the scan is amortized over a stream
/// rather than repeated for every delta.
const CHECK_INTERVAL: usize = 48;

/// Restatement required before a still-arriving reply is cut.
///
/// Sized so a second similar line cannot trigger a cut on its opening alone.
const STREAMING_MIN_EVIDENCE: usize = 64;

impl RepetitionGuard {
    /// Records one streamed chunk and says what to paint.
    pub(super) fn observe(&mut self, chunk: &str) -> GuardAction {
        if self.salvaged.is_some() {
            return GuardAction::Suppress;
        }
        self.seen.push_str(chunk);
        if self.seen.len() < self.checked_len + CHECK_INTERVAL {
            return GuardAction::Paint;
        }
        self.checked_len = self.seen.len();
        match repetition_onset_with_evidence(&self.seen, STREAMING_MIN_EVIDENCE) {
            Some(onset) => {
                let salvaged = self.seen[..onset].trim_end().to_string();
                self.salvaged = Some(salvaged.clone());
                GuardAction::Replace(salvaged)
            }
            None => GuardAction::Paint,
        }
    }

    /// Restarts the guard when a transport replaces what it has painted.
    pub(super) fn restart(&mut self, text: &str) {
        self.seen.clear();
        self.seen.push_str(text);
        self.salvaged = None;
        self.checked_len = 0;
    }

    /// The reply to keep once the stream ends.
    ///
    /// Runs a final check, since the stream may have finished inside the
    /// interval between amortized scans.
    pub(super) fn finish(mut self, streamed: String) -> String {
        if let Some(salvaged) = self.salvaged.take() {
            return salvaged;
        }
        match repetition_onset(&streamed) {
            Some(onset) => streamed[..onset].trim_end().to_string(),
            None => streamed,
        }
    }
}

/// Drops a restatement from a reply received in one piece.
///
/// The production path streams through [`RepetitionGuard`]; this states the
/// same contract directly for the corpus tests.
#[cfg(test)]
fn salvage(text: &str) -> Option<String> {
    repetition_onset(text).map(|onset| text[..onset].trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Splits on character boundaries. Slicing raw bytes would cut multi-byte
    /// characters apart and silently drop those chunks.
    fn char_chunks(text: &str, size: usize) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        chars
            .chunks(size)
            .map(|chunk| chunk.iter().collect())
            .collect()
    }

    /// Observed failures, captured from the live endpoint, paired with the text
    /// that must survive.
    const CORRUPTED: &[(&str, &str)] = &[
        (
            "Screenshot 2026-08-19 213759.png\nScreenshot 2026-08-19 213630.png\nScreenshot\nt 2026-08-19 21\n3759.png\nScreensh\nnot 2026-08-19 2\n213630.png",
            "Screenshot 2026-08-19 213759.png\nScreenshot 2026-08-19 213630.png",
        ),
        (
            "Screenshot 2026-08-19 213759.png Screenshot 2026-08-19 213630.png Screenshot 2026-08-19 213759.png Screenshot 2026-08-19 213630.png",
            "Screenshot 2026-08-19 213759.png Screenshot 2026-08-19 213630.png",
        ),
        (
            "Screenshot 2026-08-19 213759.png\nScreenshot 2026-08-19 213630.png\nScreensho\not 2026-08-19 21\n13759.png\nScreensh\nhot 2026-08-19 2\n213630.png",
            "Screenshot 2026-08-19 213759.png\nScreenshot 2026-08-19 213630.png",
        ),
        (
            "Điều khiển máy tính\nĐiều khi\nển máy tính\nĐiều khiển má\ny tính",
            "Điều khiển máy tính",
        ),
        // Captured live on 2026-08-20, after the guard was written: the same
        // fault re-segmented differently, which is why the check measures
        // repetition rather than matching any particular break pattern.
        (
            "Screenshot 2026-08-19 213759.png\nScreenshot 2026-08-19 213630.png\nScreenshot\not 2026-08-19 21\n13759.png\nScreensh\not 2026-08-19 2\n213630.png",
            "Screenshot 2026-08-19 213759.png\nScreenshot 2026-08-19 213630.png",
        ),
    ];

    /// Correct extractions that must survive untouched, taken from the product's
    /// OCR corpus and including its most repetitive cases.
    const LEGITIMATE: &[&str] = &[
        "Screenshot 2026-08-19 213759.png\nScreenshot 2026-08-19 213630.png",
        "Shop Succulents | Assorted Collection of Live Air Plants, Hand Selected Variety Pack of Air Succulents | Collection of 6 — $21.96\nSunset Jade Plant - Crassula - Easy to Grow House Plant - 4\" Pot — $12.50\nAmerican Plant Exchange Windmill Cold Hardy Palm Tree Live Plant, 4\" Pot, Indoor Outdoor Use — $21.96",
        "primer/design Landing Page — #54\nGuidelines should document our take on link targets — #85\nAdd info about font usage in Figma — #44\nLinks w/ Octicons — #30\nFilters & Clearing Filters — #29",
        "Товарный чек №\n«        »                         20     г.\nНаименование\nКол-во\nЦена\nСумма\nИтого\nПодпись продавца",
        "Four score and seven years ago our fathers brought forth, upon this continent, a new nation, conceived in liberty.",
        "WIKIPEDIA TIẾNG VIỆT\nBạn chính là tác giả của Wikipedia\nBài viết: Tra cứu · Bài mới · Hỏi đáp · Thỉnh cầu · Thư viện",
    ];

    #[test]
    fn keeps_the_correct_prefix_of_every_observed_corruption() {
        for (corrupted, expected) in CORRUPTED {
            assert_eq!(
                salvage(corrupted).as_deref(),
                Some(*expected),
                "wrong salvage for {corrupted:?}"
            );
        }
    }

    #[test]
    fn never_touches_a_correct_extraction() {
        for sample in LEGITIMATE {
            assert!(
                salvage(sample).is_none(),
                "cut legitimate text {sample:?} at {:?}",
                repetition_onset(sample)
            );
        }
    }

    #[test]
    fn short_replies_are_never_judged() {
        assert!(!looks_like_repetition_defect("HÀ NỘI\nPHỐ\nNHÀ CHUNG"));
        assert!(!looks_like_repetition_defect(""));
        assert!(!looks_like_repetition_defect("aaaaaaaaaaaaaaaa"));
    }

    #[test]
    fn a_stream_is_replaced_once_and_then_stays_quiet() {
        let (corrupted, expected) = CORRUPTED[0];
        let mut guard = RepetitionGuard::default();
        let mut replaced: Option<String> = None;
        let mut suppressed = 0;
        let mut streamed = String::new();
        for chunk in char_chunks(corrupted, 8) {
            streamed.push_str(&chunk);
            match guard.observe(&chunk) {
                GuardAction::Paint => {}
                GuardAction::Replace(text) => replaced = Some(text),
                GuardAction::Suppress => suppressed += 1,
            }
        }
        // A mid-stream replacement is an optimisation, not a guarantee: a short
        // restatement is only provable once the reply ends. Whenever one does
        // happen it must be the correct text, and the final answer always is.
        if let Some(replaced) = replaced.as_deref() {
            assert_eq!(replaced, expected);
            assert!(suppressed > 0, "a replacement must stop later painting");
        }
        assert_eq!(guard.finish(streamed), expected);
    }

    #[test]
    fn a_long_restatement_is_cut_before_the_stream_ends() {
        // Enough restatement to be provable early, so the window stops showing
        // it without waiting for the reply to finish.
        let good = "Товарный чек №
Наименование
Кол-во
Цена
Сумма
Итого
Подпись продавца";
        let corrupted = format!(
            "{good}
{good}
{good}"
        );
        let mut guard = RepetitionGuard::default();
        let mut replaced = None;
        for chunk in char_chunks(&corrupted, 12) {
            if let GuardAction::Replace(text) = guard.observe(&chunk) {
                replaced = Some(text);
                break;
            }
        }
        assert_eq!(replaced.as_deref(), Some(good));
    }

    #[test]
    fn a_clean_stream_is_never_replaced() {
        let text = LEGITIMATE[1];
        let mut guard = RepetitionGuard::default();
        let mut streamed = String::new();
        for chunk in char_chunks(text, 8) {
            streamed.push_str(&chunk);
            assert!(
                matches!(guard.observe(&chunk), GuardAction::Paint),
                "clean text must paint unchanged"
            );
        }
        assert_eq!(guard.finish(streamed), text);
    }

    #[test]
    fn a_corruption_landing_between_scans_is_caught_at_the_end() {
        let (corrupted, expected) = CORRUPTED[3];
        let guard = RepetitionGuard::default();
        assert_eq!(guard.finish(corrupted.to_string()), expected);
    }
}
