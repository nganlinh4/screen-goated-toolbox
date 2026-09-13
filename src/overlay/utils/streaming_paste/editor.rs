//! Ownership-checked text replacement on one apartment-bound UI Automation target.

use anyhow::{Context, Result, ensure};
use std::marker::PhantomData;
use std::rc::Rc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Variant::VT_BOOL;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

const MAX_TEXT_UNITS: usize = 262_144;
const MAX_INPUT_UNITS: usize = 16_384;
#[path = "best_effort.rs"]
mod best_effort;
#[path = "editor_input.rs"]
mod input;
#[path = "editor_ranges.rs"]
mod ranges;
pub(super) use best_effort::BestEffortTarget;
use input::{input_settled, send_unicode, validate_input};
pub(super) use ranges::UnsafeCapture;

#[derive(Debug)]
pub(super) struct BeforeMutation;

impl std::fmt::Display for BeforeMutation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("target changed before any editor mutation")
    }
}
pub(super) fn shortcut_held() -> bool {
    input::shortcut_held()
}

#[cfg(test)]
pub(super) fn require_test_target(foreground: HWND) -> Result<()> {
    let mut process = 0;
    unsafe { GetWindowThreadProcessId(foreground, Some(&mut process)) };
    ensure!(
        process == std::process::id(),
        "UI test cannot target another process"
    );
    Ok(())
}

struct Target {
    automation: IUIAutomation2,
    element: IUIAutomationElement,
    foreground: HWND,
    process: u32,
    thread: u32,
    _apartment: PhantomData<Rc<()>>,
}

impl Target {
    fn capture(expected: HWND) -> Result<Self> {
        #[cfg(test)]
        require_test_target(expected)?;
        unsafe {
            ensure!(
                !expected.is_invalid() && GetForegroundWindow() == expected,
                "foreground changed before capture"
            );
            let automation: IUIAutomation2 =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
            automation.SetAutoSetFocus(false)?;
            automation.SetConnectionTimeout(200)?;
            automation.SetTransactionTimeout(200)?;
            let element = automation.GetFocusedElement()?;
            let root = automation.ElementFromHandle(expected)?;
            let walker = automation.RawViewWalker()?;
            let mut ancestor = element.clone();
            let mut belongs = false;
            for _ in 0..32 {
                if automation.CompareElements(&ancestor, &root)?.as_bool() {
                    belongs = true;
                    break;
                }
                let Ok(parent) = walker.GetParentElement(&ancestor) else {
                    break;
                };
                ancestor = parent;
            }
            ensure!(belongs, "focused element is outside captured foreground");
            let mut process = 0;
            let thread = GetWindowThreadProcessId(expected, Some(&mut process));
            ensure!(
                thread != 0 && process != 0,
                "foreground identity unavailable"
            );
            let target = Self {
                automation,
                element,
                foreground: expected,
                process,
                thread,
                _apartment: PhantomData,
            };
            target.check()?;
            crate::log_info!(
                "[AutoPasteTarget] pid={} thread={} control_type={} framework={:?} class={:?}",
                process,
                thread,
                target.element.CurrentControlType()?.0,
                target.element.CurrentFrameworkId()?,
                target.element.CurrentClassName()?
            );
            Ok(target)
        }
    }

    fn check(&self) -> Result<()> {
        unsafe {
            ensure!(
                GetForegroundWindow() == self.foreground,
                "foreground changed"
            );
            let mut process = 0;
            let thread = GetWindowThreadProcessId(self.foreground, Some(&mut process));
            ensure!(
                process == self.process && thread == self.thread,
                "foreground identity changed"
            );
            ensure!(
                self.element.CurrentIsEnabled()?.as_bool(),
                "focused control is disabled"
            );
            ensure!(
                !self.element.CurrentIsPassword()?.as_bool(),
                "password fields are excluded"
            );
            ensure!(
                self.element.CurrentHasKeyboardFocus()?.as_bool(),
                "editor lost keyboard focus"
            );
            let focused = self.automation.GetFocusedElement()?;
            ensure!(
                self.automation
                    .CompareElements(&focused, &self.element)?
                    .as_bool(),
                "focused element changed"
            );
            ensure!(
                GetForegroundWindow() == self.foreground,
                "foreground changed during verification"
            );
            Ok(())
        }
    }
}

