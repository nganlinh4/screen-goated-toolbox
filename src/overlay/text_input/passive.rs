use super::state::{
    INPUT_HWND, PASSIVE_CAPTURE_ENABLED, SHOULD_CLOSE, SUBMITTED_TEXT, WM_APP_SYNC_PASSIVE_EDITOR,
};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::{HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, GetKeyboardLayout, MAPVK_VK_TO_VSC, MapVirtualKeyW, ToUnicodeEx, VK_BACK,
    VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LCONTROL, VK_LEFT, VK_LMENU,
    VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SHIFT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, IsWindowVisible, KBDLLHOOKSTRUCT, PostMessageW, WM_KEYDOWN,
    WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[derive(Default)]
struct PassiveEditorState {
    text: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
}

static PASSIVE_EDITOR_STATE: LazyLock<Mutex<PassiveEditorState>> =
    LazyLock::new(|| Mutex::new(PassiveEditorState::default()));

pub fn reset_state() {
    let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
    *state = PassiveEditorState::default();
}

pub fn sync_editor() {
    let text = PASSIVE_EDITOR_STATE.lock().unwrap().text.clone();
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
                    const editor = document.getElementById('editor');
                    if (!editor) return;
                    editor.value = `{}`;
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
        if code != HC_ACTION as i32 || !passive_capture_is_active() {
            return CallNextHookEx(None, code, wparam, lparam);
        }

        let event = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let message = wparam.0 as u32;

        match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                handle_key_down(event.vkCode);
                return LRESULT(1);
            }
            WM_KEYUP | WM_SYSKEYUP => {
                handle_key_up(event.vkCode);
                return LRESULT(1);
            }
            _ => {}
        }

        CallNextHookEx(None, code, wparam, lparam)
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

fn handle_key_down(vk_code: u32) {
    {
        let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
        if update_modifier_state(&mut state, vk_code, true) {
            return;
        }
    }

    match vk_code {
        vk if vk == VK_ESCAPE.0 as u32 => {
            crate::overlay::input_history::reset_history_navigation();
            *SHOULD_CLOSE.lock().unwrap() = true;
            wake_input_window();
        }
        vk if vk == VK_RETURN.0 as u32 => {
            let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
            if state.shift {
                state.text.push('\n');
                drop(state);
                post_editor_sync();
            } else {
                let submitted = state.text.trim().to_string();
                if !submitted.is_empty() {
                    crate::overlay::input_history::add_to_history(&submitted);
                    *SUBMITTED_TEXT.lock().unwrap() = Some(submitted);
                    *SHOULD_CLOSE.lock().unwrap() = true;
                    wake_input_window();
                }
            }
        }
        vk if vk == VK_BACK.0 as u32 => {
            let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
            if state.ctrl {
                trim_last_word(&mut state.text);
            } else {
                state.text.pop();
            }
            drop(state);
            post_editor_sync();
        }
        vk if vk == VK_DELETE.0 as u32 => {}
        vk if vk == VK_UP.0 as u32 => {
            let current = PASSIVE_EDITOR_STATE.lock().unwrap().text.clone();
            if let Some(text) = crate::overlay::input_history::navigate_history_up(&current) {
                let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
                state.text = text;
                drop(state);
                post_editor_sync();
            }
        }
        vk if vk == VK_DOWN.0 as u32 => {
            let current = PASSIVE_EDITOR_STATE.lock().unwrap().text.clone();
            if let Some(text) = crate::overlay::input_history::navigate_history_down(&current) {
                let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
                state.text = text;
                drop(state);
                post_editor_sync();
            }
        }
        vk if vk == VK_LEFT.0 as u32
            || vk == VK_RIGHT.0 as u32
            || vk == VK_HOME.0 as u32
            || vk == VK_END.0 as u32 => {}
        _ => {
            if handle_control_shortcut(vk_code) {
                return;
            }

            if let Some(text) = translate_vk(vk_code) {
                let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
                state.text.push_str(&text);
                drop(state);
                post_editor_sync();
            }
        }
    }
}

fn handle_key_up(vk_code: u32) {
    let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
    let _ = update_modifier_state(&mut state, vk_code, false);
}

fn update_modifier_state(state: &mut PassiveEditorState, vk_code: u32, is_down: bool) -> bool {
    match vk_code {
        vk if vk == VK_CONTROL.0 as u32
            || vk == VK_LCONTROL.0 as u32
            || vk == VK_RCONTROL.0 as u32 =>
        {
            state.ctrl = is_down;
            true
        }
        vk if vk == VK_MENU.0 as u32 || vk == VK_LMENU.0 as u32 || vk == VK_RMENU.0 as u32 => {
            state.alt = is_down;
            true
        }
        vk if vk == VK_SHIFT.0 as u32 || vk == VK_LSHIFT.0 as u32 || vk == VK_RSHIFT.0 as u32 => {
            state.shift = is_down;
            true
        }
        vk if vk == VK_LWIN.0 as u32 || vk == VK_RWIN.0 as u32 => {
            state.win = is_down;
            true
        }
        _ => false,
    }
}

fn handle_control_shortcut(vk_code: u32) -> bool {
    let (ctrl, alt, win) = {
        let state = PASSIVE_EDITOR_STATE.lock().unwrap();
        (state.ctrl, state.alt, state.win)
    };

    if !ctrl || alt || win {
        return false;
    }

    match vk_code {
        0x56 => {
            if let Some(clipboard_text) = get_clipboard_text() {
                let mut state = PASSIVE_EDITOR_STATE.lock().unwrap();
                state.text.push_str(&clipboard_text);
                drop(state);
                post_editor_sync();
            }
            true
        }
        _ => false,
    }
}

fn translate_vk(vk_code: u32) -> Option<String> {
    let (shift, ctrl, alt) = {
        let state = PASSIVE_EDITOR_STATE.lock().unwrap();
        (state.shift, state.ctrl, state.alt)
    };

    if ctrl || alt {
        return None;
    }

    let mut keyboard_state = [0u8; 256];
    if shift {
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
            let wide_slice = unsafe { std::slice::from_raw_parts(ptr as *const u16, size / 2) };
            if let Some(end) = wide_slice.iter().position(|&c| c == 0) {
                result = Some(String::from_utf16_lossy(&wide_slice[..end]));
            }
        }
        let _ = unsafe { GlobalUnlock(h_global) };
    }
    let _ = unsafe { CloseClipboard() };
    result
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
