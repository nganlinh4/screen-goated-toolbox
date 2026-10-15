//! Transcript-free diagnostics for the batch clipboard paste path.

use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows::Win32::UI::WindowsAndMessaging::{
    GUITHREADINFO, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static ENABLED: OnceLock<bool> = OnceLock::new();

pub(crate) fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        cfg!(debug_assertions)
            || std::env::var("SGT_AUTOPASTE_BATCH_DIAGNOSTICS").as_deref() == Ok("1")
    })
}

pub(crate) fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn clipboard_sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

pub(crate) fn focus(id: u64, stage: &str, target: Option<HWND>) {
    if !enabled() {
        return;
    }
    let foreground = unsafe { GetForegroundWindow() };
    let foreground = describe(foreground);
    let target = target.map(describe).unwrap_or_else(|| "none".to_owned());
    crate::log_info!(
        "[AutoPasteBatch] id={id} stage={stage} foreground=({foreground}) target=({target}) clipboard_seq={}",
        clipboard_sequence()
    );
}

fn describe(hwnd: HWND) -> String {
    if hwnd.is_invalid() {
        return "invalid".to_owned();
    }
    let mut process = 0;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process)) };
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    let focus = if thread != 0 && unsafe { GetGUIThreadInfo(thread, &mut info) }.is_ok() {
        info.hwndFocus.0 as usize
    } else {
        0
    };
    let mut class = [0_u16; 128];
    let length = unsafe { GetClassNameW(hwnd, &mut class) };
    let class = String::from_utf16_lossy(&class[..length.max(0) as usize]);
    format!(
        "hwnd={:x} pid={process} thread={thread} focus={focus:x} class={class:?}",
        hwnd.0 as usize
    )
}