/// No focus restoration, clipboard, or guessed caret operations are permitted.
pub(super) struct Editor {
    target: Target,
    text: IUIAutomationTextPattern,
    before: String,
    after: String,
    anchor_bytes: usize,
}

impl Editor {
    pub(super) fn capture(expected: HWND) -> Result<Self> {
        let target = Target::capture(expected)?;
        unsafe {
            let text = target
                .element
                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .context("focused control has no verifiable text ranges")?;
            ensure_editable(&target.element, &text)?;
            let (before, selected, after, _) = snapshot(&text)?;
            if !selected.is_empty() {
                return Err(UnsafeCapture.into());
            }
            let editor = Self {
                target,
                text,
                anchor_bytes: before.len(),
                before,
                after,
            };
            editor.check()?;
            ranges::probe(&editor)?;
            Ok(editor)
        }
    }

    pub(super) fn check(&self) -> Result<()> {
        self.target.check()?;
        unsafe {
            ensure_editable(&self.target.element, &self.text)?;
        }
        let (before, selected, after, _) = snapshot(&self.text)?;
        ensure!(
            selected.is_empty() && before == self.before && after == self.after,
            "text or caret ownership changed"
        );
        self.target.check()
    }

    pub(super) fn replace(
        &mut self,
        old_tail: &str,
        new_tail: &str,
        allowed: &dyn Fn() -> bool,
    ) -> Result<()> {
        ensure!(allowed(), "input session is no longer current");
        self.check().context(BeforeMutation)?;
        ensure!(
            old_tail.len() <= self.before.len().saturating_sub(self.anchor_bytes),
            "replacement crosses the original user-text boundary"
        );
        let prefix = self
            .before
            .strip_suffix(old_tail)
            .context("old tail does not match owned caret suffix")?
            .to_owned();
        validate_input(new_tail)?;
        let next_before = format!("{prefix}{new_tail}");
        ensure!(
            next_before.encode_utf16().count() + self.after.encode_utf16().count()
                <= MAX_TEXT_UNITS,
            "editor snapshot exceeds bound"
        );
        if old_tail == new_tail {
            return Ok(());
        }
        let unchanged = old_tail
            .chars()
            .zip(new_tail.chars())
            .take_while(|(old, new)| old == new)
            .map(|(character, _)| character.len_utf8())
            .sum::<usize>();
        let unchanged_chars = old_tail[..unchanged].chars().count();
        let full_old = old_tail;
        let full_new = new_tail;
        let original_prefix = prefix;
        let mut prefix = format!("{original_prefix}{}", &old_tail[..unchanged]);
        let mut old_tail = &old_tail[unchanged..];
        let mut new_tail = &new_tail[unchanged..];
        crate::log_paste_trace!(
            "[AutoPasteRange] preserved_chars={unchanged_chars} selected_chars={} inserted_chars={} surrounding_before_chars={} surrounding_after_chars={}",
            old_tail.chars().count(),
            new_tail.chars().count(),
            prefix.chars().count(),
            self.after.chars().count()
        );

        let (_, _, _, caret) = snapshot(&self.text)?;
        let range = ranges::suffix(&caret, old_tail).or_else(|_| {
            // A shared scalar prefix can end inside a provider character unit.
            // Widen only to the complete, still-owned tail, never user text.
            old_tail = full_old;
            new_tail = full_new;
            prefix = original_prefix;
            ranges::suffix(&caret, old_tail)
        })?;
        ensure!(
            range_text(&range)? == old_tail,
            "provider returned an ambiguous replacement range"
        );
        self.check().context(BeforeMutation)?;
        ensure!(allowed(), "input session ended before selection");
        unsafe {
            range.Select()?;
        }
        // Select is itself a mutation. A later failure suspends ownership; it
        // must never trigger an unverified final-paste fallback.
        self.target.check()?;
        let (before, selected, after, actual) = snapshot(&self.text)?;
        ensure!(
            before == prefix && selected == old_tail && after == self.after,
            "selected range no longer matches owned text"
        );
        unsafe {
            ensure!(
                actual.Compare(&range)?.as_bool(),
                "provider selected different endpoints"
            );
        }

        self.target.check()?;
        ensure!(allowed(), "input session ended before replacement");
        send_unicode(new_tail, !old_tail.is_empty())?;
        let deadline = Instant::now() + Duration::from_millis(350);
        let mut settled = None;
        loop {
            let last_error;
            self.target.check()?;
            let matches = match snapshot(&self.text) {
                Ok((before, selected, after, _)) => {
                    last_error = format!(
                        "expected_before_units={} observed_before_units={} selected_units={} expected_after_units={} observed_after_units={}",
                        next_before.encode_utf16().count(),
                        before.encode_utf16().count(),
                        selected.encode_utf16().count(),
                        self.after.encode_utf16().count(),
                        after.encode_utf16().count()
                    );
                    selected.is_empty() && before == next_before && after == self.after
                }
                Err(error) => {
                    last_error = format!("{error:#}");
                    false
                }
            };
            if matches && input_settled(&mut settled) {
                self.before = next_before;
                return self.check();
            } else if !matches {
                settled = None;
            }
            ensure!(
                Instant::now() < deadline,
                "replacement postcondition did not settle: {last_error}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Final-only input for a pinned writable target without safe revision capability.
/// It cannot safely revise a previous write and is never used after Editor mutates.
pub(super) struct AppendTarget {
    target: Target,
    text: Option<IUIAutomationTextPattern>,
    snapshot: Option<(String, String)>,
}

impl AppendTarget {
    pub(super) fn capture(expected: HWND) -> Result<Self> {
        let target = Target::capture(expected)?;
        unsafe {
            let kind = target.element.CurrentControlType()?;
            let value = target
                .element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId);
            if let Ok(value) = &value {
                ensure!(
                    !value.CurrentIsReadOnly()?.as_bool(),
                    "focused value is read-only"
                );
            } else {
                ensure!(
                    kind == UIA_EditControlTypeId || kind == UIA_DocumentControlTypeId,
                    "focused control does not accept text"
                );
            }
            let text = target
                .element
                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .context("final-only input requires verifiable selection and caret")?;
            ensure_append_writable(&target.element, &text)?;
            let text = Some(text);
            let snapshot = if let Some(text) = &text {
                if kind == UIA_EditControlTypeId {
                    ensure_editable(&target.element, text)?;
                }
                let (before, selected, after, _) = snapshot(text)?;
                ensure!(
                    selected.is_empty(),
                    "final-only input cannot replace a user selection"
                );
                Some((before, after))
            } else {
                None
            };
            let target = Self {
                target,
                text,
                snapshot,
            };
            target.check()?;
            Ok(target)
        }
    }

    pub(super) fn check(&self) -> Result<()> {
        self.target.check()?;
        unsafe {
            if let Ok(value) = self
                .target
                .element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            {
                ensure!(
                    !value.CurrentIsReadOnly()?.as_bool(),
                    "focused value became read-only"
                );
            }
        }
        if let (Some(text), Some((expected_before, expected_after))) = (&self.text, &self.snapshot)
        {
            unsafe {
                ensure_append_writable(&self.target.element, text)?;
                if self.target.element.CurrentControlType()? == UIA_EditControlTypeId {
                    ensure_editable(&self.target.element, text)?;
                }
            }
            let (before, selected, after, _) = snapshot(text)?;
            ensure!(
                selected.is_empty() && before == *expected_before && after == *expected_after,
                "final-only text or caret ownership changed"
            );
        }
        self.target.check()
    }

    pub(super) fn append(&mut self, text: &str, allowed: &dyn Fn() -> bool) -> Result<()> {
        ensure!(allowed(), "input session is no longer current");

        validate_input(text)?;
        self.check().context(BeforeMutation)?;

        self.check().context(BeforeMutation)?;
        ensure!(allowed(), "input session ended before append");
        send_unicode(text, false)?;
        if let Some((before, _)) = &mut self.snapshot {
            before.push_str(text);
        }
        let deadline = Instant::now() + Duration::from_millis(350);
        let mut settled = None;
        loop {
            self.target.check()?;
            let matches = self.check().is_ok();
            if matches && input_settled(&mut settled) {
                return Ok(());
            } else if !matches {
                settled = None;
            }
            ensure!(
                Instant::now() < deadline,
                "final-only insertion postcondition failed"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

unsafe fn ensure_editable(
    element: &IUIAutomationElement,
    text: &IUIAutomationTextPattern,
) -> Result<()> {
    // Writable text capability, not control type, determines whether a document
    // may be revised. Exact selection and mutation postconditions remain required.
    unsafe { ensure_append_writable(element, text) }
}

unsafe fn ensure_append_writable(
    element: &IUIAutomationElement,
    text: &IUIAutomationTextPattern,
) -> Result<()> {
    unsafe {
        let value_read_only =
            match element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                Ok(value) => Some(value.CurrentIsReadOnly()?.as_bool()),
                Err(_) => None,
            };
        let attribute_read_only = if value_read_only.is_none() {
            let value = text
                .DocumentRange()?
                .GetAttributeValue(UIA_IsReadOnlyAttributeId)?;
            if value.vt() == VT_BOOL {
                Some(bool::try_from(&value)?)
            } else {
                None
            }
        } else {
            None
        };
        ensure!(
            append_writable(value_read_only, attribute_read_only),
            "final-only target has no positive writable capability"
        );
        Ok(())
    }
}

fn append_writable(value_read_only: Option<bool>, attribute_read_only: Option<bool>) -> bool {
    matches!(value_read_only.or(attribute_read_only), Some(false))
}

fn range_text(range: &IUIAutomationTextRange) -> Result<String> {
    let value = unsafe { range.GetText((MAX_TEXT_UNITS + 1) as i32)? };
    ensure!(value.len() <= MAX_TEXT_UNITS, "text snapshot exceeds bound");
    String::from_utf16(&value).context("provider text contains ambiguous UTF-16")
}

fn snapshot(
    text: &IUIAutomationTextPattern,
) -> Result<(String, String, String, IUIAutomationTextRange)> {
    unsafe {
        let selections = text.GetSelection()?;
        ensure!(
            selections.Length()? == 1,
            "editor does not expose exactly one selection"
        );
        let selection = selections.GetElement(0)?;
        let document = text.DocumentRange()?;
        let whole = range_text(&document).context("document snapshot")?;
        let before = document.Clone()?;
        before.MoveEndpointByRange(
            TextPatternRangeEndpoint_End,
            &selection,
            TextPatternRangeEndpoint_Start,
        )?;
        let after = document.Clone()?;
        after.MoveEndpointByRange(
            TextPatternRangeEndpoint_Start,
            &selection,
            TextPatternRangeEndpoint_End,
        )?;
        let before = range_text(&before).context("prefix snapshot")?;
        let selected = range_text(&selection).context("selection snapshot")?;
        let after = range_text(&after).context("suffix snapshot")?;
        ensure!(
            format!("{before}{selected}{after}") == whole,
            "provider document and selection disagree"
        );
        if selected.is_empty() {
            ensure!(
                selection.CompareEndpoints(
                    TextPatternRangeEndpoint_Start,
                    &selection,
                    TextPatternRangeEndpoint_End
                )? == 0,
                "empty selection has noncollapsed endpoints"
            );
        }
        Ok((before, selected, after, selection))
    }
}

#[cfg(test)]
#[path = "editor_tests.rs"]
mod tests;
