// --- KEYSEE CAPTURE ---
// Low-level keyboard + mouse event capture, ported from Keysee semantics.

use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Instant;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_MOUSEWHEEL,
    WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_QUIT,
};

#[derive(Debug, Clone, Serialize)]
pub struct InputModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawInputEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vk: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub btn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    pub modifiers: InputModifiers,
}

#[derive(Default)]
struct CaptureState {
    running: bool,
    start_time: Option<Instant>,
    events: Vec<RawInputEvent>,
    known_key_state: HashSet<u32>,
    hook_thread_id: u32,
    hook_thread: Option<JoinHandle<()>>,
}

lazy_static::lazy_static! {
    static ref CAPTURE_STATE: Mutex<CaptureState> = Mutex::new(CaptureState::default());
}

pub fn start_capture() -> anyhow::Result<()> {
    {
        let mut state = CAPTURE_STATE.lock();
        if state.running {
            return Ok(());
        }
        state.running = true;
        state.start_time = Some(Instant::now());
        state.events.clear();
        state.known_key_state.clear();
        state.hook_thread_id = 0;
        state.hook_thread = None;
    }

    let (ready_tx, ready_rx) = mpsc::channel::<anyhow::Result<()>>();
    let handle = thread::spawn(move || unsafe {
        let thread_id = GetCurrentThreadId();

        {
            let mut state = CAPTURE_STATE.lock();
            state.hook_thread_id = thread_id;
        }

        let h_instance = GetModuleHandleW(None).ok().map(Into::into);
        let keyboard_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), h_instance, 0);
        let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), h_instance, 0);

        let (kb_hook, mouse_hook) = match (keyboard_hook.ok(), mouse_hook.ok()) {
            (Some(kb), Some(ms)) => {
                let _ = ready_tx.send(Ok(()));
                (kb, ms)
            }
            (kb, ms) => {
                if let Some(h) = kb {
                    let _ = UnhookWindowsHookEx(h);
                }
                if let Some(h) = ms {
                    let _ = UnhookWindowsHookEx(h);
                }
                {
                    let mut state = CAPTURE_STATE.lock();
                    state.running = false;
                    state.hook_thread_id = 0;
                }
                let _ = ready_tx.send(Err(anyhow::anyhow!(
                    "Failed to install low-level keyboard/mouse hooks"
                )));
                return;
            }
        };

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if msg.message == WM_QUIT {
                break;
            }
        }

        let _ = UnhookWindowsHookEx(kb_hook);
        let _ = UnhookWindowsHookEx(mouse_hook);
    });

    match ready_rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(Ok(())) => {
            let mut state = CAPTURE_STATE.lock();
            state.hook_thread = Some(handle);
            Ok(())
        }
        Ok(Err(err)) => {
            let _ = handle.join();
            let mut state = CAPTURE_STATE.lock();
            state.running = false;
            state.hook_thread_id = 0;
            state.hook_thread = None;
            Err(err)
        }
        Err(_) => {
            {
                let mut state = CAPTURE_STATE.lock();
                state.running = false;
            }
            let _ = handle.join();
            let mut state = CAPTURE_STATE.lock();
            state.hook_thread_id = 0;
            state.hook_thread = None;
            Err(anyhow::anyhow!("Timed out while starting key/mouse capture"))
        }
    }
}

pub fn stop_capture_and_drain() -> Vec<RawInputEvent> {
    let (thread_id, handle_opt) = {
        let mut state = CAPTURE_STATE.lock();
        if !state.running && state.hook_thread.is_none() {
            state.known_key_state.clear();
            state.start_time = None;
            return std::mem::take(&mut state.events);
        }
        state.running = false;
        state.start_time = None;
        state.known_key_state.clear();
        (state.hook_thread_id, state.hook_thread.take())
    };

    if thread_id != 0 {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                thread_id,
                WM_QUIT,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }

    if let Some(handle) = handle_opt {
        let _ = handle.join();
    }

    let mut state = CAPTURE_STATE.lock();
    state.hook_thread_id = 0;
    state.running = false;
    std::mem::take(&mut state.events)
}

unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let kbd = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let vk = kbd.vkCode;
    let message = wparam.0 as u32;

    match message {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let mut state = CAPTURE_STATE.lock();
            if !state.running {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            if state.known_key_state.contains(&vk) {
                // Some apps/games may swallow KEYUP events. If the key is no
                // longer physically down, recover instead of suppressing this
                // key forever for the rest of the recording.
                if !key_down(vk as i32) {
                    state.known_key_state.remove(&vk);
                } else {
                    return CallNextHookEx(None, code, wparam, lparam);
                }
            }
            state.known_key_state.insert(vk);

            let timestamp = state
                .start_time
                .map(|t| t.elapsed().as_secs_f64())
                .unwrap_or(0.0);

            state.events.push(RawInputEvent {
                event_type: "keyboard".to_string(),
                timestamp,
                vk: Some(vk),
                key: Some(vk_to_name(vk)),
                btn: None,
                direction: None,
                modifiers: get_modifiers(),
            });
        }
        WM_KEYUP | WM_SYSKEYUP => {
            let mut state = CAPTURE_STATE.lock();
            state.known_key_state.remove(&vk);
        }
        _ => {}
    }

    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let mouse = &*(lparam.0 as *const MSLLHOOKSTRUCT);
    let message = wparam.0 as u32;
    let modifiers = get_modifiers();

    match message {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            let mut state = CAPTURE_STATE.lock();
            if !state.running {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            let timestamp = state
                .start_time
                .map(|t| t.elapsed().as_secs_f64())
                .unwrap_or(0.0);

            state.events.push(RawInputEvent {
                event_type: "mousedown".to_string(),
                timestamp,
                vk: None,
                key: None,
                btn: Some(mouse_button_name(message)),
                direction: None,
                modifiers,
            });
        }
        WM_MOUSEWHEEL => {
            let delta = ((mouse.mouseData >> 16) & 0xFFFF) as u16 as i16;
            let direction = if delta > 0 {
                "up".to_string()
            } else if delta < 0 {
                "down".to_string()
            } else {
                "none".to_string()
            };

            let mut state = CAPTURE_STATE.lock();
            if !state.running {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            let timestamp = state
                .start_time
                .map(|t| t.elapsed().as_secs_f64())
                .unwrap_or(0.0);

            state.events.push(RawInputEvent {
                event_type: "wheel".to_string(),
                timestamp,
                vk: None,
                key: None,
                btn: None,
                direction: Some(direction),
                modifiers,
            });
        }
        _ => {}
    }

    CallNextHookEx(None, code, wparam, lparam)
}

fn mouse_button_name(message: u32) -> String {
    match message {
        WM_LBUTTONDOWN => "left".to_string(),
        WM_RBUTTONDOWN => "right".to_string(),
        WM_MBUTTONDOWN => "middle".to_string(),
        _ => String::new(),
    }
}

fn key_down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

fn get_modifiers() -> InputModifiers {
    InputModifiers {
        ctrl: key_down(VK_CONTROL.0 as i32),
        alt: key_down(VK_MENU.0 as i32),
        shift: key_down(VK_SHIFT.0 as i32),
        win: key_down(VK_LWIN.0 as i32) || key_down(VK_RWIN.0 as i32),
    }
}

fn vk_to_name(vk: u32) -> String {
    match vk {
        8 => "Backspace".to_string(),
        9 => "Tab".to_string(),
        13 => "Enter".to_string(),
        20 => "CapsLock".to_string(),
        27 => "Esc".to_string(),
        32 => "Space".to_string(),
        33 => "PageUp".to_string(),
        34 => "PageDown".to_string(),
        35 => "End".to_string(),
        36 => "Home".to_string(),
        37 => "Left".to_string(),
        38 => "Up".to_string(),
        39 => "Right".to_string(),
        40 => "Down".to_string(),
        44 => "PrintScreen".to_string(),
        45 => "Insert".to_string(),
        46 => "Delete".to_string(),
        91 | 92 => "Win".to_string(),
        93 => "Menu".to_string(),
        144 => "NumLock".to_string(),
        145 => "ScrollLock".to_string(),
        19 => "Pause".to_string(),
        160 | 161 => "Shift".to_string(),
        162 | 163 => "Ctrl".to_string(),
        164 | 165 => "Alt".to_string(),
        48..=57 => char::from_u32(vk).unwrap_or('?').to_string(),
        65..=90 => char::from_u32(vk).unwrap_or('?').to_string(),
        96..=105 => format!("Num{}", vk - 96),
        106 => "*".to_string(),
        107 => "+".to_string(),
        109 => "-".to_string(),
        110 => ".".to_string(),
        111 => "/".to_string(),
        112..=123 => format!("F{}", vk - 111),
        186 => ";".to_string(),
        187 => "=".to_string(),
        188 => ",".to_string(),
        189 => "-".to_string(),
        190 => ".".to_string(),
        191 => "/".to_string(),
        192 => "`".to_string(),
        219 => "[".to_string(),
        220 => "\\".to_string(),
        221 => "]".to_string(),
        222 => "'".to_string(),
        _ => format!("VK_{}", vk),
    }
}
