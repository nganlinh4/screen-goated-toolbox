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
    WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    WM_MBUTTONUP, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_QUIT,
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
    pub event_type: &'static str,
    pub timestamp: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vk: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub btn: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<&'static str>,
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

const INITIAL_CAPTURE_EVENT_CAPACITY: usize = 8_192;
const MAX_CAPTURE_EVENTS: usize = 250_000;

pub fn start_capture() -> anyhow::Result<()> {
    {
        let mut state = CAPTURE_STATE.lock();
        if state.running {
            return Ok(());
        }
        state.running = true;
        state.start_time = Some(Instant::now());
        state.events = Vec::with_capacity(INITIAL_CAPTURE_EVENT_CAPACITY);
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

fn capture_timestamp(state: &CaptureState) -> f64 {
    state
        .start_time
        .map(|t| t.elapsed().as_secs_f64())
        .unwrap_or(0.0)
}

fn push_event(state: &mut CaptureState, event: RawInputEvent) {
    if state.events.len() < MAX_CAPTURE_EVENTS {
        state.events.push(event);
    }
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

            let timestamp = capture_timestamp(&state);
            push_event(
                &mut state,
                RawInputEvent {
                    event_type: "keyboard",
                    timestamp,
                    vk: Some(vk),
                    key: vk_to_name(vk),
                    btn: None,
                    direction: Some("down"),
                    modifiers: get_modifiers(),
                },
            );
        }
        WM_KEYUP | WM_SYSKEYUP => {
            let mut state = CAPTURE_STATE.lock();
            state.known_key_state.remove(&vk);
            if !state.running {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            let timestamp = capture_timestamp(&state);
            push_event(
                &mut state,
                RawInputEvent {
                    event_type: "keyboard",
                    timestamp,
                    vk: Some(vk),
                    key: vk_to_name(vk),
                    btn: None,
                    direction: Some("up"),
                    modifiers: get_modifiers(),
                },
            );
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
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONUP
        | WM_MBUTTONUP => {
            let mut state = CAPTURE_STATE.lock();
            if !state.running {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            let timestamp = capture_timestamp(&state);
            let direction = match message {
                WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => "up",
                _ => "down",
            };
            push_event(
                &mut state,
                RawInputEvent {
                    event_type: "mousedown",
                    timestamp,
                    vk: None,
                    key: None,
                    btn: Some(mouse_button_name(message)),
                    direction: Some(direction),
                    modifiers,
                },
            );
        }
        WM_MOUSEWHEEL => {
            let delta = ((mouse.mouseData >> 16) & 0xFFFF) as u16 as i16;
            let direction = if delta > 0 {
                "up"
            } else if delta < 0 {
                "down"
            } else {
                "none"
            };

            let mut state = CAPTURE_STATE.lock();
            if !state.running {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            let timestamp = capture_timestamp(&state);
            push_event(
                &mut state,
                RawInputEvent {
                    event_type: "wheel",
                    timestamp,
                    vk: None,
                    key: None,
                    btn: None,
                    direction: Some(direction),
                    modifiers,
                },
            );
        }
        _ => {}
    }

    CallNextHookEx(None, code, wparam, lparam)
}

fn mouse_button_name(message: u32) -> &'static str {
    match message {
        WM_LBUTTONDOWN | WM_LBUTTONUP => "left",
        WM_RBUTTONDOWN | WM_RBUTTONUP => "right",
        WM_MBUTTONDOWN | WM_MBUTTONUP => "middle",
        _ => "",
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

fn vk_to_name(vk: u32) -> Option<&'static str> {
    match vk {
        8 => Some("Backspace"),
        9 => Some("Tab"),
        13 => Some("Enter"),
        20 => Some("CapsLock"),
        27 => Some("Esc"),
        32 => Some("Space"),
        33 => Some("PageUp"),
        34 => Some("PageDown"),
        35 => Some("End"),
        36 => Some("Home"),
        37 => Some("Left"),
        38 => Some("Up"),
        39 => Some("Right"),
        40 => Some("Down"),
        44 => Some("PrintScreen"),
        45 => Some("Insert"),
        46 => Some("Delete"),
        91 | 92 => Some("Win"),
        93 => Some("Menu"),
        144 => Some("NumLock"),
        145 => Some("ScrollLock"),
        19 => Some("Pause"),
        160 | 161 => Some("Shift"),
        162 | 163 => Some("Ctrl"),
        164 | 165 => Some("Alt"),
        48 => Some("0"),
        49 => Some("1"),
        50 => Some("2"),
        51 => Some("3"),
        52 => Some("4"),
        53 => Some("5"),
        54 => Some("6"),
        55 => Some("7"),
        56 => Some("8"),
        57 => Some("9"),
        65 => Some("A"),
        66 => Some("B"),
        67 => Some("C"),
        68 => Some("D"),
        69 => Some("E"),
        70 => Some("F"),
        71 => Some("G"),
        72 => Some("H"),
        73 => Some("I"),
        74 => Some("J"),
        75 => Some("K"),
        76 => Some("L"),
        77 => Some("M"),
        78 => Some("N"),
        79 => Some("O"),
        80 => Some("P"),
        81 => Some("Q"),
        82 => Some("R"),
        83 => Some("S"),
        84 => Some("T"),
        85 => Some("U"),
        86 => Some("V"),
        87 => Some("W"),
        88 => Some("X"),
        89 => Some("Y"),
        90 => Some("Z"),
        96 => Some("Num0"),
        97 => Some("Num1"),
        98 => Some("Num2"),
        99 => Some("Num3"),
        100 => Some("Num4"),
        101 => Some("Num5"),
        102 => Some("Num6"),
        103 => Some("Num7"),
        104 => Some("Num8"),
        105 => Some("Num9"),
        106 => Some("*"),
        107 => Some("+"),
        109 => Some("-"),
        110 => Some("."),
        111 => Some("/"),
        112 => Some("F1"),
        113 => Some("F2"),
        114 => Some("F3"),
        115 => Some("F4"),
        116 => Some("F5"),
        117 => Some("F6"),
        118 => Some("F7"),
        119 => Some("F8"),
        120 => Some("F9"),
        121 => Some("F10"),
        122 => Some("F11"),
        123 => Some("F12"),
        186 => Some(";"),
        187 => Some("="),
        188 => Some(","),
        189 => Some("-"),
        190 => Some("."),
        191 => Some("/"),
        192 => Some("`"),
        219 => Some("["),
        220 => Some("\\"),
        221 => Some("]"),
        222 => Some("'"),
        _ => None,
    }
}
