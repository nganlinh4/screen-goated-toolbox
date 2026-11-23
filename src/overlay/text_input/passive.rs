use super::key_ownership::{KeyOwnership, Modifiers, is_modifier};
use super::state::{
    EDITOR_SESSIONS, INPUT_HWND, PASSIVE_CAPTURE_ENABLED, SHOULD_CLOSE, SUBMITTED_TEXT,
    SubmittedText, WM_APP_INPUT_LIMIT, WM_APP_PASSIVE_KEY, WM_APP_SYNC_PASSIVE_EDITOR,
};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::{HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, MAPVK_VK_TO_VSC, MapVirtualKeyW, ToUnicodeEx,
    VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_RETURN, VK_RIGHT,
    VK_SHIFT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, IsWindowVisible, KBDLLHOOKSTRUCT, PostMessageW, WM_KEYDOWN,
    WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[derive(Default)]
struct PassiveEditorState {
    text: String,
    session: Option<u64>,
    revision: u64,
}

impl PassiveEditorState {
    fn changed(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("passive input revision exhausted");
    }

    fn clear_if_revision(&mut self, session: u64, expected: u64) -> bool {
        if self.session != Some(session) || self.revision != expected || self.revision == u64::MAX {
            return false;
        }
        self.text.clear();
        self.changed();
        true
    }

    fn apply(&mut self, session: u64, text: &str, replace: bool) -> Option<(String, String)> {
        if self.session != Some(session) {
            return None;
        }
        let limit = if replace {
            super::state::MAX_PENDING_TEXT_BYTES
        } else {
            super::state::MAX_PENDING_TEXT_BYTES.saturating_sub(self.text.len())
        };
        let text = normalize_line_endings(text, limit)?;
        if replace {
            self.text.clear();
        }
        self.text.push_str(&text);
        self.changed();
        Some((self.text.clone(), self.revision.to_string()))
    }

    fn append_if_fits(&mut self, text: &str) -> bool {
        let Some(text) = normalize_line_endings(
            text,
            super::state::MAX_PENDING_TEXT_BYTES.saturating_sub(self.text.len()),
        ) else {
            return false;
        };
        self.text.push_str(&text);
        self.changed();
        true
    }

    fn replace_if_fits(&mut self, text: &str) -> bool {
        let Some(text) = normalize_line_endings(text, super::state::MAX_PENDING_TEXT_BYTES) else {
            return false;
        };
        self.text = text.into_owned();
        self.changed();
        true
    }
}

fn normalize_line_endings(text: &str, limit: usize) -> Option<std::borrow::Cow<'_, str>> {
    if !text.contains('\r') {
        return (text.len() <= limit).then_some(std::borrow::Cow::Borrowed(text));
    }
    let mut result = String::with_capacity(text.len().min(limit));
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        let next = if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            '\n'
        } else {
            ch
        };
        if next.len_utf8() > limit.saturating_sub(result.len()) {
            return None;
        }
        result.push(next);
    }
    Some(std::borrow::Cow::Owned(result))
}

static PASSIVE_EDITOR_STATE: LazyLock<Mutex<PassiveEditorState>> =
    LazyLock::new(|| Mutex::new(PassiveEditorState::default()));
static KEY_OWNERSHIP: LazyLock<Mutex<KeyOwnership>> =
    LazyLock::new(|| Mutex::new(KeyOwnership::default()));

pub(super) fn seed_hook_keys() {
    let down: [bool; 256] =
        std::array::from_fn(|vk| unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 });
    let mut keys = KEY_OWNERSHIP
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *keys = KeyOwnership::default();
    keys.seed(|vk| down[vk as usize]);
}

pub(super) fn retire_hook_keys() {
    *KEY_OWNERSHIP
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = KeyOwnership::default();
}

pub fn reset_state() {
    let session = EDITOR_SESSIONS.current();
    let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
    *state = PassiveEditorState::default();
    state.session = session;
}

pub(super) fn clear_if_revision(session: u64, revision: &str) {
    let Ok(expected) = revision.parse::<u64>() else {
        return;
    };
    let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
    state.clear_if_revision(session, expected);
}

