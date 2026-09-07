use super::card_document::compositor_document;
use super::dcomp::{DcompHost, build_host};
use super::mailbox::{CommandBuffer, PushResult};
use super::protocol::{ChildEvent, HostCommand, SceneCard};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{LazyLock, Mutex};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MOUSE_EVENT_KIND, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS,
};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::System::Com::CoUninitialize;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

const WM_DRAIN_COMMANDS: u32 = WM_APP + 91;
const INPUT_TIMER_ID: usize = 1;
static HOST_HWND: AtomicIsize = AtomicIsize::new(0);
pub(super) static INPUT_SURFACE_HWND: AtomicIsize = AtomicIsize::new(0);
static RENDERER_READY: AtomicBool = AtomicBool::new(false);
static COMMANDS: LazyLock<Mutex<CommandBuffer>> =
    LazyLock::new(|| Mutex::new(CommandBuffer::default()));
pub(super) static CARDS: LazyLock<Mutex<HashMap<isize, SceneCard>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

thread_local! {
    pub(super) static HOST: RefCell<Option<DcompHost>> = const { RefCell::new(None) };
}

pub fn run() -> anyhow::Result<()> {
    crate::initialization::init_com_and_dpi();
    let visual_hwnd = create_host_window()?;
    HOST_HWND.store(visual_hwnd.0 as isize, Ordering::SeqCst);

    let origin = super::isolated_server::start()?;
    let compositor_html = compositor_document(&origin);
    super::isolated_server::set_compositor_html(compositor_html);
    let page_url = format!("{origin}/index.html");

    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) };
    let x = super::compositor_host_x(unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) }, width);
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let software = super::supervisor::software_rendering_requested();

    let dcomp_host = build_host(visual_hwnd, software, width, height, 1.0, &page_url)?;
    let input_hwnd = super::input_surface::create_input_surface(x, y, width, height)?;
    INPUT_SURFACE_HWND.store(input_hwnd.0 as isize, Ordering::SeqCst);

    HOST.with(|slot| *slot.borrow_mut() = Some(dcomp_host));

    unsafe {
        let _ = SetTimer(Some(visual_hwnd), INPUT_TIMER_ID, 100, None);
    }
    start_input_thread();

    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        HOST.with(|slot| *slot.borrow_mut() = None);
        let input_val = INPUT_SURFACE_HWND.swap(0, Ordering::SeqCst);
        if input_val != 0 {
            let _ = DestroyWindow(HWND(input_val as *mut std::ffi::c_void));
        }
        HOST_HWND.store(0, Ordering::SeqCst);
        CoUninitialize();
    }
    Ok(())
}

fn create_host_window() -> anyhow::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("SGTResultSceneCompositor");
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        let _ = RegisterClassW(&window_class);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        let x = super::compositor_host_x(GetSystemMetrics(SM_XVIRTUALSCREEN), width);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE
                | WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_NOREDIRECTIONBITMAP,
            class_name,
            w!("Result compositor"),
            WS_POPUP | WS_CLIPCHILDREN,
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
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        DwmExtendFrameIntoClientArea(hwnd, &margins)?;
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        Ok(hwnd)
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
    let hwnd_value = HOST_HWND.load(Ordering::SeqCst);
    if hwnd_value != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd_value as *mut std::ffi::c_void)),
                message,
                WPARAM(0),
                LPARAM(0),
            );
        }
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
            WM_TIMER if wparam.0 == INPUT_TIMER_ID => {
                poll_compositor_cursor();
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

pub(super) fn focus_webview() {
    HOST.with(|slot| {
        if let Some(host) = slot.borrow().as_ref() {
            host.move_focus();
        }
    });
}

pub(super) fn get_dcomp_cursor(cur: &mut HCURSOR) -> windows::core::Result<()> {
    HOST.with(|slot| {
        if let Some(host) = slot.borrow().as_ref() {
            host.cursor(cur)
        } else {
            Ok(())
        }
    })
}

pub(super) fn send_mouse_to_webview(
    kind: COREWEBVIEW2_MOUSE_EVENT_KIND,
    virtual_keys: COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS,
    mouse_data: u32,
    point: POINT,
) {
    HOST.with(|slot| {
        if let Some(host) = slot.borrow().as_ref() {
            host.send_mouse_input(kind, virtual_keys, mouse_data, point);
        }
    });
}

fn drain_commands(hwnd: HWND) {
    if !RENDERER_READY.load(Ordering::SeqCst) {
        return;
    }
    let mut scripts = Vec::new();
    let mut handled_command = false;
    let mut highest_revision = 0u64;
    let commands = COMMANDS.lock().unwrap().drain();
    for command in commands {
        if command == HostCommand::Shutdown {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
        handled_command = true;
        if let HostCommand::ApplyRevision { revision } = &command {
            highest_revision = highest_revision.max(*revision);
        }
        super::child_commands::apply(&command);
        if !matches!(command, HostCommand::ApplyRevision { .. })
            && let Ok(command_json) = serde_json::to_string(&command)
        {
            let script = format!(
                "try{{window.applyHostCommand({command_json});}}catch(error){{console.error(error);}}"
            );
            scripts.push(script);
        }
    }
    if handled_command {
        let input_val = INPUT_SURFACE_HWND.load(Ordering::SeqCst);
        let input_hwnd = if input_val != 0 {
            HWND(input_val as *mut std::ffi::c_void)
        } else {
            hwnd
        };
        let cards = CARDS.lock().unwrap();
        let button_regions = super::button_input::interactive_regions();
        let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) };
        let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) };
        let x = super::compositor_host_x(unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) }, width);
        let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let applied = super::input_surface::update_input_regions(
            input_hwnd,
            &cards,
            &button_regions,
            x,
            y,
            width,
            height,
        );
        drop(cards);
        if highest_revision > 0 {
            emit_event(ChildEvent::StateAcknowledged {
                revision: highest_revision,
                visible_cards: applied.visible_cards,
                input_rect_count: applied.input_region_count,
            });
        }
    }
    if !scripts.is_empty() {
        evaluate_script(&scripts.concat());
    }
}

