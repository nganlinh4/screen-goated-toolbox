use super::dcomp::DcompHost;
use super::mailbox::{CommandBuffer, PushResult};
use super::protocol::{ChildEvent, HostCommand, RecordingScene, StatusSnapshot};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{LazyLock, Mutex, Once};
use webview2_com::ExecuteScriptCompletedHandler;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, w};

const WM_DRAIN_COMMANDS: u32 = WM_APP + 81;
static HOST_HWND: AtomicIsize = AtomicIsize::new(0);
static INPUT_HWND: AtomicIsize = AtomicIsize::new(0);
static RENDERER_READY: AtomicBool = AtomicBool::new(false);
static SCRIPT_RESYNC_PENDING: AtomicBool = AtomicBool::new(false);
static REGISTER_CLASS: Once = Once::new();
static COMMANDS: LazyLock<Mutex<CommandBuffer>> =
    LazyLock::new(|| Mutex::new(CommandBuffer::default()));
static SCENE: LazyLock<Mutex<StatusSnapshot>> =
    LazyLock::new(|| Mutex::new(StatusSnapshot::default()));
static STDOUT: LazyLock<Mutex<std::io::Stdout>> = LazyLock::new(|| Mutex::new(std::io::stdout()));

thread_local! {
    static HOST: RefCell<Option<DcompHost>> = const { RefCell::new(None) };
}

pub(super) fn run() -> anyhow::Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    let hwnd = create_window()?;
    HOST_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    let input_hwnd = create_input_window()?;
    INPUT_HWND.store(input_hwnd.0 as isize, Ordering::SeqCst);
    start_input_thread();
    let host = super::dcomp::build_host(hwnd).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    HOST.with(|slot| *slot.borrow_mut() = Some(host));

    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    HOST.with(|slot| *slot.borrow_mut() = None);
    unsafe {
        let _ = DestroyWindow(input_hwnd);
    }
    INPUT_HWND.store(0, Ordering::SeqCst);
    HOST_HWND.store(0, Ordering::SeqCst);
    unsafe { CoUninitialize() };
    Ok(())
}

fn create_window() -> anyhow::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("SGTStatusCompositorDComp");
        REGISTER_CLASS.call_once(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            RegisterClassW(&class);
        });
        let (x, y, width, height) = super::virtual_screen();
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP
                | WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE
                | WS_EX_LAYERED
                | WS_EX_TRANSPARENT,
            class_name,
            w!("SGT Status Compositor"),
            WS_POPUP,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;
        Ok(hwnd)
    }
}

fn create_input_window() -> anyhow::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("SGTStatusCompositorInput");
        let class = WNDCLASSW {
            lpfnWndProc: Some(input_window_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            ..Default::default()
        };
        RegisterClassW(&class);
        Ok(CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("SGT Status Input"),
            WS_POPUP,
            -32_000,
            -32_000,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        )?)
    }
}

fn start_input_thread() {
    std::thread::spawn(|| {
        for line in BufReader::new(std::io::stdin())
            .lines()
            .map_while(Result::ok)
        {
            let Ok(command) = serde_json::from_str::<HostCommand>(&line) else {
                continue;
            };
            match COMMANDS.lock().unwrap().push(command) {
                PushResult::Queued => post_host(WM_DRAIN_COMMANDS),
                PushResult::Overflowed => emit_event(ChildEvent::ResyncRequested),
                PushResult::AwaitingSnapshot => {}
            }
        }
        post_host(WM_CLOSE);
    });
}

