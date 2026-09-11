//! Best-effort keyboard revisions when accessibility cannot describe an editor.
use super::input::{send_edit, validate_input};
use anyhow::{Context, Result, ensure};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId,
};

pub(crate) struct BestEffortTarget {
    foreground: HWND,
    focus: HWND,
    thread: u32,
    process: u32,
}

impl BestEffortTarget {
    pub(crate) fn capture(foreground: HWND) -> Result<Self> {
        #[cfg(test)]
        super::require_test_target(foreground)?;
        let mut process = 0;
        let thread = unsafe { GetWindowThreadProcessId(foreground, Some(&mut process)) };
        ensure!(thread != 0 && process != 0, "native target is unavailable");
        let target = Self {
            foreground,
            focus: focused_window(thread)?,
            thread,
            process,
        };
        target.check()?;
        Ok(target)
    }

    pub(crate) fn check(&self) -> Result<()> {
        // Missing accessibility is not a veto, but an explicit protected field is.
        unsafe {
            use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
            use windows::Win32::UI::Accessibility::{CUIAutomation8, IUIAutomation2};
            if let Ok(automation) =
                CoCreateInstance::<_, IUIAutomation2>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
            {
                automation.SetConnectionTimeout(200)?;
                automation.SetTransactionTimeout(200)?;
                if let Ok(element) = automation.GetFocusedElement() {
                    ensure!(
                        !element.CurrentIsPassword().is_ok_and(|v| v.as_bool()),
                        "password field is excluded"
                    );
                    ensure!(
                        !element.CurrentIsEnabled().is_ok_and(|v| !v.as_bool()),
                        "focused control is disabled"
                    );
                }
            }
        }
        let mut process = 0;
        let thread = unsafe { GetWindowThreadProcessId(self.foreground, Some(&mut process)) };
        ensure!(
            unsafe { GetForegroundWindow() } == self.foreground
                && thread == self.thread
                && process == self.process,
            "native foreground identity changed"
        );
        ensure!(
            focused_window(thread)? == self.focus,
            "native focus changed"
        );
        Ok(())
    }

    pub(crate) fn replace(&self, old: &str, new: &str, allowed: &dyn Fn() -> bool) -> Result<()> {
        validate_input(old)?;
        validate_input(new)?;
        let (backspaces, suffix) = keyboard_delta(old, new);
        crate::log_info!(
            "[AutoPasteKeyboard] backspaces={backspaces} inserted_chars={} delivery=unverified",
            suffix.chars().count()
        );
        self.check().context(super::BeforeMutation)?;
        ensure!(allowed(), "input session is no longer current");
        // No retries: accepted input events do not prove application delivery.
        send_edit(suffix, false, backspaces)?;
        self.check()
    }
}

fn keyboard_delta<'a>(old: &str, new: &'a str) -> (usize, &'a str) {
    let prefix_bytes: usize = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum();
    (old[prefix_bytes..].chars().count(), &new[prefix_bytes..])
}

fn focused_window(thread: u32) -> Result<HWND> {
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetGUIThreadInfo(thread, &mut info)? };
    Ok(info.hwndFocus)
}

#[cfg(test)]
mod tests {
    use super::keyboard_delta;
    #[test]
    fn keyboard_corrections_preserve_common_prefix_and_only_replace_tail() {
        assert_eq!(keyboard_delta("", "hello"), (0, "hello"));
        assert_eq!(keyboard_delta("hello", "hello world"), (0, " world"));
        assert_eq!(keyboard_delta("hello wurld", "hello world"), (4, "orld"));
        assert_eq!(keyboard_delta("café", "café!"), (0, "!"));
        assert_eq!(keyboard_delta("hello", "hello"), (0, ""));
        assert_eq!(keyboard_delta("tail", ""), (4, ""));
    }
}
