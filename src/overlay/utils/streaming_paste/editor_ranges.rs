//! Exact caret-relative ranges and read-only range capability checks.
use super::*;

#[derive(Debug)]
pub(crate) struct UnsafeCapture;
impl std::fmt::Display for UnsafeCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("selection ownership is uncertain; keyboard fallback excluded")
    }
}
impl std::error::Error for UnsafeCapture {}

pub(super) fn probe(editor: &Editor, allowed: &dyn Fn() -> bool) -> Result<()> {
    let (_, _, _, original) = snapshot(&editor.text)?;
    let candidate = unsafe { original.Clone()? };
    let moved = unsafe { candidate.Move(TextUnit_Character, -1)? };
    let moved = if moved == 0 {
        unsafe { candidate.Move(TextUnit_Character, 1)? }
    } else {
        moved
    };
    if moved == 0 {
        ensure!(
            editor.before.is_empty() && editor.after.is_empty(),
            "nonempty text provider cannot expose a neighboring caret"
        );
    }
    // Detached range movement is read-only. Never move and restore the user's
    // actual caret merely to discover whether a provider supports selection.
    check_probe(allowed, || editor.check())
}

fn check_probe(allowed: &dyn Fn() -> bool, check: impl FnOnce() -> Result<()>) -> Result<()> {
    ensure!(allowed(), UnsafeCapture);
    check().context(UnsafeCapture)?;
    ensure!(allowed(), UnsafeCapture);
    Ok(())
}

pub(super) fn suffix(
    caret: &IUIAutomationTextRange,
    expected: &str,
) -> Result<IUIAutomationTextRange> {
    let units = expected.encode_utf16().count();
    ensure!(units <= MAX_TEXT_UNITS, "replacement range exceeds bound");
    let mut low = 0usize;
    let mut high = units;
    // Character units may represent UTF-16, scalars, or grapheme clusters.
    // Search by observed text length; only exact text and endpoints authorize use.
    while low <= high {
        let count = low + (high - low) / 2;
        let range = unsafe { caret.Clone()? };
        unsafe {
            range.MoveEndpointByUnit(
                TextPatternRangeEndpoint_Start,
                TextUnit_Character,
                -(count as i32),
            )?;
            ensure!(
                range.CompareEndpoints(
                    TextPatternRangeEndpoint_End,
                    caret,
                    TextPatternRangeEndpoint_Start
                )? == 0,
                "replacement endpoint moved away from caret"
            );
        }
        let actual = range_text(&range)?;
        if actual == expected {
            return Ok(range);
        }
        if actual.encode_utf16().count() < units {
            low = count + 1;
        } else if count == 0 {
            break;
        } else {
            high = count - 1;
        }
    }
    anyhow::bail!("caret-relative range does not exactly match owned suffix")
}

#[cfg(test)]
mod selection_guard_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    };

    #[test]
    fn user_selection_change_during_blocked_probe_is_unsafe_for_fallback() {
        let changed = AtomicBool::new(false);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        std::thread::scope(|scope| {
            let changed = &changed;
            let worker = scope.spawn(move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                check_probe(&|| true, || {
                    anyhow::ensure!(!changed.load(Ordering::SeqCst), "selection changed");
                    Ok(())
                })
            });
            entered_rx.recv().unwrap();
            changed.store(true, Ordering::SeqCst);
            release_tx.send(()).unwrap();
            assert!(worker.join().unwrap().unwrap_err().is::<UnsafeCapture>());
        });
    }

    #[test]
    fn expired_probe_is_unsafe_for_fallback() {
        let error = check_probe(&|| false, || Ok(())).unwrap_err();
        assert!(error.is::<UnsafeCapture>());
    }
}
