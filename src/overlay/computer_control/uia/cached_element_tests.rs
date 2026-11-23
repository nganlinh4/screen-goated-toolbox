use super::super::enumeration::{Detail, enumerate_inner};
use super::*;
use std::sync::{Mutex, mpsc};
use std::time::Duration;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::CUIAutomation;
use windows::Win32::UI::WindowsAndMessaging::{
    BM_SETCHECK, BS_AUTOCHECKBOX, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    ES_AUTOHSCROLL, HMENU, LWA_ALPHA, MSG, PM_REMOVE, PeekMessageW, RegisterClassW,
    SW_SHOWNOACTIVATE, SendMessageW, SetLayeredWindowAttributes, ShowWindow, TranslateMessage,
    WINDOW_STYLE, WNDCLASSW, WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};
use windows::core::w;

fn cached(entries: Vec<(UIA_PROPERTY_ID, VARIANT)>) -> impl Fn(UIA_PROPERTY_ID) -> Option<VARIANT> {
    move |id| {
        entries
            .iter()
            .find(|(property, _)| *property == id)
            .map(|(_, value)| value.clone())
    }
}

#[test]
fn pattern_values_count_only_when_the_provider_reports_the_pattern() {
    let unavailable = cached(vec![
        (UIA_IsTogglePatternAvailablePropertyId, VARIANT::from(false)),
        (
            UIA_ToggleToggleStatePropertyId,
            VARIANT::from(ToggleState_On.0),
        ),
    ]);
    assert_eq!(state_from(&unavailable), None);

    let toggle = cached(vec![
        (UIA_IsTogglePatternAvailablePropertyId, VARIANT::from(true)),
        (
            UIA_ToggleToggleStatePropertyId,
            VARIANT::from(ToggleState_On.0),
        ),
    ]);
    assert_eq!(state_from(&toggle).as_deref(), Some("on"));

    let range = cached(vec![
        (
            UIA_IsRangeValuePatternAvailablePropertyId,
            VARIANT::from(true),
        ),
        (UIA_RangeValueValuePropertyId, VARIANT::from(30.004_f64)),
    ]);
    assert_eq!(state_from(&range).as_deref(), Some("value 30"));
}

#[test]
fn coerced_or_sentinel_values_are_not_trusted() {
    // A string "1" or an integer flag is not a typed toggle state or boolean.
    let coerced = cached(vec![
        (UIA_IsTogglePatternAvailablePropertyId, VARIANT::from(1_i32)),
        (
            UIA_ToggleToggleStatePropertyId,
            VARIANT::from(ToggleState_On.0),
        ),
        (
            UIA_IsSelectionItemPatternAvailablePropertyId,
            VARIANT::from(true),
        ),
        (UIA_SelectionItemIsSelectedPropertyId, VARIANT::from("true")),
    ]);
    assert_eq!(state_from(&coerced), None);
    assert!(!flag(Some(VARIANT::from(1_i32))));
    assert!(!flag(None));
}

