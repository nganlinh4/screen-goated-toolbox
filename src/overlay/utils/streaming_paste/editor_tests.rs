use super::*;
use std::sync::mpsc::{self, Sender};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Controls::{EM_SETPASSWORDCHAR, EM_SETSEL};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

static ACCEPTANCE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[path = "editor_provider_tests.rs"]
mod provider_tests;

#[test]
#[ignore = "opens and types only into disposable owned native Edit controls"]
fn owned_edit_destination_handoff_acceptance() {
    use super::super::super::StreamingAutoPaste;
    use std::sync::{Arc, atomic::AtomicBool};
    let _exclusive = ACCEPTANCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let first = OwnedWindow::new();
    first.prepare("first ", 6);
    let session = StreamingAutoPaste::new(true, Arc::new(AtomicBool::new(false)));
    session.interim("old draft");
    first.wait_text("first old draft");
    let second = OwnedWindow::new();
    second.prepare("second ", 7);
    session.final_text("old corrected final");
    std::thread::sleep(Duration::from_millis(700));
    session.interim("new draft");
    second.wait_text("second new draft");
    session.final_text("new final");
    second.wait_text("second new final");
    session.finish();
    assert_eq!(first.read(), "first old draft");
    assert_eq!(second.read(), "second new final");
}

#[test]
#[ignore = "opens and types only into a disposable owned native Edit control"]
fn owned_edit_best_effort_acceptance() {
    let _exclusive = ACCEPTANCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    assert!(initialized);
    let _watch = super::super::input_activity::InputWatch::start().unwrap();
    let window = OwnedWindow::new();
    window.prepare("prefix ", 7);
    let target = BestEffortTarget::capture(unsafe { GetForegroundWindow() }).unwrap();
    target.replace("", "helo", &|| true).unwrap();
    window.wait_text("prefix helo");
    target.replace("helo", "hello 🦀", &|| true).unwrap();
    window.wait_text("prefix hello 🦀");
    assert!(target.replace("hello 🦀", " forbidden", &|| false).is_err());
    window.wait_text("prefix hello 🦀");
    unsafe { CoUninitialize() };
}

#[test]
#[ignore = "opens and types only into a disposable owned native Edit control"]
fn owned_edit_worker_acceptance_streams_and_stops_on_lost_ownership() {
    use super::super::super::StreamingAutoPaste;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let _exclusive = ACCEPTANCE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let window = OwnedWindow::new();
    window.prepare("prefix suffix", 7);
    let abort = Arc::new(AtomicBool::new(false));
    let session = StreamingAutoPaste::new(true, abort.clone());
    session.interim("helo");
    window.wait_text("prefix helosuffix");
    session.interim("hello 🦀");
    window.wait_text("prefix hello 🦀suffix");
    session.final_text("hello 🦀");
    session.interim(" next");
    window.wait_text("prefix hello 🦀 nextsuffix");
    session.final_text(" revised");
    window.wait_text("prefix hello 🦀 revisedsuffix");
    session.finish();
    window.wait_text("prefix hello 🦀 revisedsuffix");
    drop(session);

    window.prepare("keep ", 5);
    let session = StreamingAutoPaste::new(true, abort.clone());
    session.interim("draft");
    window.wait_text("keep draft");
    window.prepare("user replacement", 16);
    session.final_text("late final");
    session.finish();
    assert_eq!(window.read(), "user replacement");
    drop(session);

    window.prepare("keep ", 5);
    let session = StreamingAutoPaste::new(true, abort.clone());
    session.interim("provisional");
    window.wait_text("keep provisional");
    abort.store(true, Ordering::SeqCst);
    session.finish();
    window.wait_text("keep ");
    eprintln!(
        "[StreamingWorkerAcceptance] interim_before_final=true unicode_correction=true equal_final_rebase=true changed_final=true user_edit_preserved=true aborted_tail_removed=true"
    );
}