pub(super) fn apply_text(session: u64, text: &str, replace: bool) -> Option<(String, String)> {
    PASSIVE_EDITOR_STATE
        .lock()
        .unwrap()
        .apply(session, text, replace)
}

pub fn sync_editor() {
    let Some(session) = EDITOR_SESSIONS.current() else {
        return;
    };
    let (text, revision) = {
        let state = PASSIVE_EDITOR_STATE.lock().unwrap();
        if state.session != Some(session) {
            return;
        }
        (state.text.clone(), state.revision.to_string())
    };
    let escaped = text
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${")
        .replace('\n', "\\n")
        .replace('\r', "");

    super::state::TEXT_INPUT_WEBVIEW.with(|webview| {
        if let Some(wv) = webview.borrow().as_ref() {
            let script = format!(
                r#"(function() {{
                    const session = '{session}', revision = '{revision}';
                    const atMost = (a, b) => a.length < b.length || (a.length === b.length && a <= b);
                    if (window.__sgtTextSession !== session || !atMost(window.__sgtPassiveRevision || '0', revision)) return;
                    const editor = document.getElementById('editor');
                    if (!editor) return;
                    editor.value = `{}`;
                    window.__sgtPassiveRevision = revision;
                    window.__sgtTextRevision = (BigInt(window.__sgtTextRevision || '0') + 1n).toString();
                    editor.selectionStart = editor.selectionEnd = editor.value.length;
                    editor.scrollTop = editor.scrollHeight;
                }})();"#,
                escaped
            );
            let _ = wv.evaluate_script(&script);
        }
    });
}

pub unsafe extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if code != HC_ACTION as i32 {
            return CallNextHookEx(None, code, wparam, lparam);
        }

        let event = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let message = wparam.0 as u32;

        let down = match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => true,
            WM_KEYUP | WM_SYSKEYUP => false,
            _ => return CallNextHookEx(None, code, wparam, lparam),
        };
        let session = passive_capture_is_active()
            .then(|| EDITOR_SESSIONS.current())
            .flatten();
        let decision = KEY_OWNERSHIP
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .event(event.vkCode, event.scanCode, event.flags.0, down, session);
        if decision.deliver {
            defer_key(event.vkCode, decision.modifiers);
        }
        if decision.consume {
            LRESULT(1)
        } else {
            CallNextHookEx(None, code, wparam, lparam)
        }
    }
}

fn passive_capture_is_active() -> bool {
    if !PASSIVE_CAPTURE_ENABLED.load(Ordering::SeqCst) {
        return false;
    }

    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val == 0 {
        return false;
    }

    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

/// The hook only decides ownership. Editing, history persistence and
/// clipboard reads run after it returns, so they can never push the hook past
/// the system's low-level hook timeout.
fn defer_key(vk_code: u32, modifiers: Modifiers) {
    let hwnd = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut _)),
                WM_APP_PASSIVE_KEY,
                WPARAM(vk_code as usize),
                LPARAM(modifiers.to_bits()),
            );
        }
    }
}

pub(super) fn handle_deferred_key(wparam: WPARAM, lparam: LPARAM) {
    handle_key_down(wparam.0 as u32, Modifiers::from_bits(lparam.0));
}