unsafe extern "system" fn test_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// A fully transparent, click-through, never-activated window owned by a
/// pumping thread, holding a checked checkbox and a filled edit control.
struct FormWindow {
    hwnd: usize,
    release: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl FormWindow {
    fn spawn() -> Self {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        let thread = std::thread::spawn(move || unsafe {
            let instance = GetModuleHandleW(None).unwrap();
            let class = w!("SGTComputerControlCachedEnumerationTest");
            // A repeated registration from a parallel test fails harmlessly.
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(test_window_proc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("Cached enumeration test"),
                WS_POPUP,
                40,
                40,
                320,
                160,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();
            SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA).unwrap();
            let checkbox = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Remember me"),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                10,
                10,
                200,
                30,
                Some(hwnd),
                Some(HMENU(std::ptr::dangling_mut())),
                Some(instance.into()),
                None,
            )
            .unwrap();
            SendMessageW(checkbox, BM_SETCHECK, Some(WPARAM(1)), Some(LPARAM(0)));
            CreateWindowExW(
                Default::default(),
                w!("EDIT"),
                w!("hello"),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                10,
                60,
                200,
                30,
                Some(hwnd),
                Some(HMENU(2 as *mut _)),
                Some(instance.into()),
                None,
            )
            .unwrap();
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let mut ready = Some(ready_tx);
            while released.try_recv().is_err() {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                // Announce the window only once its creation messages are drained.
                if let Some(ready) = ready.take() {
                    ready.send(hwnd.0 as usize).unwrap();
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let _ = DestroyWindow(hwnd);
        });
        let hwnd = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        Self {
            hwnd,
            release,
            thread: Some(thread),
        }
    }
}

impl Drop for FormWindow {
    fn drop(&mut self) {
        let _ = self.release.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The real-window tests share one in-process UIA client and run one at a time.
static REAL_WINDOW: Mutex<()> = Mutex::new(());

/// Run on a fresh MTA thread with a hard cap, so a provider regression fails
/// instead of hanging the suite.
fn bounded<T: Send + 'static>(operation: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let _ = tx.send(operation());
    });
    rx.recv_timeout(Duration::from_secs(20))
        .expect("UIA enumeration of the test window did not finish")
}

#[test]
fn enumeration_reads_a_real_window_from_one_cached_snapshot() {
    let _serial = REAL_WINDOW
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let window = FormWindow::spawn();
    let hwnd = window.hwnd;
    let elements = bounded(move || enumerate_inner(hwnd, Detail::Values)).unwrap();
    let checkbox = elements
        .iter()
        .find(|element| element.name == "Remember me")
        .expect("checkbox enumerated");
    assert_eq!(checkbox.control_type, "CheckBox");
    assert_eq!(checkbox.state.as_deref(), Some("on"));
    assert!(checkbox.enabled);
    assert!(!checkbox.runtime_id.is_empty());
    let edit = elements
        .iter()
        .find(|element| element.control_type == "Edit")
        .expect("edit enumerated");
    assert!(edit.value.supported && edit.value.writable);
    // A single-line Win32 edit may expose no TextPattern; its text is then
    // unknown rather than invented.
    match edit.value.text.as_deref() {
        Some(text) => assert_eq!(text, "hello"),
        None => assert!(edit.value.incomplete),
    }
    assert!(!edit.protected);

    // Grid snapshots never fetch pattern state or text.
    let controls = bounded(move || enumerate_inner(hwnd, Detail::Controls)).unwrap();
    let checkbox = controls
        .iter()
        .find(|element| element.name == "Remember me")
        .expect("checkbox enumerated without values");
    assert_eq!(checkbox.state, None);
    drop(window);
}

#[test]
fn nodes_without_a_cache_are_rejected_instead_of_read_live() {
    let _serial = REAL_WINDOW
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let window = FormWindow::spawn();
    let hwnd = window.hwnd;
    let (cached_count, live_count, live_read) = bounded(move || unsafe {
        let uia: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).unwrap();
        let root = uia.ElementFromHandle(HWND(hwnd as *mut _)).unwrap();
        let cached = find_visible_descendants(&uia, &root, true).unwrap();
        let cached_count = (0..cached.Length().unwrap())
            .filter_map(|index| cached.GetElement(index).ok())
            .filter(|element| read(element, true).is_some())
            .count();
        let live = root
            .FindAll(TreeScope_Descendants, &uia.CreateTrueCondition().unwrap())
            .unwrap();
        let live_count = live.Length().unwrap();
        let live_read = (0..live_count)
            .filter_map(|index| live.GetElement(index).ok())
            .filter(|element| read(element, true).is_some())
            .count();
        (cached_count, live_count, live_read)
    });
    assert!(cached_count >= 2, "cached snapshot found {cached_count}");
    assert!(live_count >= 2, "plain FindAll found {live_count}");
    assert_eq!(live_read, 0, "uncached nodes must not be read");
    drop(window);
}