fn post_host(message: u32) {
    let value = HOST_HWND.load(Ordering::SeqCst);
    if value == 0 {
        return;
    }
    unsafe {
        let _ = PostMessageW(
            Some(HWND(value as *mut std::ffi::c_void)),
            message,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_DRAIN_COMMANDS => {
                drain_commands(hwnd);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
                resize_host(hwnd);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

unsafe extern "system" fn input_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_MOUSEMOVE if super::input::active() => {
                move_recording_drag();
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                update_recording_pointer_feedback();
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                begin_recording_interaction(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                finish_recording_interaction();
                LRESULT(0)
            }
            WM_CAPTURECHANGED | WM_CANCELMODE => {
                cancel_recording_interaction();
                LRESULT(0)
            }
            WM_SETCURSOR => set_recording_cursor(hwnd, message, wparam, lparam),
            WM_ERASEBKGND => LRESULT(1),
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

fn drain_commands(hwnd: HWND) {
    if !RENDERER_READY.load(Ordering::SeqCst) {
        return;
    }
    let commands = COMMANDS.lock().unwrap().drain();
    for command in commands {
        if matches!(command, HostCommand::Shutdown) {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
        apply_native_state(&command);
        if let Ok(json) = serde_json::to_string(&command) {
            execute_script(&format!("window.applyStatusCommand({json});"));
        }
    }
}

fn apply_native_state(command: &HostCommand) {
    let mut scene = SCENE.lock().unwrap();
    match command {
        HostCommand::Snapshot { scene: snapshot } => *scene = snapshot.clone(),
        HostCommand::Theme { is_dark } => scene.is_dark = *is_dark,
        HostCommand::RecordingPrepare { scene: recording } => {
            scene.recording = Some(recording.clone())
        }
        HostCommand::RecordingShow { rect } => {
            scene.recording = Some(RecordingScene {
                rect: *rect,
                visible: true,
                state: "warmup".to_string(),
                rms: 0.0,
            });
        }
        HostCommand::RecordingUpdate { state, rms } => {
            if let Some(recording) = scene.recording.as_mut() {
                recording.state.clone_from(state);
                recording.rms = *rms;
            }
        }
        HostCommand::RecordingHide => {
            if let Some(recording) = scene.recording.as_mut() {
                recording.visible = false;
            }
        }
        HostCommand::ProgressUpsert { rect, progress } => {
            scene.notification_rect = *rect;
            scene.progress = Some(progress.clone());
        }
        HostCommand::ProgressRemove | HostCommand::ProgressRemoveBeforeCapture { .. } => {
            scene.progress = None
        }
        HostCommand::SelectionShow { rect, text } => {
            scene.selection.rect = *rect;
            scene.selection.text_visible = true;
            scene.selection.selecting = false;
            scene.selection.text.clone_from(text);
        }
        HostCommand::SelectionHide => scene.selection.text_visible = false,
        HostCommand::SelectionUpdate { selecting, text } => {
            scene.selection.selecting = *selecting;
            scene.selection.text.clone_from(text);
        }
        HostCommand::SelectionPosition { rect } => scene.selection.rect = *rect,
        HostCommand::ImageBadgeShow { rect, text } => {
            scene.selection.rect = *rect;
            scene.selection.image_visible = true;
            scene.selection.image_text.clone_from(text);
        }
        HostCommand::ImageBadgeHide => scene.selection.image_visible = false,
        HostCommand::SelectionCapture { visible, .. } => scene.selection.capture_visible = *visible,
        HostCommand::NotificationAdd { rect, .. } => scene.notification_rect = *rect,
        HostCommand::Shutdown => {}
    }
    let recording = scene.recording.clone();
    drop(scene);
    sync_input_window(recording.as_ref());
}

pub(super) fn handle_renderer_message(body: &str) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return;
    };
    if value.get("type").and_then(|item| item.as_str()) == Some("recording_regions") {
        let pause = value
            .get("pause")
            .cloned()
            .and_then(|rect| serde_json::from_value(rect).ok());
        let cancel = value
            .get("cancel")
            .cloned()
            .and_then(|rect| serde_json::from_value(rect).ok());
        if let (Some(pause), Some(cancel)) = (pause, cancel) {
            super::input::set_button_regions(pause, cancel);
        }
        return;
    }
    let Ok(event) = serde_json::from_value::<ChildEvent>(value) else {
        return;
    };
    if event == ChildEvent::Ready {
        RENDERER_READY.store(true, Ordering::SeqCst);
        let value = HOST_HWND.load(Ordering::SeqCst);
        if value != 0 {
            unsafe {
                let hwnd = HWND(value as *mut std::ffi::c_void);
                let display = super::display_metrics(hwnd);
                execute_script(&format!(
                    "window.statusDisplayChanged({{x:{},y:{},width:{},height:{},scale:{}}});",
                    display.x, display.y, display.width, display.height, display.scale
                ));
                drain_commands(hwnd);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }
    emit_event(event);
}

fn move_recording_drag() {
    let Some(rect) = super::input::update() else {
        return;
    };
    if let Some(recording) = SCENE.lock().unwrap().recording.as_mut() {
        recording.rect = rect;
    }
    sync_input_window(Some(&RecordingScene {
        rect,
        visible: true,
        state: String::new(),
        rms: 0.0,
    }));
    if let Ok(rect) = serde_json::to_string(&rect) {
        execute_script(&format!("window.moveStatusRecording({rect});"));
    }
}

fn finish_recording_drag() -> bool {
    if !super::input::finish() {
        return false;
    }
    if let Some(recording) = SCENE.lock().unwrap().recording.as_ref() {
        emit_event(ChildEvent::RecordingMoved {
            rect: recording.rect,
        });
    }
    true
}

fn begin_recording_interaction(hwnd: HWND) {
    if let Some(target) = super::input::target_at_cursor() {
        super::input::begin_button(hwnd, target);
    } else if let Some(rect) = SCENE
        .lock()
        .unwrap()
        .recording
        .as_ref()
        .map(|recording| recording.rect)
    {
        super::input::begin(hwnd, rect);
    }
    update_recording_pointer_feedback();
}

fn finish_recording_interaction() {
    if super::input::active() {
        finish_recording_drag();
    } else if super::input::button_pressed() {
        match super::input::finish_button() {
            Some(super::input::RecordingTarget::Pause) => {
                emit_event(ChildEvent::RecordingPauseToggle)
            }
            Some(super::input::RecordingTarget::Cancel) => emit_event(ChildEvent::RecordingCancel),
            None => {}
        }
    }
    update_recording_pointer_feedback();
}

fn cancel_recording_interaction() {
    finish_recording_drag();
    super::input::cancel_button();
    update_recording_pointer_feedback();
}

fn update_recording_pointer_feedback() {
    let Some((hovered, active)) = super::input::feedback_change() else {
        return;
    };
    let hovered = match hovered {
        Some(super::input::RecordingTarget::Pause) => "pause",
        Some(super::input::RecordingTarget::Cancel) => "cancel",
        None => "",
    };
    execute_script(&format!(
        "window.setStatusRecordingPointer({hovered:?},{active});"
    ));
}

fn execute_script(script: &str) {
    HOST.with(|slot| {
        if let Some(host) = slot.borrow().as_ref() {
            let script = HSTRING::from(script);
            let handler = ExecuteScriptCompletedHandler::create(Box::new(|code, _| {
                if code.is_ok() {
                    SCRIPT_RESYNC_PENDING.store(false, Ordering::SeqCst);
                } else {
                    request_script_resync();
                }
                Ok(())
            }));
            unsafe {
                if host
                    .webview
                    .ExecuteScript(PCWSTR(script.as_ptr()), &handler)
                    .is_err()
                {
                    request_script_resync();
                }
            }
        }
    });
}

fn request_script_resync() {
    if !SCRIPT_RESYNC_PENDING.swap(true, Ordering::SeqCst) {
        emit_event(ChildEvent::ResyncRequested);
    }
}

pub(super) fn emit_event(event: ChildEvent) {
    let mut stdout = STDOUT.lock().unwrap();
    if serde_json::to_writer(&mut *stdout, &event).is_ok() {
        let _ = stdout.write_all(b"\n");
        let _ = stdout.flush();
    }
}

fn sync_input_window(recording: Option<&RecordingScene>) {
    let value = INPUT_HWND.load(Ordering::SeqCst);
    if value == 0 {
        return;
    }
    let hwnd = HWND(value as *mut std::ffi::c_void);
    unsafe {
        if let Some(recording) = recording.filter(|recording| recording.visible) {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                recording.rect.x,
                recording.rect.y,
                recording.rect.width.max(1),
                recording.rect.height.max(1),
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        } else {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

unsafe fn set_recording_cursor(
    _hwnd: HWND,
    _message: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    let resource = if super::input::target_at_cursor().is_some() {
        IDC_HAND
    } else {
        IDC_ARROW
    };
    unsafe { SetCursor(Some(LoadCursorW(None, resource).unwrap_or_default())) };
    LRESULT(1)
}

fn resize_host(hwnd: HWND) {
    let display = super::display_metrics(hwnd);
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            display.x,
            display.y,
            display.width,
            display.height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    HOST.with(|slot| {
        if let Some(host) = slot.borrow().as_ref() {
            let _ = host.update_display(display);
        }
    });
    execute_script(&format!(
        "window.statusDisplayChanged({{x:{},y:{},width:{},height:{},scale:{}}});",
        display.x, display.y, display.width, display.height, display.scale
    ));
    reconcile_recording_after_display_change(display);
}

fn reconcile_recording_after_display_change(display: super::DisplayMetrics) {
    let moved = {
        let mut scene = SCENE.lock().unwrap();
        scene.recording.as_mut().and_then(|recording| {
            let fitted = super::fit_rect_to_display(recording.rect, display);
            (fitted != recording.rect).then(|| {
                recording.rect = fitted;
                recording.clone()
            })
        })
    };
    if let Some(recording) = moved {
        sync_input_window(Some(&recording));
        if let Ok(rect) = serde_json::to_string(&recording.rect) {
            execute_script(&format!("window.moveStatusRecording({rect});"));
        }
        emit_event(ChildEvent::RecordingMoved {
            rect: recording.rect,
        });
    }
}
