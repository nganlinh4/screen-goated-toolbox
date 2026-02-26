// --- INPUT CAPTURE ---
// Low-level keyboard + mouse event capture.

use crossbeam_queue::ArrayQueue;
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
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
    hook_thread_id: u32,
    hook_thread: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy)]
enum QueuedEventKind {
    KeyboardDown,
    KeyboardUp,
    MouseButtonDown,
    MouseButtonUp,
    Wheel,
}

#[derive(Clone, Copy)]
struct QueuedInputEvent {
    kind: QueuedEventKind,
    timestamp_ns: u64,
    code: u32,
    modifiers: u8,
}

lazy_static::lazy_static! {
    static ref CAPTURE_STATE: Mutex<CaptureState> = Mutex::new(CaptureState::default());
    static ref EVENT_QUEUE: ArrayQueue<QueuedInputEvent> = ArrayQueue::new(MAX_CAPTURE_EVENTS);
}

const MAX_CAPTURE_EVENTS: usize = 250_000;
const MOD_CTRL_BIT: u8 = 1 << 0;
const MOD_ALT_BIT: u8 = 1 << 1;
const MOD_SHIFT_BIT: u8 = 1 << 2;
const MOD_WIN_BIT: u8 = 1 << 3;
const TRACKED_VK_WORDS: usize = 8; // 512 virtual keys

static IS_RUNNING: AtomicBool = AtomicBool::new(false);
static START_CAPTURE_UNIX_NS: AtomicU64 = AtomicU64::new(0);
static DROPPED_EVENTS: AtomicU64 = AtomicU64::new(0);
static MOUSE_BUTTONS_DOWN: AtomicU8 = AtomicU8::new(0);
static KEY_DOWN_BITS: [AtomicU64; TRACKED_VK_WORDS] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

pub fn start_capture() -> anyhow::Result<()> {
    {
        let mut state = CAPTURE_STATE.lock();
        if IS_RUNNING.load(Ordering::Acquire) {
            return Ok(());
        }
        state.hook_thread_id = 0;
        state.hook_thread = None;
    }
    clear_key_state_bits();
    MOUSE_BUTTONS_DOWN.store(0, Ordering::Relaxed);
    crate::overlay::screen_record::engine::IS_MOUSE_CLICKED.store(false, Ordering::SeqCst);
    while EVENT_QUEUE.pop().is_some() {}
    DROPPED_EVENTS.store(0, Ordering::Relaxed);
    START_CAPTURE_UNIX_NS.store(now_unix_nanos(), Ordering::Release);
    IS_RUNNING.store(true, Ordering::Release);

    let (ready_tx, ready_rx) = mpsc::channel::<anyhow::Result<()>>();
    let handle = thread::spawn(move || unsafe {
        let thread_id = GetCurrentThreadId();

        {
            let mut state = CAPTURE_STATE.lock();
            state.hook_thread_id = thread_id;
        }

        let h_instance = GetModuleHandleW(None).ok().map(Into::into);
        let keyboard_hook =
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), h_instance, 0);
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
                    state.hook_thread_id = 0;
                }
                IS_RUNNING.store(false, Ordering::Release);
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
            state.hook_thread_id = 0;
            state.hook_thread = None;
            IS_RUNNING.store(false, Ordering::Release);
            Err(err)
        }
        Err(_) => {
            IS_RUNNING.store(false, Ordering::Release);
            let _ = handle.join();
            let mut state = CAPTURE_STATE.lock();
            state.hook_thread_id = 0;
            state.hook_thread = None;
            Err(anyhow::anyhow!(
                "Timed out while starting key/mouse capture"
            ))
        }
    }
}