#[test]
fn input_batches_reject_controls_and_bound_utf16_not_utf8() {
    assert!(validate_input("hello 🦀 e\u{301}").is_ok());
    for text in ["\0", "\r", "\n", "\t", "\u{7f}"] {
        assert!(validate_input(text).is_err());
    }
    assert!(validate_input(&"🦀".repeat(MAX_INPUT_UNITS / 2)).is_ok());
    assert!(validate_input(&"🦀".repeat(MAX_INPUT_UNITS / 2 + 1)).is_err());
}

#[test]
fn append_requires_positive_writable_capability() {
    assert!(append_writable(Some(false), None));
    assert!(append_writable(None, Some(false)));
    assert!(!append_writable(Some(true), Some(false)));
    assert!(!append_writable(None, Some(true)));
    assert!(!append_writable(None, None));
}

enum Action {
    SetText(String),
    Select(usize, usize),
    FocusOther,
    Password,
    Read(Sender<String>),
    Stop,
}

struct OwnedWindow {
    hwnd: HWND,
    actions: Sender<(Action, Sender<()>)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl OwnedWindow {
    fn new() -> Self {
        let (ready, receive) = mpsc::channel();
        let (actions, requests) = mpsc::channel::<(Action, Sender<()>)>();
        let thread = std::thread::spawn(move || unsafe {
            let top = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("Owned streaming input acceptance"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100,
                100,
                640,
                220,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let edit = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                12,
                12,
                580,
                45,
                Some(top),
                None,
                None,
                None,
            )
            .unwrap();
            let other = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                w!("other"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                12,
                72,
                580,
                45,
                Some(top),
                None,
                None,
                None,
            )
            .unwrap();
            let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let thread_id = GetCurrentThreadId();
            let attached = foreground_thread != 0
                && foreground_thread != thread_id
                && AttachThreadInput(thread_id, foreground_thread, true).as_bool();
            let _ = ShowWindow(top, SW_SHOW);
            let _ = BringWindowToTop(top);
            let _ = SetForegroundWindow(top);
            let _ = SetFocus(Some(edit));
            if attached {
                let _ = AttachThreadInput(thread_id, foreground_thread, false);
            }
            ready.send(top.0 as isize).unwrap();
            loop {
                let mut message = MSG::default();
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                if let Ok((action, done)) = requests.try_recv() {
                    match action {
                        Action::SetText(text) => {
                            let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
                            SetWindowTextW(edit, PCWSTR(wide.as_ptr())).unwrap();
                            let _ = SetFocus(Some(edit));
                        }
                        Action::Select(start, end) => {
                            SendMessageW(
                                edit,
                                EM_SETSEL,
                                Some(WPARAM(start)),
                                Some(LPARAM(end as isize)),
                            );
                        }
                        Action::FocusOther => {
                            let _ = SetFocus(Some(other));
                        }
                        Action::Password => {
                            SendMessageW(
                                edit,
                                EM_SETPASSWORDCHAR,
                                Some(WPARAM('*' as usize)),
                                Some(LPARAM(0)),
                            );
                        }
                        Action::Read(reply) => {
                            let mut text = [0_u16; 4096];
                            let length = GetWindowTextW(edit, &mut text);
                            reply
                                .send(String::from_utf16(&text[..length as usize]).unwrap())
                                .unwrap();
                        }
                        Action::Stop => {
                            DestroyWindow(top).unwrap();
                            let _ = done.send(());
                            break;
                        }
                    }
                    let _ = done.send(());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        Self {
            hwnd: HWND(receive.recv_timeout(Duration::from_secs(3)).unwrap() as *mut _),
            actions,
            thread: Some(thread),
        }
    }

    fn act(&self, action: Action) {
        let (send, wait) = mpsc::channel();
        self.actions.send((action, send)).unwrap();
        wait.recv_timeout(Duration::from_secs(3)).unwrap();
    }

    fn prepare(&self, text: &str, caret: usize) {
        self.act(Action::SetText(text.into()));
        self.act(Action::Select(caret, caret));
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut tick = input_epoch();
        let mut stable = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(20));
            let current = input_epoch();
            if current != tick {
                eprintln!(
                    "[StreamingEditorAcceptance] setup_input_changed previous={tick} current={current}"
                );
                tick = current;
                stable = Instant::now();
            }
            if stable.elapsed() >= Duration::from_millis(200) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "external desktop input did not become idle for owned editor test"
            );
        }
    }

    fn read(&self) -> String {
        let (send, wait) = mpsc::channel();
        self.act(Action::Read(send));
        wait.recv_timeout(Duration::from_secs(3)).unwrap()
    }

    fn wait_text(&self, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let actual = self.read();
            if actual == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "owned editor mismatch: expected_bytes={} actual_bytes={}",
                expected.len(),
                actual.len()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for OwnedWindow {
    fn drop(&mut self) {
        self.act(Action::Stop);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

#[test]
#[ignore = "opens and types only into a disposable owned native Edit control"]
fn owned_edit_acceptance_preserves_text_focus_selection_and_unicode() {
    let _exclusive = ACCEPTANCE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _input_watch = super::super::input_activity::InputWatch::start().unwrap();
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
    }
    let window = OwnedWindow::new();
    window.prepare("prefix suffix", 7);
    let mut editor = Editor::capture(window.hwnd).unwrap();
    assert!(editor.replace("prefix ", "bad", &|| true).is_err());
    assert_eq!(window.read(), "prefix suffix");
    editor.replace("", "hello", &|| true).unwrap();
    editor.replace("hello", "help 🦀", &|| true).unwrap();
    assert_eq!(window.read(), "prefix help 🦀suffix");
    editor.check().unwrap();
    // A committed chunk needs no range rebasing: empty old tail appends.
    editor.replace("", " next", &|| true).unwrap();
    assert_eq!(window.read(), "prefix help 🦀 nextsuffix");
    editor.replace(" next", "", &|| true).unwrap();
    assert_eq!(window.read(), "prefix help 🦀suffix");
    window.act(Action::Select(0, 6));
    assert!(editor.check().is_err());
    assert!(editor.replace("", "bad", &|| true).is_err());
    assert!(AppendTarget::capture(window.hwnd).is_err());
    assert_eq!(window.read(), "prefix help 🦀suffix");
    drop(editor);

    window.prepare("keep ", 5);
    let mut editor = Editor::capture(window.hwnd).unwrap();
    editor.replace("", "draft", &|| true).unwrap();
    let guard_calls = std::cell::Cell::new(0);
    assert!(
        editor
            .replace("draft", "late", &|| {
                let count = guard_calls.get() + 1;
                guard_calls.set(count);
                count < 3
            })
            .is_err()
    );
    assert_eq!(guard_calls.get(), 3);
    assert_eq!(window.read(), "keep draft");
    drop(editor);

    window.prepare("original", 8);
    let mut editor = Editor::capture(window.hwnd).unwrap();
    window.prepare("external edit", 13);
    assert!(editor.replace("", "bad", &|| true).is_err());
    assert_eq!(window.read(), "external edit");
    drop(editor);

    window.prepare("keep", 4);
    let mut editor = Editor::capture(window.hwnd).unwrap();
    window.act(Action::FocusOther);
    assert!(editor.replace("", "bad", &|| true).is_err());
    assert_eq!(window.read(), "keep");
    drop(editor);

    window.prepare("secret", 6);
    window.act(Action::Password);
    assert!(Editor::capture(window.hwnd).is_err());
    assert!(AppendTarget::capture(window.hwnd).is_err());
    assert_eq!(window.read(), "secret");
    drop(window);
    unsafe {
        CoUninitialize();
    }
    eprintln!(
        "[StreamingEditorAcceptance] native_edit=true correction=true unicode=true deletion=true external_edit_preserved=true moved_selection_preserved=true focus_switch_preserved=true password_rejected=true"
    );
}
