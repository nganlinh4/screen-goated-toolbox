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
//! is worse than showing repeated text, and an image may legitimately repeat
//! itself: tables, forms, receipts and lists all restate whole rows.
//!
//! A cut therefore needs four things together: a span that repeats earlier
//! content, an almost entirely repeated window after it, a mostly repeated
//! remainder, and -- the part that separates damage from ordinary repetition --
//! a broken word. Correct output repeats whole tokens; the defect re-tokenizes
//! as it repeats and emits pieces of words that appear intact elsewhere.
//!
//! The cost of that strictness is that a repetition carrying no damage is kept,
//! because it cannot be told apart from a correct reading of an image that
//! really does show the same text twice. That is the right way to be wrong.
//!
//! Every threshold here is an absolute character count, so each one is also a
//! blind spot for replies shorter than it. OCR replies are routinely a single
//! filename or label, so the floors are kept at the smallest value the scan can
//! actually work with rather than at a comfortable-looking round number.

/// Span that proves a restatement is one, rather than a coincidence.
///
/// Ordinary prose repeats shorter runs all the time, so nothing counts as a
/// restatement until one run this long has occurred earlier.
const MIN_REPEAT_SPAN: usize = 8;

/// Span that counts as evidence *once a restatement is established*.
///
/// Seam damage chops a restatement into fragments, and requiring every fragment
/// to be as long as the anchor discards nearly all of it: the reported
/// `nvidia/nemotron-mini-4b-instruct` case reassembled as three runs of 7, 11 and
/// 7 characters covering the tail exactly, of which only the 11 counted -- 0.44
/// against a 0.80 floor. The anchor already ruled out coincidence; after it,
/// short runs are what the damage left behind, not noise.
const MIN_FRAGMENT_SPAN: usize = 3;

/// Characters examined immediately after a candidate cut.
const ONSET_WINDOW: usize = 32;

/// Repetition share required in that window for a cut to be made.
const LOCAL_COVERAGE: f32 = 0.92;

/// Repetition share required across everything after the cut.
const TAIL_COVERAGE: f32 = 0.80;