fn handle_key_down(vk_code: u32, modifiers: Modifiers) {
    if is_modifier(vk_code) {
        return;
    }

    match vk_code {
        vk if vk == VK_ESCAPE.0 as u32 => {
            crate::overlay::input_history::reset_history_navigation();
            *SHOULD_CLOSE.lock().unwrap() = true;
            wake_input_window();
        }
        vk if vk == VK_RETURN.0 as u32 => {
            let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
            if modifiers.shift {
                if !state.append_if_fits("\n") {
                    report_input_limit(state.session);
                    return;
                }
                drop(state);
                post_editor_sync();
            } else {
                let submitted = state.text.trim().to_string();
                if !submitted.is_empty() {
                    crate::overlay::input_history::add_to_history(&submitted);
                    *SUBMITTED_TEXT.lock().unwrap() = Some(SubmittedText {
                        text: submitted,
                        snapshot: state.text.clone(),
                        revision: state.revision.to_string(),
                        passive: true,
                    });
                    *SHOULD_CLOSE.lock().unwrap() = true;
                    wake_input_window();
                }
            }
        }
        vk if vk == VK_BACK.0 as u32 => {
            let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
            if modifiers.ctrl {
                trim_last_word(&mut state.text);
            } else {
                state.text.pop();
            }
            state.changed();
            drop(state);
            post_editor_sync();
        }
        vk if vk == VK_DELETE.0 as u32 => {}
        vk if vk == VK_UP.0 as u32 => {
            let current = PASSIVE_EDITOR_STATE.lock().unwrap().text.clone();
            if let Some(text) = crate::overlay::input_history::navigate_history_up(&current) {
                let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
                if !state.replace_if_fits(&text) {
                    report_input_limit(state.session);
                    return;
                }
                drop(state);
                post_editor_sync();
            }
        }
        vk if vk == VK_DOWN.0 as u32 => {
            let current = PASSIVE_EDITOR_STATE.lock().unwrap().text.clone();
            if let Some(text) = crate::overlay::input_history::navigate_history_down(&current) {
                let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
                if !state.replace_if_fits(&text) {
                    report_input_limit(state.session);
                    return;
                }
                drop(state);
                post_editor_sync();
            }
        }
        vk if vk == VK_LEFT.0 as u32
            || vk == VK_RIGHT.0 as u32
            || vk == VK_HOME.0 as u32
            || vk == VK_END.0 as u32 => {}
        _ => {
            if handle_control_shortcut(vk_code, modifiers) {
                return;
            }

            if let Some(text) = translate_vk(vk_code, modifiers) {
                let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
                if !state.append_if_fits(&text) {
                    report_input_limit(state.session);
                    return;
                }
                drop(state);
                post_editor_sync();
            }
        }
    }
}

fn handle_control_shortcut(vk_code: u32, modifiers: Modifiers) -> bool {
    if !modifiers.ctrl || modifiers.alt || modifiers.win {
        return false;
    }

    match vk_code {
        0x56 => {
            if let Some(clipboard_text) = get_clipboard_text() {
                let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
                if !state.append_if_fits(&clipboard_text) {
                    report_input_limit(state.session);
                    return true;
                }
                drop(state);
                post_editor_sync();
            }
            true
        }
        _ => false,
    }
}

fn translate_vk(vk_code: u32, modifiers: Modifiers) -> Option<String> {
    if modifiers.ctrl || modifiers.alt {
        return None;
    }

    let mut keyboard_state = [0u8; 256];
    if modifiers.shift {
        keyboard_state[VK_SHIFT.0 as usize] = 0x80;
    }
    if (unsafe { GetKeyState(windows::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL.0 as i32) }
        & 0x0001)
        != 0
    {
        keyboard_state[windows::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL.0 as usize] = 0x01;
    }

    let layout = unsafe { GetKeyboardLayout(0) };
    let scan_code = unsafe { MapVirtualKeyW(vk_code, MAPVK_VK_TO_VSC) };
    let mut utf16 = [0u16; 8];
    let translated = unsafe {
        ToUnicodeEx(
            vk_code,
            scan_code,
            &keyboard_state,
            &mut utf16,
            0,
            Some(layout),
        )
    };

    if translated < 0 {
        let _ = unsafe {
            ToUnicodeEx(
                vk_code,
                scan_code,
                &keyboard_state,
                &mut utf16,
                0,
                Some(layout),
            )
        };
        return None;
    }

    if translated == 0 {
        return None;
    }

    String::from_utf16(&utf16[..translated as usize]).ok()
}

fn get_clipboard_text() -> Option<String> {
    if unsafe { OpenClipboard(Some(HWND::default())) }.is_err() {
        return None;
    }

    let mut result = None;
    if let Ok(h_data) = unsafe { GetClipboardData(13u32) } {
        let h_global: HGLOBAL = unsafe { std::mem::transmute(h_data) };
        let ptr = unsafe { GlobalLock(h_global) };
        if !ptr.is_null() {
            let size = unsafe { GlobalSize(h_global) };
            if size > 0 && size <= 2 * super::state::MAX_PENDING_TEXT_BYTES {
                let wide_slice = unsafe { std::slice::from_raw_parts(ptr as *const u16, size / 2) };
                result = decode_clipboard_wide(wide_slice);
            } else if size > 2 * super::state::MAX_PENDING_TEXT_BYTES {
                report_input_limit(EDITOR_SESSIONS.current());
            }
            let _ = unsafe { GlobalUnlock(h_global) };
        }
    }
    let _ = unsafe { CloseClipboard() };
    result
}

