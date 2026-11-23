use super::card_document::compositor_document;
use super::dcomp::{DcompHost, build_host};
use super::mailbox::{CommandBuffer, PushResult};
use super::protocol::{ChildEvent, HostCommand, SceneCard};
use serde::Deserialize;
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
#[path = "command_delivery.rs"]
mod command_delivery;
#[path = "cursor_poll.rs"]
mod cursor_poll;
#[path = "renderer_event_gate.rs"]
mod renderer_event_gate;
#[path = "topology.rs"]
pub(super) mod topology;
#[path = "topology_transition.rs"]
pub(super) mod topology_transition;
const WM_REFRESH_VISUAL: u32 = WM_APP + 92;
const WM_RECONCILE_STACK: u32 = WM_APP + 93;
static STACK_RECONCILIATION_PENDING: AtomicBool = AtomicBool::new(false);
static VISUAL_REFRESH_PENDING: AtomicBool = AtomicBool::new(false);
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
    let mut initial_rect = windows::Win32::Foundation::RECT::default();
    unsafe { GetWindowRect(visual_hwnd, &mut initial_rect)? };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) };
    let x = super::compositor_host_x(unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) }, width);
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    anyhow::ensure!(
        topology::matches_requested_bounds(initial_rect, x, y, width, height),
        "compositor host bounds mismatch"
    );
    topology::set_size(width, height);
    topology::set_origin(topology::coordinate_origin(
        topology::Origin {
            x: initial_rect.left,
            y: initial_rect.top,
        },
        topology::Origin {
            x: unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) },
            y: unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) },
        },
        super::acceptance_offscreen(),
    ));
    HOST_HWND.store(visual_hwnd.0 as isize, Ordering::SeqCst);

    let resource_server = super::isolated_server::start()?;
    let origin = resource_server.origin();
    let compositor_html = compositor_document();
    resource_server.set_html(compositor_html);
    let page_url = format!("{origin}/index.html");

    let software = super::supervisor::software_rendering_requested();

    let dcomp_host = build_host(visual_hwnd, software, width, height, 1.0, &page_url)?;
    let input_hwnd = super::input_surface::create_input_surface(x, y, width, height)?;
    INPUT_SURFACE_HWND.store(input_hwnd.0 as isize, Ordering::SeqCst);

    HOST.with(|slot| *slot.borrow_mut() = Some(dcomp_host));

    unsafe {
        if SetTimer(Some(visual_hwnd), INPUT_TIMER_ID, 100, None) == 0 {
            crate::crash_report::exit(1, "renderer-input-timer-failed");
        }
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

pub(super) fn create_host_window() -> anyhow::Result<HWND> {
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
        )
        .and_then(crate::overlay::shell_policy::prepare)?;
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        DwmExtendFrameIntoClientArea(hwnd, &margins)?;
        super::visual_region::hide(hwnd);
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
            WM_RECONCILE_STACK => {
                STACK_RECONCILIATION_PENDING.store(false, Ordering::SeqCst);
                emit_event(ChildEvent::StackChanged);
                LRESULT(0)
            }
            WM_REFRESH_VISUAL => {
                VISUAL_REFRESH_PENDING.store(false, Ordering::SeqCst);
                refresh_visual_region();
                LRESULT(0)
            }
            WM_DRAIN_COMMANDS => {
                drain_commands(hwnd);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
                if HOST_HWND.load(Ordering::SeqCst) != 0 {
                    topology_transition::begin(hwnd);
                }
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == INPUT_TIMER_ID => {
                HOST.with(|slot| {
                    if let Some(host) = slot.borrow().as_ref() {
                        host.poll_heartbeat();
                    }
                });
                topology_transition::tick(hwnd);
                if !topology_transition::active() {
                    poll_compositor_cursor();
                }
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

pub(super) fn set_renderer_visible(hwnd: HWND, visible: bool) -> bool {
    if HOST_HWND.load(Ordering::SeqCst) != hwnd.0 as isize {
        return true;
    }
    HOST.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_none_or(|host| host.set_visible(visible).is_ok())
    })
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
    command_delivery::drain(hwnd);
}

pub(super) fn handle_renderer_event(body: &str) {
    let message = serde_json::from_str::<serde_json::Value>(body).ok();
    if !renderer_event_gate::accept(message.as_ref()) {
        return;
    }
    if let Some(message) = message.as_ref()
        && message.get("action").and_then(|action| action.as_str())
            == Some("control_tooltip_region")
    {
        let rect = message
            .get("rect")
            .and_then(|rect| super::protocol::SceneRect::deserialize(rect).ok())
            .filter(|rect| rect.width > 0 && rect.height > 0);
        if super::visual_region::set_tooltip(rect) {
            request_visual_region();
        }
        return;
    }
    if let Some(token) = body
        .strip_prefix("scene_painted:")
        .and_then(|value| value.parse().ok())
    {
        if !topology_transition::active() && command_delivery::current_paint_token(token) {
            super::visual_region::mark_first_scene_painted();
            let visual = HWND(HOST_HWND.load(Ordering::SeqCst) as _);
            if !visual.is_invalid() {
                super::region::update(visual);
            }
        }
        return;
    }
    let host_value = HOST_HWND.load(Ordering::SeqCst);
    if host_value != 0
        && message
            .as_ref()
            .is_some_and(|message| message.get("action").is_some())
    {
        let host = HWND(host_value as *mut std::ffi::c_void);
        let outcome = {
            let region_only = matches!(
                message
                    .as_ref()
                    .and_then(|value| value.get("action"))
                    .and_then(|value| value.as_str()),
                Some(
                    "update_clickable_regions"
                        | "restore_clickable_regions"
                        | "processing_control_regions"
                        | "release_refine_focus"
                )
            );
            let cards = if region_only {
                HashMap::new()
            } else {
                CARDS.lock().unwrap().clone()
            };
            let input = HWND(INPUT_SURFACE_HWND.load(Ordering::SeqCst) as *mut std::ffi::c_void);
            super::button_input::handle_renderer_message(body, input, &cards)
        };
        match outcome {
            super::button_input::RendererInput::Unhandled => {}
            super::button_input::RendererInput::Handled => return,
            super::button_input::RendererInput::RefreshRegion => {
                super::region::update(host);
                return;
            }
            super::button_input::RendererInput::FocusRefine { id, session } => {
                if !super::acceptance_offscreen() {
                    let input_val = INPUT_SURFACE_HWND.load(Ordering::SeqCst);
                    if input_val != 0 {
                        super::input_surface::activate_for_refine(HWND(input_val as *mut _));
                    }
                }
                let epoch = message
                    .as_ref()
                    .and_then(|value| value.get("topology_epoch"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let epoch = serde_json::to_string(epoch).expect("epoch must serialize");
                evaluate_script(&format!(
                    "window.__SGT_REFINE_EDITOR__?.nativeFocusGranted('{}', '{}', {});",
                    id, session, epoch
                ));
                super::region::update(host);
                return;
            }
            super::button_input::RendererInput::ReleaseRefineFocus => {
                let input_val = INPUT_SURFACE_HWND.load(Ordering::SeqCst);
                if input_val != 0 {
                    super::input_surface::restore_nonactivating(HWND(input_val as *mut _));
                }
                super::region::update(host);
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
                    super::region::update(host);
                    evaluate_script(&format!(
                        "window.__SGT_BUTTON_SCENE__?.setDragActive(true,{gesture_id});"
                    ));
                }
                emit_event(event);
                if started_gesture.is_none() {
                    super::region::update(host);
                }
                return;
            }
        }
    }
    match body {
        "renderer_ready" => {
            RENDERER_READY.store(true, Ordering::SeqCst);
            let origin = topology::origin();
            evaluate_script(&super::control_surface::monitor_work_areas_script((
                origin.x, origin.y,
            )));
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
                    ChildEvent::StateAcknowledged { .. } if topology_transition::active() => {
                        topology_transition::hold_ack(event);
                    }
                    ChildEvent::ProcessingFinished { id } => {
                        super::processing::child_finished(id);
                        super::region::update(HWND(HOST_HWND.load(Ordering::SeqCst) as _));
                        emit_event(event);
                    }
                    ChildEvent::ProcessingDiagnostic { .. }
                    | ChildEvent::SourceImageReady { .. }
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
                    | ChildEvent::StackChanged
                    | ChildEvent::Heartbeat
                    | ChildEvent::ResyncRequested
                    | ChildEvent::TopologyHold { .. }
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
        cursor_poll::invalidate();
        emit_event(event);
        let host_value = HOST_HWND.load(Ordering::SeqCst);
        if host_value != 0 {
            super::region::update(HWND(host_value as *mut std::ffi::c_void));
        }
        return;
    }
    cursor_poll::poll(
        RENDERER_READY.load(Ordering::SeqCst)
            && !super::button_input::captures_desktop_input()
            && !super::button_input::is_dragging(),
    );
}

fn evaluate_script(script: &str) {
    HOST.with(|slot| {
        if let Some(host) = slot.borrow().as_ref() {
            host.evaluate_script(script);
        }
    });
}

pub(super) fn request_visual_region() {
    if !VISUAL_REFRESH_PENDING.swap(true, Ordering::SeqCst) {
        post_host(WM_REFRESH_VISUAL);
    }
}

pub(super) fn request_stack_reconciliation() {
    if HOST_HWND.load(Ordering::SeqCst) != 0
        && !STACK_RECONCILIATION_PENDING.swap(true, Ordering::SeqCst)
    {
        post_host(WM_RECONCILE_STACK);
    }
}

fn refresh_visual_region() {
    if topology_transition::active() {
        return;
    }
    let value = HOST_HWND.load(Ordering::SeqCst);
    if value == 0 {
        return;
    }
    super::region::update(HWND(value as *mut _));
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
                    crate::crash_report::exit(1, "renderer-event-output-failed");
                }
            }
        })
        .expect("event thread spawn");
    tx
});

pub(super) fn emit_event(mut event: ChildEvent) {
    if let ChildEvent::ResizeFinished { rect, .. } = &mut event {
        let origin = topology::origin();
        let Some(x) = rect.x.checked_add(origin.x) else {
            crate::crash_report::exit(1, "renderer-coordinate-overflow");
        };
        let Some(y) = rect.y.checked_add(origin.y) else {
            crate::crash_report::exit(1, "renderer-coordinate-overflow");
        };
        rect.x = x;
        rect.y = y;
    }
    if EVENT_TX.try_send(event).is_err() {
        // Lost lifecycle events invalidate the input contract. Ending this owned
        // child removes its HWNDs and lets the parent supervisor resynchronize.
        crate::crash_report::exit(1, "renderer-event-queue-full");
    }
}

#[cfg(test)]
#[path = "child_tests.rs"]
mod tests;