/// Shortest reply that can be judged at all.
///
/// Derived, not tuned: a cut needs one repeat span before the onset and one
/// after it, so below twice [`MIN_REPEAT_SPAN`] the scan cannot produce a
/// candidate no matter what the text says. Anything higher would be an extra
/// precision rule, and precision is already carried by [`is_fragmented`].
///
/// It was previously 32, from when length *was* the precision defence. That
/// floor silently disabled the whole guard for short replies -- `DJI_0872.JPG`
/// re-emitted as `DJI` / `_087` / `2.JPG` is 24 characters and was returned
/// untouched -- which is backwards: the shorter the reply, the less room there
/// is for a repetition to be a legitimate reading of the image.
const MIN_JUDGED_CHARS: usize = MIN_REPEAT_SPAN * 2;

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
    // How much real text each kept character stands for, so that how much
    // restatement has arrived is measured in what the model actually emitted
    // rather than in what survived collapsing.
    let mut written = Vec::with_capacity(MAX_SCANNED_CHARS);
    let mut total_written = 0usize;
    for (offset, ch) in text.char_indices() {
        if ch.is_whitespace() {
            continue;
        }
        if chars.len() == MAX_SCANNED_CHARS {
            break;
        }
        let folded = ch.to_lowercase().next().unwrap_or(ch);
        // Collapse a character that repeats itself. The defect duplicates one
        // character at each seam where it re-tokenizes -- `Screensho` + `ot`
        // arrives as `Screenshoot`, `nvidia` + `a/nem` as `nvidiaa/nem` -- and
        // those duplicates sit exactly where the verbatim runs would otherwise
        // be measured, so every seam shortens the evidence for the restatement
        // it is part of. Left in, damage suppresses its own detection: the worse
        // the corruption, the less of it is recognisable as a repeat.
        //
        // Collapsing applies to the whole scan, so a doubled letter in ordinary
        // text is folded on both sides of the comparison and nothing is
        // distorted. The recorded offset is the original one, so a cut still
        // lands where the text really begins.
        total_written += 1;
        if chars.last() == Some(&folded) {
            // A collapsed run can straddle the boundary: the good text ends with
            // `-` and the restatement opens with `-`, and once merged there is
            // nothing to say which side the survivor belongs to. Point it at the
            // later occurrence, so a cut here removes the restatement's copy and
            // leaves the original's intact. Guessing the other way would discard
            // a character of correct output.
            if let Some(last) = offsets.last_mut() {
                *last = offset;
            }
            continue;
        }
        chars.push(folded);
        offsets.push(offset);
        written.push(total_written - 1);
    }
    if chars.len() < MIN_JUDGED_CHARS {
        return None;
    }

    let last = chars.len().saturating_sub(MIN_REPEAT_SPAN);
    for (start, &offset) in offsets
        .iter()
        .enumerate()
        .take(last.saturating_add(1))
        .skip(MIN_REPEAT_SPAN)
    {
        // Cheap rejection first: without even a fragment repeating here, nothing
        // downstream can hold.
        if !occurs_earlier(&chars, start, MIN_FRAGMENT_SPAN) {
            continue;
        }
        if total_written - written[start] < min_evidence {
            continue;
        }
        // The anchor is a property of the whole restatement, not of its first
        // character. Damage at the very first seam used to disqualify an onset
        // outright, which is how an insertion or an omission escaped while the
        // same corruption with a clean first seam was caught.
        if !has_anchor(&chars, start) {
            continue;
        }
        let window_end = (start + ONSET_WINDOW).min(chars.len());
        if coverage(&chars, start, window_end) < LOCAL_COVERAGE {
            continue;
        }
        if coverage(&chars, start, chars.len()) < TAIL_COVERAGE {
            continue;
        }
        if !is_fragmented(text, offset) {
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

/// Whether the text after `onset` contains a broken word.
///
/// This is what separates the defect from content that simply repeats. A table,
/// a form or a list restates whole tokens, so every repeated word is intact. The
/// defect re-tokenizes as it repeats and emits pieces of words -- `Screensh` for
/// `Screenshot`, `khi` for `khiển` -- which never appear in correct output.
///
/// Only shorter pieces count. A longer token that merely starts with an earlier
/// one is ordinary language (`Plant` and `Plants`), not damage.
fn is_fragmented(text: &str, onset: usize) -> bool {
    let mut before: Vec<String> = Vec::new();
    let mut after: Vec<String> = Vec::new();
    for (offset, token) in text.split_whitespace().map(|token| {
        let offset = token.as_ptr() as usize - text.as_ptr() as usize;
        (offset, token.to_lowercase())
    }) {
        if offset < onset {
            before.push(token);
        } else {
            after.push(token);
        }
    }

    // Damage means a word was split across a seam, so its pieces rejoin into
    // something the reply had already written. That is the difference from text
    // that legitimately repeats itself: a table, a receipt, or a screen showing
    // `Configuration` beside a truncated `Config` repeats *whole* words, and no
    // two of them join into an earlier one.
    //
    // Asking only whether a piece looks like part of an earlier word cannot make
    // that distinction -- `Config` really is the start of `Configuration` -- and
    // reading it that way cuts a correct reply in half.
    let haystack = squeeze(&before.concat());
    after.windows(2).any(|pair| {
        let joined = squeeze(&format!("{}{}", pair[0], pair[1]));
        joined.chars().count() > pair[0].chars().count()
            && !before.contains(&pair[0])
            && haystack.contains(&joined)
    })
}

/// Collapses runs of one character, so a seam duplicate does not hide the join.
///
/// `nvidia` + `a/nem` is `nvidiaa/nem` as emitted, and `nvidia/nem` as written
/// originally; without this the two never match.
fn squeeze(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if !out.ends_with(ch) {
            out.push(ch);
        }
    }
    out
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

/// Whether anything after `start` repeats earlier content at anchor length.
///
/// One such run anywhere in the restatement is enough. It is what separates a
/// restatement from text that merely shares short runs with itself, and it is
/// checked over the whole tail so damage cannot hide it by landing early.
fn has_anchor(chars: &[char], start: usize) -> bool {
    (start..chars.len().saturating_sub(MIN_REPEAT_SPAN) + 1)
        .any(|index| occurs_earlier(chars, index, MIN_REPEAT_SPAN))
}

/// Share of `chars[start..end]` covered by spans that occurred before `start`.
///
/// Fragments count from [`MIN_FRAGMENT_SPAN`] upwards, because the caller has
/// already established an anchor. Measuring recall here and leaving precision to
/// the damage test is deliberate: a legitimately repetitive image -- a table, a
/// receipt -- also covers itself completely, and is kept because its repeated
/// words are whole.
fn coverage(chars: &[char], start: usize, end: usize) -> f32 {
    if end <= start {
        return 0.0;
    }
    let mut covered = 0usize;
    let mut index = start;
    while index < end {
        let mut span = 0usize;
        let mut length = MIN_FRAGMENT_SPAN;
        while index + length <= chars.len() && occurs_earlier(chars, index, length) {
            span = length;
            length += 1;
        }
        if span >= MIN_FRAGMENT_SPAN {
            covered += span.min(end - index);
            index += span;
        } else {
            index += 1;
        }
    }
    covered as f32 / (end - start) as f32
}

#[path = "repetition/guard.rs"]
mod guard;
pub(super) use guard::{GuardAction, RepetitionGuard};

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
        // Captured live on 2026-08-21. The restatement duplicates a character
        // at every seam -- `nvidia` + `a/nem`, `otron-` + `-mini-4` -- which is
        // the same fault as the fixtures above, but on a reply short enough that
        // the duplicates were most of the evidence. Verbatim coverage came to
        // 0.44 against a 0.80 floor, so the clearest corruption of the set was
        // the one that scored lowest for being corrupt.
        (
            "- nvidia/nemotron-mini-4b-instruct -\n- nvidia\na/nem\notron-\n-mini-4",
            "- nvidia/nemotron-mini-4b-instruct -",
        ),
        // Captured live on 2026-08-21 from the OCR preset. Twenty-four
        // non-whitespace characters: the whole reply sat under the old length
        // floor, so the guard returned it untouched even though the fragments
        // are unmistakable.
        ("DJI_0872.JPG\nDJI\n_087\n2.JPG", "DJI_0872.JPG"),
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
        // Short replies, now that they are judged at all. Each repeats without
        // damage, which is what an image that really does show its text twice
        // looks like.
        "DJI_0872.JPG\nDJI_0872.JPG",
        "IMG_20260821_162425.jpg\nIMG_20260821_162430.jpg",
        "EXIT\nEXIT",
        "STOP STOP STOP",
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
    fn ordinary_text_that_repeats_whole_words_is_never_cut() {
        // Precision is carried entirely by the damage test, so these are the
        // cases that matter: every one repeats heavily, and every one repeats
        // *whole* words. `Configuration` beside a truncated `Config` is the
        // sharpest -- `Config` genuinely is the start of `Configuration`, so a
        // rule that only asked "does this look like part of an earlier word"
        // cut it in half.
        for sample in [
            "Configuration\nConfig\nConfiguration\nConfig",
            "Air Plant\nAir Plants\nAir Plant\nAir Plants",
            "Install\nInstaller\nInstall\nInstaller\nInstall",
            "run\nrunning\nrunner\nrun\nrunning\nrunner",
            "self-\ncontained\nself-\ncontained",
            "1. Alpha\n2. Alpha\n3. Alpha\n4. Alpha",
            "Coffee 3.50\nTea 3.50\nJuice 3.50\nWater 3.50",
            "Name: ____\nName: ____\nName: ____\nName: ____",
            "v1.0.0\nv1.0.1\nv1.0.2\nv1.0.3\nv1.0.4",
            "and miles to go before I sleep\nand miles to go before I sleep",
        ] {
            assert!(
                salvage(sample).is_none(),
                "cut ordinary repetitive text {sample:?} at {:?}",
                repetition_onset(sample)
            );
        }
    }

    #[test]
    fn a_seam_that_loses_a_character_is_still_a_restatement() {
        // Seams do not all corrupt the same way. The reported cases split
        // cleanly or duplicated a character; a seam that drops one leaves the
        // pieces just as joinable, and requiring every fragment to be as long as
        // the anchor hid it.
        let good = "- nvidia/nemotron-mini-4b-instruct -";
        let dropped = format!("{good}\n- nvidi\n/nem\notron\nmini-4");
        assert_eq!(salvage(&dropped).as_deref(), Some(good));
    }

    #[test]
    fn genuinely_repetitive_images_survive() {
        // Tables, forms and lists legitimately repeat whole rows. Cutting
        // these would silently drop real content.
        for sample in [
            "apple  1.00\napple  1.00\napple  1.00\napple  1.00",
            "Mon Tue Wed Thu Fri\nMon Tue Wed Thu Fri\nMon Tue Wed Thu Fri",
            "Total: $5.00\nTotal: $5.00\nTotal: $5.00\nTotal: $5.00",
            "Untitled.png\nUntitled.png\nUntitled.png\nUntitled.png",
        ] {
            assert!(
                salvage(sample).is_none(),
                "cut repetitive but correct text {sample:?} at {:?}",
                repetition_onset(sample)
            );
        }
    }

    #[test]
    fn an_undamaged_duplicate_is_left_alone() {
        // The model sometimes repeats itself with no fragmentation at all.
        // That output is identical to what a correct reading of an image that
        // genuinely shows the same text twice would produce, so it is kept.
        // Dropping real content is the worse error of the two.
        let clean_duplicate = "Screenshot 2026-08-19 213759.png Screenshot 2026-08-19 213630.png Screenshot 2026-08-19 213759.png Screenshot 2026-08-19 213630.png";
        assert!(salvage(clean_duplicate).is_none());
    }

    #[test]
    fn a_short_reply_is_judged_but_still_needs_damage_to_be_cut() {
        // Short replies are judged now: an OCR reply is routinely one
        // filename, and a floor above that length disabled the guard exactly
        // where a repetition is least likely to be a real reading of the image.
        assert!(looks_like_repetition_defect(
            "DJI_0872.JPG\nDJI\n_087\n2.JPG"
        ));
        assert!(looks_like_repetition_defect("abcdefgh\nabcd efgh"));

        // Being judged is not being cut. None of these carry a broken word.
        assert!(!looks_like_repetition_defect("HÀ NỘI\nPHỐ\nNHÀ CHUNG"));
        assert!(!looks_like_repetition_defect(""));
        // Exactly at the floor and almost entirely self-repeating, but it is
        // one whole token: repetition alone never justifies a cut.
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
    fn a_long_fragmented_restatement_is_cut_before_the_stream_ends() {
        // Enough damaged restatement to be provable early, so the window stops
        // showing it without waiting for the reply to finish.
        let good = "Screenshot 2026-08-19 213759.png\nScreenshot 2026-08-19 213630.png";
        let corrupted = format!(
            "{good}\nScreensho\nt 2026-08-19 21\n13759.png\nScreensh\nhot 2026-08-19 2\n213630.png\nScreensho\nt 2026-08-19 21\n13759.png"
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
    fn a_repetitive_but_correct_stream_is_never_cut() {
        // The same content three times over, undamaged: an image may really
        // show this, so no partially received chunk may condemn it.
        let good = "Total: $5.00";
        let repeated = format!("{good}\n{good}\n{good}\n{good}\n{good}\n{good}");
        let mut guard = RepetitionGuard::default();
        for chunk in char_chunks(&repeated, 7) {
            assert!(
                matches!(guard.observe(&chunk), GuardAction::Paint),
                "cut correct repetitive content mid-stream"
            );
        }
        assert_eq!(guard.finish(repeated.clone()), repeated);
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
