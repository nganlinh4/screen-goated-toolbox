//! Steer Gemini Translate cues toward a target duration. The output VAD splits
//! the voice at natural pauses, which gives very uneven spans (a long pause-free
//! read becomes one big cue; a rapid list becomes many tiny ones). This re-groups
//! those spans toward `~target` seconds: it splits a long pause-free span into
//! even pieces and merges short ones into their neighbours, so cues stay near the
//! desired length without ever leaving one too short.

/// Re-group contiguous speech spans `(start_sec, end_sec)` toward `target_sec`.
/// Input spans must be contiguous (`end_i == start_{i+1}`); the result is too,
/// covering the same `[first.start, last.end]` range.
///
/// Walks the audio in ~`target` steps. At each step it snaps the cut to a natural
/// pause (a span boundary) when one is within `SNAP * target` of the ideal cut —
/// giving a clean sentence break — otherwise it cuts cleanly at the ideal point so
/// the cue still lands near the target rather than drifting to wherever the pauses
/// happen to fall. Every cue (and the remainder) is kept at least `MIN * target`,
/// so none is ever too short, and cuts at ~target keep none too long.
pub(super) fn resegment(spans: &[(f64, f64)], target_sec: f64) -> Vec<(f64, f64)> {
    const MIN: f64 = 0.5; // shortest cue, as a fraction of target
    const SNAP: f64 = 0.2; // snap to a pause only within this fraction of target

    if spans.is_empty() {
        return Vec::new();
    }
    let target = target_sec.max(0.5);
    let min_seg = target * MIN;
    let snap_window = target * SNAP;
    let start = spans[0].0;
    let end = spans[spans.len() - 1].1;
    // Interior pause points (each span's end except the last) — clean break points.
    let pauses: Vec<f64> = spans[..spans.len() - 1].iter().map(|span| span.1).collect();

    let mut result = Vec::new();
    let mut cur = start;
    // Cut while enough remains that another cue is warranted; the leftover becomes
    // the final cue (kept >= min by construction).
    while end - cur > target + snap_window {
        let ideal = cur + target;
        let mut cut = ideal;
        let mut best = f64::INFINITY;
        for &pause in &pauses {
            // Keep both this cue and the remainder long enough.
            if pause < cur + min_seg || end - pause < min_seg {
                continue;
            }
            let dist = (pause - ideal).abs();
            if dist <= snap_window && dist < best {
                best = dist;
                cut = pause;
            }
        }
        cut = cut.clamp(cur + min_seg, end - min_seg);
        result.push((cur, cut));
        cur = cut;
    }
    result.push((cur, end));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn min_dur(spans: &[(f64, f64)]) -> f64 {
        spans
            .iter()
            .map(|(a, b)| b - a)
            .fold(f64::INFINITY, f64::min)
    }

    fn assert_contiguous(spans: &[(f64, f64)], start: f64, end: f64) {
        assert!((spans[0].0 - start).abs() < 1e-6, "starts at {start}");
        assert!(
            (spans[spans.len() - 1].1 - end).abs() < 1e-6,
            "ends at {end}"
        );
        for window in spans.windows(2) {
            assert!((window[0].1 - window[1].0).abs() < 1e-6, "contiguous");
        }
    }

    #[test]
    fn splits_long_pause_free_read() {
        // One 12s pause-free span, target 4 → several even pieces, none over cap.
        let out = resegment(&[(0.0, 12.0)], 4.0);
        assert!(out.len() >= 3);
        assert!(out.iter().all(|(a, b)| b - a <= 4.0 * 1.6 + 1e-6));
        assert_contiguous(&out, 0.0, 12.0);
    }

    #[test]
    fn merges_short_reads_never_too_short() {
        // Ten 1s spans, target 4 → merged toward ~4, none below min (2.0).
        let spans: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, i as f64 + 1.0)).collect();
        let out = resegment(&spans, 4.0);
        assert!(min_dur(&out) >= 2.0 - 1e-6);
        assert_contiguous(&out, 0.0, 10.0);
    }

    #[test]
    fn absorbs_short_orphan_between_long_reads() {
        // 4s, 0.5s, 4s: the lone 0.5s cannot stand alone — it must be absorbed.
        let out = resegment(&[(0.0, 4.0), (4.0, 4.5), (4.5, 8.5)], 4.0);
        assert!(min_dur(&out) >= 2.0 - 1e-6);
        assert_contiguous(&out, 0.0, 8.5);
    }

    #[test]
    fn short_tail_folds_into_predecessor() {
        // 4s then a 0.6s tail → the tail folds back, leaving no short cue.
        let out = resegment(&[(0.0, 4.0), (4.0, 4.6)], 4.0);
        assert_eq!(out.len(), 1);
        assert_contiguous(&out, 0.0, 4.6);
    }
}