fn decode_clipboard_wide(wide: &[u16]) -> Option<String> {
    if wide.len() > super::state::MAX_PENDING_TEXT_BYTES {
        return None;
    }
    let end = wide.iter().position(|&value| value == 0)?;
    let text = String::from_utf16_lossy(&wide[..end]);
    (text.len() <= super::state::MAX_PENDING_TEXT_BYTES).then_some(text)
}

fn report_input_limit(session: Option<u64>) {
    let Some(session) = session else { return };
    let hwnd = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut _)),
                WM_APP_INPUT_LIMIT,
                WPARAM(session as usize),
                LPARAM(0),
            );
        }
    }
}

fn trim_last_word(text: &mut String) {
    while text.chars().last().is_some_and(|ch| ch.is_whitespace()) {
        text.pop();
    }

    while let Some(ch) = text.chars().last() {
        if ch.is_whitespace() {
            break;
        }
        text.pop();
    }
}

fn post_editor_sync() {
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd_val as *mut std::ffi::c_void)),
                WM_APP_SYNC_PASSIVE_EDITOR,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

fn wake_input_window() {
    let hwnd_val = INPUT_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd_val as *mut std::ffi::c_void)),
                WM_APP_SYNC_PASSIVE_EDITOR,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PassiveEditorState, decode_clipboard_wide};

    #[test]
    fn clipboard_decode_requires_bounded_terminated_text() {
        assert_eq!(decode_clipboard_wide(&[65, 0]), Some("A".into()));
        assert_eq!(decode_clipboard_wide(&[65]), None);
        assert_eq!(
            decode_clipboard_wide(&vec![0; super::super::state::MAX_PENDING_TEXT_BYTES + 1]),
            None
        );
    }

    #[test]
    fn late_clear_cannot_erase_reopened_or_newer_passive_input() {
        let mut state = PassiveEditorState {
            session: Some(7),
            revision: 3,
            text: "new".into(),
        };
        assert!(!state.clear_if_revision(5, 3));
        assert!(!state.clear_if_revision(7, 2));
        assert_eq!(state.text, "new");
        assert!(state.clear_if_revision(7, 3));
        assert_eq!(state.text, "");
        assert_eq!(state.revision, 4);
    }

    #[test]
    fn inserted_passive_text_survives_later_typing_and_old_clear() {
        let mut state = PassiveEditorState {
            session: Some(7),
            ..Default::default()
        };
        assert_eq!(
            state.apply(7, "inserted", false),
            Some(("inserted".into(), "1".into()))
        );
        assert!(state.append_if_fits(" plus"));
        assert_eq!(state.text, "inserted plus");
        assert!(!state.clear_if_revision(7, 1));
        assert_eq!(state.text.trim(), "inserted plus");
        assert!(state.apply(8, "wrong session", false).is_none());
    }

    #[test]
    fn passive_input_normalizes_crlf_and_lone_cr_like_a_textarea() {
        let mut state = PassiveEditorState {
            session: Some(7),
            ..Default::default()
        };
        assert!(state.append_if_fits("a\r\nb\rc"));
        assert_eq!(state.text, "a\nb\nc");
        assert_eq!(
            state.apply(7, "\r\nd\re", false).unwrap().0,
            "a\nb\nc\nd\ne"
        );
        assert!(state.replace_if_fits("x\ry\r\nz"));
        assert_eq!(state.text, "x\ny\nz");
        let before = state.text.clone();
        assert!(
            !state.replace_if_fits(&"\r\n".repeat(super::super::state::MAX_PENDING_TEXT_BYTES + 1))
        );
        assert_eq!(state.text, before);
    }
}