pub(super) fn handle_renderer_event(body: &str) {
    let host_value = HOST_HWND.load(Ordering::SeqCst);
    if host_value != 0 {
        let host = HWND(host_value as *mut std::ffi::c_void);
        let outcome = {
            let cards = CARDS.lock().unwrap();
            let input = HWND(INPUT_SURFACE_HWND.load(Ordering::SeqCst) as *mut std::ffi::c_void);
            super::button_input::handle_renderer_message(body, input, &cards)
        };
        match outcome {
            super::button_input::RendererInput::Unhandled => {}
            super::button_input::RendererInput::Handled => return,
            super::button_input::RendererInput::RefreshRegion => {
                super::region::update(host, true);
                return;
            }
            super::button_input::RendererInput::FocusRefine { id } => {
                if !super::acceptance_offscreen() {
                    let input_val = INPUT_SURFACE_HWND.load(Ordering::SeqCst);
                    if input_val != 0 {
                        super::input_surface::activate_for_refine(HWND(input_val as *mut _));
                    }
                }
                evaluate_script(&format!(
                    "window.__SGT_REFINE_EDITOR__?.nativeFocusGranted('{}');",
                    id
                ));
                super::region::update(host, true);
                return;
            }
            super::button_input::RendererInput::ReleaseRefineFocus => {
                let input_val = INPUT_SURFACE_HWND.load(Ordering::SeqCst);
                if input_val != 0 {
                    super::input_surface::restore_nonactivating(HWND(input_val as *mut _));
                }
                super::region::update(host, true);
                return;
            }
            super::button_input::RendererInput::Event(event) => {
                emit_event(event);
                return;
            }
            super::button_input::RendererInput::EventAndRefresh(event) => {
                let started_gesture = match &event {
                    ChildEvent::DragStarted { gesture_id } => Some(*gesture_id),
                    _ => None,
                };
                if let Some(gesture_id) = started_gesture {
                    super::region::update(host, true);
                    evaluate_script(&format!(
                        "window.__SGT_BUTTON_SCENE__?.setDragActive(true,{gesture_id});"
                    ));
                }
                emit_event(event);
                if started_gesture.is_none() {
                    super::region::update(host, true);
                }
                return;
            }
        }
    }
    match body {
        "renderer_ready" => {
            RENDERER_READY.store(true, Ordering::SeqCst);
            emit_event(ChildEvent::Ready);
            let hwnd_value = HOST_HWND.load(Ordering::SeqCst);
            if hwnd_value != 0 {
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd_value as *mut std::ffi::c_void)),
                        WM_DRAIN_COMMANDS,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
        }
        "renderer_heartbeat" => emit_event(ChildEvent::Heartbeat),
        _ => {
            if let Ok(event) = serde_json::from_str::<ChildEvent>(body) {
                if let ChildEvent::CardDiagnostic { id, phase, .. } = &event
                    && phase == "interactive_document_alive"
                {
                    HOST.with(|slot| {
                        if let Some(host) = slot.borrow().as_ref() {
                            let webview = &host.webview;
                            super::acceptance_capture::capture_for_card(webview, *id);
                        }
                    });
                }
                match event {
                    ChildEvent::Navigation { .. }
                    | ChildEvent::NavigationRequest { .. }
                    | ChildEvent::Interaction { .. }
                    | ChildEvent::ButtonAction { .. }
                    | ChildEvent::DragStarted { .. }
                    | ChildEvent::DragFinished { .. }
                    | ChildEvent::ResizeFinished { .. }
                    | ChildEvent::FitDiagnostic { .. }
                    | ChildEvent::CardDiagnostic { .. }
                    | ChildEvent::FontReady { .. }
                    | ChildEvent::StateAcknowledged { .. }
                    | ChildEvent::CommandError { .. } => {
                        if let ChildEvent::CardDiagnostic { phase, .. } = &event
                            && phase == "final_fit_completed"
                        {
                            click_acceptance_link();
                        }
                        emit_event(event);
                    }
                    ChildEvent::Ready
                    | ChildEvent::Heartbeat
                    | ChildEvent::ResyncRequested
                    | ChildEvent::RendererFailure { .. } => {}
                }
            }
        }
    }
}

