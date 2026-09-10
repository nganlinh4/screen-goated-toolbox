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
use windows::core::BSTR;

const MAX_TEXT_UNITS: usize = 262_144;
const MAX_INPUT_UNITS: usize = 16_384;
#[path = "best_effort.rs"]
mod best_effort;
#[path = "editor_input.rs"]
mod input;
pub(super) use best_effort::BestEffortTarget;
use input::{input_epoch, input_settled, no_held_modifiers, send_unicode, validate_input};

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
            ensure!(selected.is_empty(), "initial selection is not collapsed");
            let editor = Self {
                target,
                text,
                anchor_bytes: before.len(),
                before,
                after,
            };
            editor.check()?;
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
        self.check()?;
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
        no_held_modifiers()?;
        let input_tick = input_epoch();
        let (_, _, _, caret) = snapshot(&self.text)?;
        let range = if old_tail.is_empty() {
            unsafe { caret.Clone()? }
        } else {
            // Find actual provider endpoints instead of guessing whether a UIA
            // Character unit means a scalar, UTF-16 unit, or grapheme cluster.
            let preceding = unsafe { self.text.DocumentRange()? };
            unsafe {
                preceding.MoveEndpointByRange(
                    TextPatternRangeEndpoint_End,
                    &caret,
                    TextPatternRangeEndpoint_Start,
                )?;
                let found = preceding.FindText(&BSTR::from(old_tail), true, false)?;
                ensure!(
                    found.CompareEndpoints(
                        TextPatternRangeEndpoint_End,
                        &caret,
                        TextPatternRangeEndpoint_Start
                    )? == 0,
                    "owned tail is not adjacent to caret"
                );
                found
            }
        };
        ensure!(
            range_text(&range)? == old_tail,
            "provider returned an ambiguous replacement range"
        );
        self.check()?;
        let current_tick = input_epoch();
        ensure!(
            current_tick == input_tick,
            "input changed before selection: previous={input_tick} current={current_tick}"
        );
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
        let current_tick = input_epoch();
        ensure!(
            current_tick == input_tick,
            "input changed before replacement: previous={input_tick} current={current_tick}"
        );
        no_held_modifiers()?;
        self.target.check()?;
        ensure!(allowed(), "input session ended before replacement");
        send_unicode(new_tail, !old_tail.is_empty())?;
        let deadline = Instant::now() + Duration::from_millis(350);
        let mut settled = None;
        let mut last_error = String::new();
        loop {
            self.target.check()?;
            let matches = match snapshot(&self.text) {
                Ok((before, selected, after, _)) => {
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
        let input_epoch_before = input_epoch();
        validate_input(text)?;
        self.check()?;
        no_held_modifiers()?;
        self.check()?;
        ensure!(
            input_epoch() == input_epoch_before,
            "user input changed before append"
        );
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
    unsafe {
        if let Ok(value) =
            element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        {
            ensure!(
                !value.CurrentIsReadOnly()?.as_bool(),
                "focused value is read-only"
            );
        } else {
            ensure!(
                element.CurrentControlType()? == UIA_EditControlTypeId,
                "document-only text providers cannot be revised"
            );
            let read_only = text
                .DocumentRange()?
                .GetAttributeValue(UIA_IsReadOnlyAttributeId)?;
            ensure!(
                read_only.vt() == VT_BOOL && !bool::try_from(&read_only)?,
                "editable text capability is unverified"
            );
        }
        Ok(())
    }
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
