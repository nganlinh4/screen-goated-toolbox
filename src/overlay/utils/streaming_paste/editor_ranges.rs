//! Exact caret-relative ranges and reversible selection capability checks.
use super::*;

#[derive(Debug)]
pub(crate) struct UnsafeCapture;
impl std::fmt::Display for UnsafeCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("selection ownership is uncertain; keyboard fallback excluded")
    }
}
impl std::error::Error for UnsafeCapture {}

pub(super) fn probe(editor: &Editor) -> Result<()> {
    let (_, _, _, original) = snapshot(&editor.text)?;

    let candidate = unsafe { original.Clone()? };
    let moved = unsafe { candidate.Move(TextUnit_Character, -1)? };
    let moved = if moved == 0 {
        unsafe { candidate.Move(TextUnit_Character, 1)? }
    } else {
        moved
    };
    // A genuinely empty editable document has no alternate caret position.
    if moved == 0 {
        ensure!(
            editor.before.is_empty() && editor.after.is_empty(),
            "nonempty text provider cannot expose a neighboring caret"
        );
        return Ok(());
    }
    editor.check()?;
    let observed = (|| -> Result<bool> {
        unsafe {
            candidate.Select()?;
        }
        editor.target.check()?;
        let (_, selected, _, actual) = snapshot(&editor.text)?;
        Ok(selected.is_empty() && unsafe { actual.Compare(&candidate)?.as_bool() })
    })();
    // Restore only while the same destination retains focus.
    let restore = (|| -> Result<()> {
        editor.target.check()?;
        unsafe {
            original.Select()?;
        }
        editor.check()?;
        Ok(())
    })();
    restore.map_err(|error| error.context(UnsafeCapture))?;
    ensure!(observed?, "text provider cannot move the editable caret");
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