pub fn stop_capture_and_drain() -> Vec<RawInputEvent> {
    let (thread_id, handle_opt) = {
        let mut state = CAPTURE_STATE.lock();
        if !IS_RUNNING.load(Ordering::Acquire) && state.hook_thread.is_none() {
            clear_key_state_bits();
            return drain_events();
        }
        (state.hook_thread_id, state.hook_thread.take())
    };
    IS_RUNNING.store(false, Ordering::Release);

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
    clear_key_state_bits();
    MOUSE_BUTTONS_DOWN.store(0, Ordering::Relaxed);
    crate::overlay::screen_record::engine::IS_MOUSE_CLICKED.store(false, Ordering::SeqCst);
    let dropped = DROPPED_EVENTS.swap(0, Ordering::Relaxed);
    if dropped > 0 {
        eprintln!(
            "[KeyseeCapture] dropped {} hook events due to full queue",
            dropped
        );
    }
    drain_events()
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
            if !IS_RUNNING.load(Ordering::Relaxed) {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            if mark_key_down(vk) {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            let _ = push_queued_event(QueuedInputEvent {
                kind: QueuedEventKind::KeyboardDown,
                timestamp_ns: relative_timestamp_ns(),
                code: vk,
                modifiers: snapshot_modifiers_bits(),
            });
        }
        WM_KEYUP | WM_SYSKEYUP => {
            clear_key_down(vk);
            if !IS_RUNNING.load(Ordering::Relaxed) {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            let _ = push_queued_event(QueuedInputEvent {
                kind: QueuedEventKind::KeyboardUp,
                timestamp_ns: relative_timestamp_ns(),
                code: vk,
                modifiers: snapshot_modifiers_bits(),
            });
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
    if !IS_RUNNING.load(Ordering::Relaxed) {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let modifiers = snapshot_modifiers_bits();

    match message {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONUP
        | WM_MBUTTONUP => {
            let kind = match message {
                WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => QueuedEventKind::MouseButtonUp,
                _ => QueuedEventKind::MouseButtonDown,
            };

            let mask: u8 = match message {
                WM_LBUTTONDOWN | WM_LBUTTONUP => 1,
                WM_RBUTTONDOWN | WM_RBUTTONUP => 2,
                WM_MBUTTONDOWN | WM_MBUTTONUP => 4,
                _ => 0,
            };
            if mask != 0 {
                let is_down = matches!(kind, QueuedEventKind::MouseButtonDown);
                let mut current = MOUSE_BUTTONS_DOWN.load(Ordering::SeqCst);
                if is_down {
                    current |= mask;
                } else {
                    current &= !mask;
                }
                MOUSE_BUTTONS_DOWN.store(current, Ordering::SeqCst);
                crate::overlay::screen_record::engine::IS_MOUSE_CLICKED
                    .store(current != 0, Ordering::SeqCst);
            }

            let _ = push_queued_event(QueuedInputEvent {
                kind,
                timestamp_ns: relative_timestamp_ns(),
                code: message,
                modifiers,
            });
        }
        WM_MOUSEWHEEL => {
            let delta = ((mouse.mouseData >> 16) & 0xFFFF) as u16 as i16;
            let _ = push_queued_event(QueuedInputEvent {
                kind: QueuedEventKind::Wheel,
                timestamp_ns: relative_timestamp_ns(),
                code: delta as i32 as u32,
                modifiers,
            });
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

fn now_unix_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn relative_timestamp_ns() -> u64 {
    let start = START_CAPTURE_UNIX_NS.load(Ordering::Relaxed);
    if start == 0 {
        return 0;
    }
    now_unix_nanos().saturating_sub(start)
}

fn push_queued_event(event: QueuedInputEvent) -> bool {
    if EVENT_QUEUE.push(event).is_err() {
        DROPPED_EVENTS.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    true
}

fn drain_events() -> Vec<RawInputEvent> {
    let mut events = Vec::new();
    while let Some(event) = EVENT_QUEUE.pop() {
        if events.len() >= MAX_CAPTURE_EVENTS {
            break;
        }
        events.push(convert_event(event));
    }
    events
}

fn convert_event(event: QueuedInputEvent) -> RawInputEvent {
    let timestamp = event.timestamp_ns as f64 / 1_000_000_000.0;
    let modifiers = modifiers_from_bits(event.modifiers);
    match event.kind {
        QueuedEventKind::KeyboardDown => RawInputEvent {
            event_type: "keyboard",
            timestamp,
            vk: Some(event.code),
            key: vk_to_name(event.code),
            btn: None,
            direction: Some("down"),
            modifiers,
        },
        QueuedEventKind::KeyboardUp => RawInputEvent {
            event_type: "keyboard",
            timestamp,
            vk: Some(event.code),
            key: vk_to_name(event.code),
            btn: None,
            direction: Some("up"),
            modifiers,
        },
        QueuedEventKind::MouseButtonDown => RawInputEvent {
            event_type: "mousedown",
            timestamp,
            vk: None,
            key: None,
            btn: Some(mouse_button_name(event.code)),
            direction: Some("down"),
            modifiers,
        },
        QueuedEventKind::MouseButtonUp => RawInputEvent {
            event_type: "mousedown",
            timestamp,
            vk: None,
            key: None,
            btn: Some(mouse_button_name(event.code)),
            direction: Some("up"),
            modifiers,
        },
        QueuedEventKind::Wheel => {
            let delta = event.code as i32 as i16;
            let direction = if delta > 0 {
                "up"
            } else if delta < 0 {
                "down"
            } else {
                "none"
            };
            RawInputEvent {
                event_type: "wheel",
                timestamp,
                vk: None,
                key: None,
                btn: None,
                direction: Some(direction),
                modifiers,
            }
        }
    }
}

fn modifiers_from_bits(bits: u8) -> InputModifiers {
    InputModifiers {
        ctrl: (bits & MOD_CTRL_BIT) != 0,
        alt: (bits & MOD_ALT_BIT) != 0,
        shift: (bits & MOD_SHIFT_BIT) != 0,
        win: (bits & MOD_WIN_BIT) != 0,
    }
}

fn snapshot_modifiers_bits() -> u8 {
    let mut bits = 0u8;
    // VK_CONTROL (17), VK_LCONTROL (162), VK_RCONTROL (163)
    if key_is_down(17) || key_is_down(162) || key_is_down(163) {
        bits |= MOD_CTRL_BIT;
    }
    // VK_MENU (18), VK_LMENU (164), VK_RMENU (165)
    if key_is_down(18) || key_is_down(164) || key_is_down(165) {
        bits |= MOD_ALT_BIT;
    }
    // VK_SHIFT (16), VK_LSHIFT (160), VK_RSHIFT (161)
    if key_is_down(16) || key_is_down(160) || key_is_down(161) {
        bits |= MOD_SHIFT_BIT;
    }
    // VK_LWIN (91), VK_RWIN (92)
    if key_is_down(91) || key_is_down(92) {
        bits |= MOD_WIN_BIT;
    }
    bits
}

fn key_is_down(vk: u32) -> bool {
    let idx = (vk / 64) as usize;
    if idx >= TRACKED_VK_WORDS {
        return false;
    }
    let mask = 1u64 << (vk % 64);
    (KEY_DOWN_BITS[idx].load(Ordering::Relaxed) & mask) != 0
}

fn mark_key_down(vk: u32) -> bool {
    let idx = (vk / 64) as usize;
    if idx >= TRACKED_VK_WORDS {
        return false;
    }
    let mask = 1u64 << (vk % 64);
    (KEY_DOWN_BITS[idx].fetch_or(mask, Ordering::Relaxed) & mask) != 0
}

fn clear_key_down(vk: u32) {
    let idx = (vk / 64) as usize;
    if idx >= TRACKED_VK_WORDS {
        return;
    }
    let mask = !(1u64 << (vk % 64));
    KEY_DOWN_BITS[idx].fetch_and(mask, Ordering::Relaxed);
}

fn clear_key_state_bits() {
    for word in &KEY_DOWN_BITS {
        word.store(0, Ordering::Relaxed);
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
        21 => Some("한/영"),   // VK_HANGUL (Korean IME toggle) / VK_KANA (Japanese)
        23 => Some("전자"),    // VK_JUNJA
        25 => Some("한자"),    // VK_HANJA (Korean) / VK_KANJI (Japanese)
        28 => Some("変換"),    // VK_CONVERT (Japanese)
        29 => Some("無変換"),  // VK_NONCONVERT (Japanese)
        44 => Some("PrintScreen"),
        45 => Some("Insert"),
        46 => Some("Delete"),
        91 | 92 => Some("Win"),
        93 => Some("Menu"),
        144 => Some("NumLock"),
        145 => Some("ScrollLock"),
        19 => Some("Pause"),
        // Media keys
        173 => Some("Mute"),
        174 => Some("Vol-"),
        175 => Some("Vol+"),
        176 => Some("Next"),
        177 => Some("Prev"),
        178 => Some("Stop"),
        179 => Some("Play"),
        // Browser keys
        166 => Some("BrBack"),
        167 => Some("BrFwd"),
        168 => Some("Refresh"),
        // Japanese IME toggle keys
        243 => Some("英数"),
        244 => Some("ひらがな"),
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