fn click_acceptance_link() {
    if !super::acceptance_offscreen() {
        return;
    }
    evaluate_script(
        "if(!window.__SGT_ACCEPTANCE_LINK_CLICKED__){for(const host of document.querySelectorAll('.result-card .direct-host')){const anchor=host.shadowRoot?.querySelector('a[href]');if(anchor){window.__SGT_ACCEPTANCE_LINK_CLICKED__=true;anchor.click();break;}}}",
    );
}

fn poll_compositor_cursor() {
    if let Some(event) = super::button_input::reconcile_released_pointer() {
        emit_event(event);
        let host_value = HOST_HWND.load(Ordering::SeqCst);
        if host_value != 0 {
            super::region::update(HWND(host_value as *mut std::ffi::c_void), true);
        }
        return;
    }
    if super::button_input::captures_desktop_input() || super::button_input::is_dragging() {
        return;
    }
    let mut cursor = windows::Win32::Foundation::POINT::default();
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return;
    }
    let virtual_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let virtual_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let script = format!(
        "window.updateCursorPosition?.({}/(window.devicePixelRatio||1),{}/(window.devicePixelRatio||1));",
        cursor.x - virtual_x,
        cursor.y - virtual_y
    );
    evaluate_script(&script);
}

fn evaluate_script(script: &str) {
    HOST.with(|slot| {
        if let Some(host) = slot.borrow().as_ref() {
            host.evaluate_script(script);
        }
    });
}

fn resize_host(hwnd: HWND) {
    unsafe {
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        let x = super::compositor_host_x(GetSystemMetrics(SM_XVIRTUALSCREEN), width);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE,
        );
        HOST.with(|slot| {
            if let Some(host) = slot.borrow().as_ref() {
                let _ = host.update_display(width, height, 1.0);
            }
        });
        let input_val = INPUT_SURFACE_HWND.load(Ordering::SeqCst);
        if input_val != 0 {
            super::input_surface::sync_input_surface_bounds(
                HWND(input_val as *mut std::ffi::c_void),
                x,
                y,
                width,
                height,
            );
        }
    }
}

static EVENT_TX: LazyLock<std::sync::mpsc::SyncSender<ChildEvent>> = LazyLock::new(|| {
    let (tx, rx) = std::sync::mpsc::sync_channel::<ChildEvent>(512);
    std::thread::Builder::new()
        .name("result-child-events".to_string())
        .spawn(move || {
            let mut stdout = std::io::stdout();
            while let Ok(event) = rx.recv() {
                if let Ok(line) = serde_json::to_string(&event)
                    && writeln!(stdout, "{line}")
                        .and_then(|_| stdout.flush())
                        .is_err()
                {
                    std::process::exit(1);
                }
            }
        })
        .expect("event thread spawn");
    tx
});

pub(super) fn emit_event(event: ChildEvent) {
    if EVENT_TX.try_send(event).is_err() {
        // Lost lifecycle events invalidate the input contract. Ending this owned
        // child removes its HWNDs and lets the parent supervisor resynchronize.
        std::process::exit(1);
    }
}

#[cfg(test)]
#[path = "child_tests.rs"]
mod tests;
